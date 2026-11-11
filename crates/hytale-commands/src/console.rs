//! Console command loop.
//!
//! The console runs the same registry as in-game chat. The server supplies a
//! [`ConsoleEffects`] implementation that owns the world thread, player
//! registry and shutdown signal; tests can supply a recording implementation.

use std::io::{BufRead, Write};

use crate::command::{CommandContext, CommandEffect, CommandSender};
use crate::registry::CommandRegistry;

/// Prompt printed before each interactive console line.
pub const CONSOLE_PROMPT: &str = "hytale> ";

/// Runtime operations a console command can request.
pub trait ConsoleEffects {
    /// The current command context (players, world, time, limits).
    fn context(&mut self) -> CommandContext;
    /// Persist the world and players.
    fn save_world(&mut self) -> Result<String, String>;
    /// Request a graceful shutdown.
    fn stop_server(&mut self) -> Result<String, String>;
    /// Broadcast a chat line to every player.
    fn broadcast(&mut self, message: &str);
    /// Disconnect a player.
    fn kick(&mut self, target: &str, reason: Option<&str>) -> Result<String, String>;
    /// Switch a player (or the console) to another world.
    fn switch_world(&mut self, world: &str) -> Result<String, String>;
    /// Authoritatively replace one block.
    fn set_block(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        block: &str,
        rotation: Option<i32>,
    ) -> Result<String, String>;
    /// Apply/report an effect the console cannot perform directly.
    fn unsupported(&mut self, effect: &CommandEffect) -> Result<String, String>;
}

/// Execute one console line and return the messages to print.
///
/// A leading `/` is optional; blank lines produce no output.
pub fn execute_console_line(
    registry: &CommandRegistry,
    line: &str,
    effects: &mut impl ConsoleEffects,
) -> Vec<String> {
    let line = line.trim();
    if line.is_empty() {
        return Vec::new();
    }
    let line = line.strip_prefix('/').unwrap_or(line);
    let sender = CommandSender::console();
    let context = effects.context();
    let mut output = Vec::new();
    match registry.execute(line, &sender, &context) {
        Ok(result) => {
            output.extend(result.messages);
            for effect in result.effects {
                let message = match effect {
                    CommandEffect::SaveWorld => effects.save_world(),
                    CommandEffect::StopServer => effects.stop_server(),
                    CommandEffect::Broadcast { message } => {
                        effects.broadcast(&message);
                        Ok(String::new())
                    }
                    CommandEffect::Kick { target, reason } => {
                        effects.kick(&target, reason.as_deref())
                    }
                    CommandEffect::SwitchWorld { world, .. } => effects.switch_world(&world),
                    CommandEffect::SetBlock {
                        x,
                        y,
                        z,
                        block,
                        rotation,
                    } => effects.set_block(x, y, z, &block, rotation),
                    other => effects.unsupported(&other),
                };
                match message {
                    Ok(message) if !message.is_empty() => output.push(message),
                    Ok(_) => {}
                    Err(error) => output.push(error),
                }
            }
        }
        Err(error) => output.push(error.to_string()),
    }
    output
}

/// Read commands until EOF, printing each result.
pub fn run_console<R: BufRead, W: Write>(
    registry: &CommandRegistry,
    reader: R,
    effects: &mut impl ConsoleEffects,
    writer: W,
) -> std::io::Result<()> {
    run_console_with_prompt(registry, reader, effects, writer, None)
}

/// Read commands until EOF, printing an optional prompt before every line.
pub fn run_console_with_prompt<R: BufRead, W: Write>(
    registry: &CommandRegistry,
    mut reader: R,
    effects: &mut impl ConsoleEffects,
    mut writer: W,
    prompt: Option<&str>,
) -> std::io::Result<()> {
    let mut line = String::new();
    loop {
        if let Some(prompt) = prompt {
            write!(writer, "{prompt}")?;
            writer.flush()?;
        }
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            if prompt.is_some() {
                writeln!(writer)?;
                writer.flush()?;
            }
            return Ok(());
        }
        for message in execute_console_line(registry, &line, effects) {
            writeln!(writer, "{message}")?;
        }
        writer.flush()?;
    }
}

#[cfg(test)]
mod tests {
    use super::{execute_console_line, run_console, run_console_with_prompt, ConsoleEffects};
    use crate::command::{CommandContext, CommandEffect};
    use crate::registry::CommandRegistry;

    #[derive(Default)]
    struct Recorder {
        saved: usize,
        stopped: usize,
        broadcasts: Vec<String>,
        kicks: Vec<(String, Option<String>)>,
        worlds: Vec<String>,
        set_blocks: Vec<(i32, i32, i32, String, Option<i32>)>,
        unsupported: Vec<String>,
    }

    impl ConsoleEffects for Recorder {
        fn context(&mut self) -> CommandContext {
            CommandContext {
                players: vec!["Alice".to_string()],
                world_name: Some("Default".to_string()),
                max_players: Some(20),
                max_view_radius: Some(16),
                ..CommandContext::default()
            }
        }

