//! Built-in commands for the console and in-game chat.

use crate::command::{CommandEffect, CommandError, CommandInvocation};
use crate::message::{parse_message, translated_message, with_color};
use crate::registry::{ArgKind, ArgSpec, Command, CommandRegistry};

const TP_ARGS: &[ArgSpec] = &[
    ArgSpec {
        name: "target",
        kind: ArgKind::Player,
        description: "Player to move (default: you)",
    },
    ArgSpec {
        name: "x",
        kind: ArgKind::Float,
        description: "X coordinate, or a player name",
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
        name: "player",
        kind: ArgKind::Player,
        description: "Receiving player (default: you)",
    },
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

const GAMEMODE_ARGS: &[ArgSpec] = &[
    ArgSpec {
        name: "player",
        kind: ArgKind::Player,
        description: "Player to change (default: you)",
    },
    ArgSpec {
        name: "mode",
        kind: ArgKind::GameMode,
        description: "adventure or creative",
    },
];

const SETBLOCK_ARGS: &[ArgSpec] = &[
    ArgSpec {
        name: "x",
        kind: ArgKind::Integer,
        description: "Block X coordinate",
    },
    ArgSpec {
        name: "y",
        kind: ArgKind::Integer,
        description: "Block Y coordinate",
    },
    ArgSpec {
        name: "z",
        kind: ArgKind::Integer,
        description: "Block Z coordinate",
    },
    ArgSpec {
        name: "block",
        kind: ArgKind::String,
        description: "Block id or block type key",
    },
    ArgSpec {
        name: "rotation",
        kind: ArgKind::Integer,
        description: "0/90/180/270 or a 0..63 tuple index",
    },
];

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
            description: "Teleport yourself or another player",
            usage: "tp [player] <x> <y> <z> | tp <player> [destination]",
            permission: None,
            args: TP_ARGS,
            handler: tp,
        },
        Command {
            name: "give",
            aliases: &["item"],
            description: "Give an item to yourself or another player",
            usage: "give [player] <item> [quantity]",
            permission: None,
            args: GIVE_ARGS,
            handler: give,
        },
        Command {
            name: "gamemode",
            aliases: &["gm"],
            description: "Change a player's game mode",
            usage: "gamemode [player] <adventure|creative>",
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
            description: "Kill yourself or another player",
            usage: "kill [player]",
            permission: None,
            args: &[],
            handler: kill,
        },
        Command {
            name: "respawn",
            aliases: &[],
            description: "Respawn yourself or another player",
            usage: "respawn [player]",
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
        Command {
            name: "say",
            aliases: &["broadcast"],
            description: "Send a chat message to every player",
            usage: "say <message>",
            permission: Some("server.commands.say"),
            args: &[],
            handler: say,
        },
        Command {
            name: "kick",
            aliases: &[],
            description: "Disconnect a player",
            usage: "kick <player> [reason]",
            permission: Some("server.commands.kick"),
            args: &[],
            handler: kick,
        },
        Command {
            name: "world",
            aliases: &["universe"],
            description: "Show or switch the current world",
            usage: "world [name]",
            permission: Some("server.commands.world"),
            args: &[],
            handler: world,
        },
        Command {
            name: "maxplayers",
            aliases: &[],
            description: "Show or change the maximum player count",
            usage: "maxplayers [amount]",
            permission: Some("server.commands.maxplayers"),
            args: &[],
            handler: maxplayers,
        },
        Command {
            name: "maxviewradius",
            aliases: &[],
            description: "Show or change the maximum view radius",
            usage: "maxviewradius [radius]",
            permission: Some("server.commands.maxviewradius"),
            args: &[],
            handler: maxviewradius,
        },
        Command {
            name: "setblock",
            aliases: &["sb"],
            description: "Place or replace a block",
            usage: "setblock <x> <y> <z> <block> [rotation]",
            permission: Some("server.commands.setblock"),
            args: SETBLOCK_ARGS,
            handler: setblock,
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

fn parse_coords(
    invocation: &CommandInvocation<'_>,
    start: usize,
) -> Result<(f64, f64, f64), CommandError> {
    let x = invocation.required_f64(start, "x")?;
    let y = invocation.required_f64(start + 1, "y")?;
    let z = invocation.required_f64(start + 2, "z")?;
    Ok((x, y, z))
}

fn tp(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    let args = invocation.raw_args().to_vec();
    match args.len() {
        // `/tp <player>`: teleport yourself to that player.
        1 => {
            let destination = invocation
                .context
                .player_position(&args[0])
                .ok_or_else(|| CommandError::UnknownPlayer(args[0].clone()))?;
            invocation.effect(CommandEffect::Teleport {
                target: None,
                x: destination[0],
                y: destination[1],
                z: destination[2],
            });
        }
        // `/tp <player> <destination>`: move one player to another.
        2 => {
            let destination = invocation
                .context
                .player_position(&args[1])
                .ok_or_else(|| CommandError::UnknownPlayer(args[1].clone()))?;
            invocation.authorize_target(&args[0])?;
            invocation.effect(CommandEffect::Teleport {
                target: Some(args[0].clone()),
                x: destination[0],
                y: destination[1],
                z: destination[2],
            });
        }
        // `/tp <x> <y> <z>`: teleport yourself to coordinates.
        3 => {
            let (x, y, z) = parse_coords(invocation, 0)?;
            invocation.effect(CommandEffect::Teleport {
                target: None,
                x,
                y,
                z,
            });
        }
        // `/tp <player> <x> <y> <z>`: move one player to coordinates.
        4 => {
            let (x, y, z) = parse_coords(invocation, 1)?;
            invocation.authorize_target(&args[0])?;
            invocation.effect(CommandEffect::Teleport {
                target: Some(args[0].clone()),
                x,
                y,
                z,
            });
        }
        _ => return Err(CommandError::Usage(invocation.usage.clone())),
    }
    Ok(())
}

fn give(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    let args = invocation.raw_args().to_vec();
    let (target, item_index, quantity) = match args.len() {
        1 => (None, 0, 1),
        2 => {
            // `give <player> <item>` when the first token is a player and the
            // second is not a quantity; otherwise `give <item> <quantity>`.
            if invocation.context.has_player(&args[0]) && args[1].parse::<i32>().is_err() {
                (Some(args[0].clone()), 1, 1)
            } else {
                let quantity = args[1]
                    .parse::<i32>()
                    .map_err(|_| CommandError::Usage(invocation.usage.clone()))?;
                (None, 0, quantity)
            }
        }
        3 => {
            let quantity = args[2]
                .parse::<i32>()
                .map_err(|_| CommandError::Usage(invocation.usage.clone()))?;
            (Some(args[0].clone()), 1, quantity)
        }
        _ => return Err(CommandError::Usage(invocation.usage.clone())),
    };
    if let Some(target) = &target {
        invocation.authorize_target(target)?;
    }
    if quantity <= 0 {
        return Err(CommandError::Message(
            "quantity must be greater than zero".to_string(),
        ));
    }
    invocation.effect(CommandEffect::GiveItem {
        target,
        item_id: args[item_index].clone(),
        quantity,
    });
    Ok(())
}

fn normalize_game_mode(mode: &str) -> Result<&'static str, CommandError> {
    match mode.to_ascii_lowercase().as_str() {
        "a" | "adventure" => Ok("adventure"),
        "c" | "creative" => Ok("creative"),
        _ => Err(CommandError::Message(
            "game mode must be `adventure` or `creative`".to_string(),
        )),
    }
}

fn gamemode(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    let args = invocation.raw_args().to_vec();
    let (target, mode) = match args.len() {
        1 => (None, args[0].as_str()),
        2 => {
            invocation.authorize_target(&args[0])?;
            (Some(args[0].clone()), args[1].as_str())
        }
        _ => return Err(CommandError::Usage(invocation.usage.clone())),
    };
    let mode = normalize_game_mode(mode)?;
    invocation.effect(CommandEffect::SetGameMode {
        target,
        mode: mode.to_string(),
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
    invocation.no_extra_args(1)?;
    let target = invocation.target(0)?;
    invocation.effect(CommandEffect::Kill { target });
    Ok(())
}

fn respawn(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    invocation.no_extra_args(1)?;
    let target = invocation.target(0)?;
    invocation.effect(CommandEffect::Respawn { target });
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

fn say(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    let message = invocation.rest_raw().to_string();
    if message.trim().is_empty() {
        return Err(CommandError::Usage(invocation.usage.clone()));
    }
    let formatted = if message.trim_start().starts_with('{') {
        with_color(
            parse_message(message.trim()).map_err(CommandError::Message)?,
            "#00ffff",
        )
    } else {
        let fallback = format!("{}: {message}", invocation.sender.name);
        with_color(
            translated_message(
                "server.chat.broadcastMessage",
                &[
                    ("username", invocation.sender.name.as_str()),
                    ("message", message.as_str()),
                ],
                fallback,
            ),
            "#00ffff",
        )
    };
    invocation.effect(CommandEffect::BroadcastFormatted { message: formatted });
    Ok(())
}

fn kick(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    let target = invocation.required(0, "player")?.to_string();
    if !invocation.context.has_player(&target) {
        return Err(CommandError::UnknownPlayer(target));
    }
    let reason = invocation.rest(1);
    invocation.effect(CommandEffect::Kick {
        target,
        reason: (!reason.is_empty()).then_some(reason),
    });
    Ok(())
}

fn world(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    invocation.no_extra_args(1)?;
    match invocation.arg(0) {
        Some(name) => invocation.effect(CommandEffect::SwitchWorld {
            target: None,
            world: name.to_string(),
        }),
        None => {
            let name = invocation
                .context
                .world_name
                .as_deref()
                .unwrap_or("unknown");
            invocation.reply(format!("Current world: {name}"));
        }
    }
    Ok(())
}

fn maxplayers(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    invocation.no_extra_args(1)?;
    if let Some(value) = invocation.arg(0) {
        let max_players: i32 = value
            .parse()
            .map_err(|_| CommandError::Usage(format!("{} <amount>", invocation.usage)))?;
        invocation.effect(CommandEffect::SetMaxPlayers { max_players });
        if max_players > 0 {
            invocation.reply(format!("Max players set to {max_players}"));
        } else {
            invocation.reply("Max players set to unlimited".to_string());
        }
        return Ok(());
    }
    invocation.reply(format!(
        "Max players: {}",
        invocation.context.max_players.unwrap_or(0)
    ));
    Ok(())
}

fn maxviewradius(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    invocation.no_extra_args(1)?;
    if let Some(value) = invocation.arg(0) {
        let radius: u32 = value
            .parse()
            .map_err(|_| CommandError::Usage(format!("{} <radius>", invocation.usage)))?;
        if radius == 0 {
            return Err(CommandError::Message(
                "view radius must be at least 1 chunk".to_string(),
            ));
        }
        if radius > 32 {
            return Err(CommandError::Message(
                "view radius cannot be higher than 32 chunks".to_string(),
            ));
        }
        invocation.effect(CommandEffect::SetMaxViewRadius {
            max_view_radius: radius,
        });
        invocation.reply(format!("Max view radius set to {radius}"));
        return Ok(());
    }
    invocation.reply(format!(
        "Max view radius: {}",
        invocation.context.max_view_radius.unwrap_or(0)
    ));
    Ok(())
}

fn setblock(invocation: &mut CommandInvocation<'_>) -> Result<(), CommandError> {
    invocation.no_extra_args(5)?;
    let x = invocation.required_i32(0, "x")?;
    let y = invocation.required_i32(1, "y")?;
    let z = invocation.required_i32(2, "z")?;
    let block = invocation.required(3, "block")?.to_string();
    let rotation = invocation
        .arg(4)
        .map(|value| {
            let value: i32 = value
                .parse()
                .map_err(|_| CommandError::Usage(invocation.usage.clone()))?;
            match value {
                0 | 90 | 180 | 270 => Ok(value / 90),
                0..=63 => Ok(value),
                _ => Err(CommandError::Message(
                    "rotation must be 0, 90, 180, 270 or a 0..63 tuple index".to_string(),
                )),
            }
        })
        .transpose()?;
    invocation.effect(CommandEffect::SetBlock {
        x,
        y,
        z,
        block,
        rotation,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::registry;
    use crate::command::{CommandContext, CommandEffect, CommandError, CommandSender};

    fn context() -> CommandContext {
        CommandContext {
            players: vec!["Alice".to_string(), "Bob".to_string()],
            player_positions: vec![
                ("Alice".to_string(), [1.0, 2.0, 3.0]),
                ("Bob".to_string(), [10.0, 20.0, 30.0]),
            ],
            ..CommandContext::default()
        }
    }

    #[test]
    fn built_in_names_and_aliases_resolve() {
        let registry = registry();
        for name in [
            "help",
            "tp",
            "give",
            "gamemode",
            "save",
            "stop",
            "who",
            "say",
            "kick",
            "world",
            "maxplayers",
            "maxviewradius",
            "setblock",
        ] {
            assert!(registry.resolve(name).is_some(), "missing command {name}");
        }
        assert_eq!(registry.resolve("gm").expect("alias").name, "gamemode");
        assert_eq!(registry.resolve("?").expect("alias").name, "help");
        assert_eq!(registry.resolve("broadcast").expect("alias").name, "say");
        assert_eq!(registry.resolve("sb").expect("alias").name, "setblock");
    }

    #[test]
    fn tp_produces_a_teleport_effect() {
        let registry = registry();
        let output = registry
            .execute(
                "/tp 12.5 64 -3.25",
                &CommandSender::player("Bob"),
                &context(),
            )
            .expect("tp");
        assert_eq!(
            output.effects,
            [CommandEffect::Teleport {
                target: None,
                x: 12.5,
                y: 64.0,
                z: -3.25
            }]
        );
    }

    #[test]
    fn tp_accepts_player_targets_and_player_destinations() {
        let registry = registry();
        let sender = CommandSender::player("Alice").operator();
        let output = registry
            .execute("/tp Bob 4 200 4", &sender, &context())
            .expect("tp player coords");
        assert_eq!(
            output.effects,
            [CommandEffect::Teleport {
                target: Some("Bob".to_string()),
                x: 4.0,
                y: 200.0,
                z: 4.0
            }]
        );

        let output = registry
            .execute("/tp Bob Alice", &sender, &context())
            .expect("tp player to player");
        assert_eq!(
            output.effects,
            [CommandEffect::Teleport {
                target: Some("Bob".to_string()),
                x: 1.0,
                y: 2.0,
                z: 3.0
            }]
        );

        let output = registry
            .execute("/tp Bob", &sender, &context())
            .expect("tp self to player");
        assert_eq!(
            output.effects,
            [CommandEffect::Teleport {
                target: None,
                x: 10.0,
                y: 20.0,
                z: 30.0
            }]
        );
    }

    #[test]
    fn targeting_another_player_requires_the_target_permission() {
        let registry = registry();
        let error = registry
            .execute(
                "/tp Bob 4 200 4",
                &CommandSender::player("Alice"),
                &context(),
            )
            .expect_err("no target permission");
        assert_eq!(error, CommandError::PermissionDenied);
        assert_eq!(
            registry
                .execute("/tp Nobody", &CommandSender::player("Alice"), &context())
                .expect_err("unknown player"),
            CommandError::UnknownPlayer("Nobody".to_string())
        );
    }

    #[test]
    fn give_validates_the_quantity_and_targets_players() {
        let registry = registry();
        let sender = CommandSender::player("Alice").operator();
        let context = context();
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
                target: None,
                item_id: "Rock".to_string(),
                quantity: 3
            }]
        );
        assert_eq!(
            registry
                .execute("give Bob Rock", &sender, &context)
                .expect("give player")
                .effects,
            [CommandEffect::GiveItem {
                target: Some("Bob".to_string()),
                item_id: "Rock".to_string(),
                quantity: 1
            }]
        );
        assert_eq!(
            registry
                .execute("give Bob Rock 2", &sender, &context)
                .expect("give player quantity")
                .effects,
            [CommandEffect::GiveItem {
                target: Some("Bob".to_string()),
                item_id: "Rock".to_string(),
                quantity: 2
            }]
        );
    }

    #[test]
    fn gamemode_accepts_self_and_targets() {
        let registry = registry();
        let sender = CommandSender::player("Alice").operator();
        let context = context();
        assert_eq!(
            registry
                .execute("gamemode c", &sender, &context)
                .expect("gamemode")
                .effects,
            [CommandEffect::SetGameMode {
                target: None,
                mode: "creative".to_string()
            }]
        );
        assert_eq!(
            registry
                .execute("gamemode Bob adventure", &sender, &context)
                .expect("gamemode target")
                .effects,
            [CommandEffect::SetGameMode {
                target: Some("Bob".to_string()),
                mode: "adventure".to_string()
            }]
        );
    }

    #[test]
    fn kick_and_say_produce_broadcasts() {
        let registry = registry();
        let sender = CommandSender::player("Alice").operator();
        let context = context();
        assert_eq!(
            registry
                .execute("kick Bob moving away", &sender, &context)
                .expect("kick")
                .effects,
            [CommandEffect::Kick {
                target: "Bob".to_string(),
                reason: Some("moving away".to_string())
            }]
        );
        let say = registry
            .execute("say hello everyone", &sender, &context)
            .expect("say");
        let [CommandEffect::BroadcastFormatted { message }] = say.effects.as_slice() else {
            panic!("expected a formatted broadcast: {:?}", say.effects);
        };
        assert_eq!(
            message.message_id.as_deref(),
            Some("server.chat.broadcastMessage")
        );
        assert_eq!(message.raw_text.as_deref(), Some("Alice: hello everyone"));
        assert_eq!(message.color.as_deref(), Some("#00ffff"));

        let say = registry
            .execute(r#"say {"RawText":"styled","Bold":true}"#, &sender, &context)
            .expect("say json");
        let [CommandEffect::BroadcastFormatted { message }] = say.effects.as_slice() else {
            panic!("expected a formatted broadcast: {:?}", say.effects);
        };
        assert_eq!(message.raw_text.as_deref(), Some("styled"));
        assert_eq!(message.bold, Some(true));
        assert_eq!(message.color.as_deref(), Some("#00ffff"));
        assert!(registry
            .execute("say", &sender, &context)
            .expect_err("empty say")
            .to_string()
            .starts_with("usage:"));
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

    #[test]
    fn setblock_accepts_ids_keys_and_rotations() {
        let registry = registry();
        let sender = CommandSender::player("Alice").operator();
        let context = context();
        assert_eq!(
            registry
                .execute("setblock 1 64 -2 Rock_Stone 90", &sender, &context)
                .expect("setblock")
                .effects,
            [CommandEffect::SetBlock {
                x: 1,
                y: 64,
                z: -2,
                block: "Rock_Stone".to_string(),
                rotation: Some(1),
            }]
        );
        assert_eq!(
            registry
                .execute("setblock 1 64 -2 3014 5", &sender, &context)
                .expect("setblock raw rotation")
                .effects,
            [CommandEffect::SetBlock {
                x: 1,
                y: 64,
                z: -2,
                block: "3014".to_string(),
                rotation: Some(5),
            }]
        );
        assert!(registry
            .execute("setblock 1 64 -2 Rock_Stone 64", &sender, &context)
            .expect_err("invalid rotation")
            .to_string()
            .contains("rotation"));
        assert!(registry
            .execute("setblock 1 64 Rock_Stone", &sender, &context)
            .expect_err("missing z")
            .to_string()
            .starts_with("usage:"));
    }

    #[test]
    fn builtin_tree_is_stable_and_permission_filtered() {
        let registry = registry();
        let operator = registry.tree(&CommandSender::player("Alice").operator());
        let commands = operator.commands.expect("commands");
        let names: Vec<&str> = commands
            .iter()
            .filter_map(|command| command.name.as_deref())
            .collect();
        assert_eq!(
            names,
            [
                "help",
                "tp",
                "give",
                "gamemode",
                "save",
                "stop",
                "whereami",
                "whoami",
                "who",
                "kill",
                "respawn",
                "time",
                "version",
                "say",
                "kick",
                "world",
                "maxplayers",
                "maxviewradius",
                "setblock",
            ]
        );
        let tp = commands
            .iter()
            .find(|command| command.name.as_deref() == Some("tp"))
            .expect("tp");
        assert_eq!(
            tp.usage_text.as_deref(),
            Some("/tp [player] <x> <y> <z> | tp <player> [destination]")
        );
        assert_eq!(tp.required_args.as_ref().expect("args").len(), 4);

        let player = registry.tree(&CommandSender::player("Bob"));
        let commands = player.commands.expect("commands");
        assert!(commands
            .iter()
            .any(|command| command.name.as_deref() == Some("tp")));
        assert!(!commands
            .iter()
            .any(|command| command.name.as_deref() == Some("stop")));
        assert!(!commands
            .iter()
            .any(|command| command.name.as_deref() == Some("kick")));
    }

    #[test]
    fn world_and_limits_report_context_values() {
        let registry = registry();
        let sender = CommandSender::player("Alice").operator();
        let context = CommandContext {
            world_name: Some("Default".to_string()),
            max_players: Some(20),
            max_view_radius: Some(16),
            ..context()
        };
        let output = registry.execute("world", &sender, &context).expect("world");
        assert_eq!(output.messages, ["Current world: Default"]);
        let output = registry
            .execute("maxplayers", &sender, &context)
            .expect("maxplayers");
        assert_eq!(output.messages, ["Max players: 20"]);
        let output = registry
            .execute("maxviewradius", &sender, &context)
            .expect("maxviewradius");
        assert_eq!(output.messages, ["Max view radius: 16"]);
        assert!(registry
            .execute("world Nether", &sender, &context)
            .expect("world switch")
            .effects
            .iter()
            .any(|effect| matches!(effect, CommandEffect::SwitchWorld { world, .. } if world == "Nether")));

        let output = registry
            .execute("maxplayers 25", &sender, &context)
            .expect("set maxplayers");
        assert_eq!(output.messages, ["Max players set to 25"]);
        assert_eq!(
            output.effects,
            [CommandEffect::SetMaxPlayers { max_players: 25 }]
        );
        let output = registry
            .execute("maxplayers 0", &sender, &context)
            .expect("unlimited maxplayers");
        assert_eq!(output.messages, ["Max players set to unlimited"]);

        let output = registry
            .execute("maxviewradius 2", &sender, &context)
            .expect("set maxviewradius");
        assert_eq!(output.messages, ["Max view radius set to 2"]);
        assert_eq!(
            output.effects,
            [CommandEffect::SetMaxViewRadius { max_view_radius: 2 }]
        );
        assert_eq!(
            registry
                .execute("maxviewradius 0", &sender, &context)
                .expect_err("zero radius")
                .to_string(),
            "view radius must be at least 1 chunk"
        );
        assert_eq!(
            registry
                .execute("maxviewradius 33", &sender, &context)
                .expect_err("too large")
                .to_string(),
            "view radius cannot be higher than 32 chunks"
        );
    }
}
