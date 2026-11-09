//! Built-in commands for the console and in-game chat.

use crate::command::{CommandEffect, CommandError, CommandInvocation};
use crate::registry::{ArgKind, ArgSpec, Command, CommandRegistry};

const TP_ARGS: &[ArgSpec] = &[
    ArgSpec {
        name: "x",
        kind: ArgKind::Float,
        description: "X coordinate",
    },
    ArgSpec {
        name: "y",
        kind: ArgKind::Float,
        description: "Y coordinate",
    },
    ArgSpec {
        name: "z",
        kind: ArgKind::Float,
        description: "Z coordinate",
    },
];

const GIVE_ARGS: &[ArgSpec] = &[
    ArgSpec {
        name: "item",
        kind: ArgKind::Item,
        description: "Item asset id",
    },
    ArgSpec {
        name: "quantity",
        kind: ArgKind::Integer,
        description: "Quantity (default 1)",
    },
];

const GAMEMODE_ARGS: &[ArgSpec] = &[ArgSpec {
    name: "mode",
    kind: ArgKind::GameMode,
    description: "adventure or creative",
}];

/// Build the built-in command registry.
pub fn registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    for command in commands() {
        registry
            .register(command)
            .expect("built-in command names and aliases are unique");
    }
    registry
}

fn commands() -> Vec<Command> {
    vec![
        Command {
            name: "help",
            aliases: &["?"],
            description: "List available commands",
            usage: "help [command]",
            permission: None,
            args: &[],
            handler: help,
        },
        Command {
            name: "tp",
            aliases: &["teleport"],
            description: "Teleport to absolute coordinates",
            usage: "tp <x> <y> <z>",
            permission: None,
            args: TP_ARGS,
            handler: tp,
        },
        Command {
            name: "give",
            aliases: &["item"],
            description: "Give yourself an item",
            usage: "give <item> [quantity]",
            permission: None,
            args: GIVE_ARGS,
            handler: give,
        },
        Command {
            name: "gamemode",
            aliases: &["gm"],
            description: "Change your game mode",
            usage: "gamemode <adventure|creative>",
            permission: None,
            args: GAMEMODE_ARGS,
            handler: gamemode,
        },
        Command {
            name: "save",
            aliases: &[],
            description: "Save the world and your player",
            usage: "save",
            permission: None,
            args: &[],
            handler: save,
        },
        Command {
            name: "stop",
            aliases: &[],
            description: "Save and shut down the server",
            usage: "stop",
            permission: Some("server.commands.stop"),
            args: &[],
            handler: stop,
        },
        Command {
            name: "whereami",
            aliases: &["pos"],
            description: "Show your current position",
            usage: "whereami",
            permission: None,
            args: &[],
            handler: whereami,
        },
        Command {
            name: "whoami",
            aliases: &[],
            description: "Show your identity",
            usage: "whoami",
            permission: None,
            args: &[],
            handler: whoami,
        },
        Command {
            name: "who",
            aliases: &["list"],
            description: "List connected players",
            usage: "who",
            permission: None,
            args: &[],
            handler: who,
        },
        Command {
            name: "kill",
            aliases: &[],
            description: "Kill yourself",
            usage: "kill",
            permission: None,
            args: &[],
            handler: kill,
        },
        Command {
            name: "respawn",
            aliases: &[],
            description: "Respawn after death",
            usage: "respawn",
            permission: None,
            args: &[],
            handler: respawn,
        },
        Command {
            name: "time",
            aliases: &[],
            description: "Show the current game time",
            usage: "time",
            permission: None,
            args: &[],
            handler: time,
        },
        Command {
            name: "version",
            aliases: &[],
            description: "Show the server version",
            usage: "version",
            permission: None,
            args: &[],
            handler: version,
        },
    ]
}

fn help(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    invocation.no_extra_args(1)?;
    if let Some(filter) = invocation.arg(0) {
        let prefix = format!("/{filter}");
        let matches: Vec<&str> = invocation
            .help
            .iter()
            .map(String::as_str)
            .filter(|line| line.starts_with(&prefix))
            .collect();
        if matches.is_empty() {
            invocation.reply(format!("No command matches `/{filter}`"));
        } else {
            invocation.reply(matches.join("\n"));
        }
    } else if invocation.help.is_empty() {
        invocation.reply("No commands are available");
    } else {
        invocation.reply(invocation.help.join("\n"));
    }
    Ok(())
}

fn tp(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    let x = invocation.required_f64(0, "x")?;
    let y = invocation.required_f64(1, "y")?;
    let z = invocation.required_f64(2, "z")?;
    invocation.no_extra_args(3)?;
    invocation.effect(CommandEffect::Teleport { x, y, z });
    Ok(())
}

fn give(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    let item_id = invocation.required(0, "item")?.to_string();
    let quantity = if invocation.arg(1).is_some() {
        invocation.required_i32(1, "quantity")?
    } else {
        1
    };
    invocation.no_extra_args(2)?;
    if quantity <= 0 {
        return Err(CommandError::Message(
            "quantity must be greater than zero".to_string(),
        ));
    }
    invocation.effect(CommandEffect::GiveItem { item_id, quantity });
    Ok(())
}