        fn save_world(&mut self) -> Result<String, String> {
            self.saved += 1;
            Ok("Saved world".to_string())
        }

        fn stop_server(&mut self) -> Result<String, String> {
            self.stopped += 1;
            Ok("Server shutdown requested".to_string())
        }

        fn broadcast(&mut self, message: &str) {
            self.broadcasts.push(message.to_string());
        }

        fn kick(&mut self, target: &str, reason: Option<&str>) -> Result<String, String> {
            self.kicks
                .push((target.to_string(), reason.map(str::to_string)));
            Ok(format!("Kicked {target}"))
        }

        fn switch_world(&mut self, world: &str) -> Result<String, String> {
            self.worlds.push(world.to_string());
            Ok(format!("Switched to {world}"))
        }

        fn set_block(
            &mut self,
            x: i32,
            y: i32,
            z: i32,
            block: &str,
            rotation: Option<i32>,
        ) -> Result<String, String> {
            self.set_blocks.push((x, y, z, block.to_string(), rotation));
            Ok(format!("Set block ({x}, {y}, {z}) to {block}"))
        }

        fn unsupported(&mut self, effect: &CommandEffect) -> Result<String, String> {
            self.unsupported.push(format!("{effect:?}"));
            Err("not available from the console".to_string())
        }
    }

    #[test]
    fn console_save_and_stop_apply_effects() {
        let registry = CommandRegistry::builtin();
        let mut recorder = Recorder::default();
        let messages = execute_console_line(&registry, "/save", &mut recorder);
        assert_eq!(messages, ["Saved world"]);
        assert_eq!(recorder.saved, 1);
        let messages = execute_console_line(&registry, "stop", &mut recorder);
        assert_eq!(recorder.stopped, 1);
        assert!(messages.iter().any(|message| message.contains("shutdown")));
    }

    #[test]
    fn console_say_kick_and_world_route_to_the_server() {
        let registry = CommandRegistry::builtin();
        let mut recorder = Recorder::default();
        execute_console_line(&registry, "say hello", &mut recorder);
        assert_eq!(recorder.broadcasts, ["hello"]);
        execute_console_line(&registry, "kick Alice bye", &mut recorder);
        assert_eq!(
            recorder.kicks,
            [("Alice".to_string(), Some("bye".to_string()))]
        );
        execute_console_line(&registry, "world Nether", &mut recorder);
        assert_eq!(recorder.worlds, ["Nether"]);
        execute_console_line(&registry, "setblock 1 64 -2 Rock_Stone 90", &mut recorder);
        assert_eq!(
            recorder.set_blocks,
            [(1, 64, -2, "Rock_Stone".to_string(), Some(1))]
        );
    }

    #[test]
    fn console_reports_unknown_commands_and_unsupported_effects() {
        let registry = CommandRegistry::builtin();
        let mut recorder = Recorder::default();
        let messages = execute_console_line(&registry, "nope", &mut recorder);
        assert!(messages[0].contains("unknown command"));
        let messages = execute_console_line(&registry, "tp 1 2 3", &mut recorder);
        assert_eq!(messages, ["not available from the console"]);
        assert_eq!(recorder.unsupported.len(), 1);
    }

    #[test]
    fn blank_lines_produce_no_output() {
        let registry = CommandRegistry::builtin();
        let mut recorder = Recorder::default();
        assert!(execute_console_line(&registry, "   ", &mut recorder).is_empty());
    }

    #[test]
    fn interactive_runner_prints_a_prompt_and_a_final_newline() {
        let registry = CommandRegistry::builtin();
        let mut recorder = Recorder::default();
        let mut output = Vec::new();
        run_console_with_prompt(
            &registry,
            std::io::Cursor::new(b"version\n".to_vec()),
            &mut recorder,
            &mut output,
            Some(super::CONSOLE_PROMPT),
        )
        .expect("console");
        let text = String::from_utf8(output).expect("utf8");
        assert!(
            text.starts_with("hytale> hytale-server-rust"),
            "unexpected prompt output: {text:?}"
        );
        assert!(
            text.ends_with("hytale> \n"),
            "the EOF prompt needs a newline: {text:?}"
        );
    }

    #[test]
    fn piped_runner_prints_results_without_a_prompt() {
        let registry = CommandRegistry::builtin();
        let mut recorder = Recorder::default();
        let mut output = Vec::new();
        run_console(
            &registry,
            std::io::Cursor::new(b"version\n".to_vec()),
            &mut recorder,
            &mut output,
        )
        .expect("console");
        let text = String::from_utf8(output).expect("utf8");
        assert!(text.starts_with("hytale-server-rust"), "{text:?}");
        assert!(
            !text.contains("hytale>"),
            "pipe output must stay clean: {text:?}"
        );
    }
}