fn gamemode(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    let mode = invocation.required(0, "mode")?.to_ascii_lowercase();
    let normalized = match mode.as_str() {
        "a" | "adventure" => "adventure",
        "c" | "creative" => "creative",
        _ => {
            return Err(CommandError::Message(
                "game mode must be `adventure` or `creative`".to_string(),
            ))
        }
    };
    invocation.no_extra_args(1)?;
    invocation.effect(CommandEffect::SetGameMode {
        mode: normalized.to_string(),
    });
    Ok(())
}

fn save(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    invocation.no_extra_args(0)?;
    invocation.effect(CommandEffect::SaveWorld);
    Ok(())
}

fn stop(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    invocation.no_extra_args(0)?;
    invocation.effect(CommandEffect::SaveWorld);
    invocation.effect(CommandEffect::StopServer);
    Ok(())
}

fn whereami(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    invocation.no_extra_args(0)?;
    let [x, y, z] = invocation.context.position;
    invocation.reply(format!("Position: {x:.2}, {y:.2}, {z:.2}"));
    Ok(())
}

fn whoami(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    invocation.no_extra_args(0)?;
    invocation.reply(format!(
        "{} ({})",
        invocation.sender.name,
        match invocation.sender.kind {
            crate::command::SenderKind::Console => "console",
            crate::command::SenderKind::Player => "player",
        }
    ));
    Ok(())
}

fn who(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    invocation.no_extra_args(0)?;
    if invocation.context.players.is_empty() {
        invocation.reply("No players are connected");
    } else {
        invocation.reply(format!(
            "{} player(s): {}",
            invocation.context.players.len(),
            invocation.context.players.join(", ")
        ));
    }
    Ok(())
}

fn kill(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    invocation.no_extra_args(0)?;
    invocation.effect(CommandEffect::Kill);
    Ok(())
}

fn respawn(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    invocation.no_extra_args(0)?;
    invocation.effect(CommandEffect::Respawn);
    Ok(())
}

fn time(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    invocation.no_extra_args(0)?;
    match invocation.context.game_time_seconds {
        Some(seconds) => {
            let paused = if invocation.context.time_paused.unwrap_or(false) {
                " (paused)"
            } else {
                ""
            };
            invocation.reply(format!("Game time: {seconds}s{paused}"));
        }
        None => invocation.reply("Game time is unavailable"),
    }
    Ok(())
}

fn version(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    invocation.no_extra_args(0)?;
    invocation.reply(format!("hytale-server-rust {}", env!("CARGO_PKG_VERSION")));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::registry;
    use crate::command::{CommandContext, CommandEffect, CommandSender};

    #[test]
    fn built_in_names_and_aliases_resolve() {
        let registry = registry();
        for name in ["help", "tp", "give", "gamemode", "save", "stop", "who"] {
            assert!(registry.resolve(name).is_some(), "missing command {name}");
        }
        assert_eq!(registry.resolve("gm").expect("alias").name, "gamemode");
        assert_eq!(registry.resolve("?").expect("alias").name, "help");
    }

    #[test]
    fn tp_produces_a_teleport_effect() {
        let registry = registry();
        let output = registry
            .execute(
                "/tp 12.5 64 -3.25",
                &CommandSender::player("Bob"),
                &CommandContext::default(),
            )
            .expect("tp");
        assert_eq!(
            output.effects,
            [CommandEffect::Teleport {
                x: 12.5,
                y: 64.0,
                z: -3.25
            }]
        );
    }

    #[test]
    fn give_validates_the_quantity() {
        let registry = registry();
        let sender = CommandSender::player("Bob");
        let context = CommandContext::default();
        assert_eq!(
            registry
                .execute("give Rock 0", &sender, &context)
                .expect_err("quantity")
                .to_string(),
            "quantity must be greater than zero"
        );
        assert_eq!(
            registry
                .execute("give Rock 3", &sender, &context)
                .expect("give")
                .effects,
            [CommandEffect::GiveItem {
                item_id: "Rock".to_string(),
                quantity: 3
            }]
        );
    }

    #[test]
    fn help_lists_commands_and_filters() {
        let registry = registry();
        let sender = CommandSender::player("Bob");
        let context = CommandContext::default();
        let output = registry
            .execute("help tp", &sender, &context)
            .expect("help");
        assert!(output.messages[0].contains("/tp - "));
        assert!(!output.messages[0].contains("/give - "));
    }

    #[test]
    fn stop_requires_operator_permission() {
        let registry = registry();
        let player = CommandSender::player("Bob");
        let context = CommandContext::default();
        assert!(registry.execute("stop", &player, &context).is_err());
        assert!(registry
            .execute("stop", &player.operator(), &context)
            .is_ok());
    }
}
