//! Command sender, context and result types.

use std::sync::Arc;

use hytale_core::PermissionSet;
use thiserror::Error;

/// Who is running a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SenderKind {
    /// The server console.
    Console,
    /// An in-game player.
    Player,
}

/// The identity and permissions of a command sender.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSender {
    /// Display name.
    pub name: String,
    /// Sender kind.
    pub kind: SenderKind,
    /// Whether the sender has operator-level permissions.
    pub operator: bool,
    /// Resolved permission set, when the server has a permission registry.
    pub permissions: Option<Arc<PermissionSet>>,
}

impl CommandSender {
    /// A player sender with no permission registry (operator flag only).
    pub fn player(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: SenderKind::Player,
            operator: false,
            permissions: None,
        }
    }

    /// A player sender with a resolved permission set.
    pub fn player_with(name: impl Into<String>, permissions: Arc<PermissionSet>) -> Self {
        let operator = permissions.contains("*");
        Self {
            name: name.into(),
            kind: SenderKind::Player,
            operator,
            permissions: Some(permissions),
        }
    }

    /// A console sender (always operator-level).
    pub fn console() -> Self {
        Self {
            name: "console".to_string(),
            kind: SenderKind::Console,
            operator: true,
            permissions: None,
        }
    }

    /// Mark the sender as an operator.
    pub fn operator(mut self) -> Self {
        self.operator = true;
        self
    }

    /// Whether the sender may run a command protected by `permission`.
    pub fn has_permission(&self, permission: &str) -> bool {
        if self.kind == SenderKind::Console {
            return true;
        }
        match &self.permissions {
            Some(permissions) => permissions.contains(permission),
            None => self.operator,
        }
    }
}

/// A side effect requested by a command.
///
/// The command crate stays independent of the server runtime; the session
/// applies these effects and reports success or failure back to the client.
/// `target` is `None` for the sender or `Some(name)` for another player.
#[derive(Debug, Clone, PartialEq)]
pub enum CommandEffect {
    /// Move a player to an absolute position.
    Teleport {
        /// Target player name; `None` is the sender.
        target: Option<String>,
        /// X coordinate.
        x: f64,
        /// Y coordinate.
        y: f64,
        /// Z coordinate.
        z: f64,
    },
    /// Add an item stack to a player's inventory.
    GiveItem {
        /// Target player name; `None` is the sender.
        target: Option<String>,
        /// Item asset id.
        item_id: String,
        /// Positive quantity.
        quantity: i32,
    },
    /// Change a player's game mode (`adventure` or `creative`).
    SetGameMode {
        /// Target player name; `None` is the sender.
        target: Option<String>,
        /// Normalized game-mode name.
        mode: String,
    },
    /// Kill a player.
    Kill {
        /// Target player name; `None` is the sender.
        target: Option<String>,
    },
    /// Respawn a player.
    Respawn {
        /// Target player name; `None` is the sender.
        target: Option<String>,
    },
    /// Persist the world and the sender.
    SaveWorld,
    /// Request a graceful listener shutdown.
    StopServer,
    /// Send a chat line to every connected player.
    Broadcast {
        /// Chat text.
        message: String,
    },
    /// Disconnect a player.
    Kick {
        /// Target player name.
        target: String,
        /// Optional disconnect reason.
        reason: Option<String>,
    },
    /// Move a player to another world in the universe.
    SwitchWorld {
        /// Target player name; `None` is the sender.
        target: Option<String>,
        /// World name or UUID.
        world: String,
    },
    /// Authoritatively replace one block (operator `/setblock`).
    SetBlock {
        /// Block X coordinate.
        x: i32,
        /// Block Y coordinate.
        y: i32,
        /// Block Z coordinate.
        z: i32,
        /// Block id or block type key.
        block: String,
        /// Java rotation tuple index, when explicitly requested.
        rotation: Option<i32>,
    },
}

/// The runtime state a command may inspect.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommandContext {
    /// Sender position.
    pub position: [f64; 3],
    /// Connected player names.
    pub players: Vec<String>,
    /// Connected player positions, keyed by the same display names.
    pub player_positions: Vec<(String, [f64; 3])>,
    /// Current game time in seconds, if known.
    pub game_time_seconds: Option<i64>,
    /// Whether the world clock is paused, if known.
    pub time_paused: Option<bool>,
    /// Current world display name, if known.
    pub world_name: Option<String>,
    /// Current world UUID, if known.
    pub world_uuid: Option<String>,
    /// Configured maximum player count, if known.
    pub max_players: Option<i32>,
    /// Configured maximum view radius, if known.
    pub max_view_radius: Option<u32>,
}

impl CommandContext {
    /// Whether `name` is a connected player (case-insensitive).
    pub fn has_player(&self, name: &str) -> bool {
        self.players
            .iter()
            .any(|player| player.eq_ignore_ascii_case(name))
    }

    /// One connected player's position (case-insensitive).
    pub fn player_position(&self, name: &str) -> Option<[f64; 3]> {
        self.player_positions
            .iter()
            .find(|(player, _)| player.eq_ignore_ascii_case(name))
            .map(|(_, position)| *position)
    }
}

/// A failed command.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CommandError {
    /// The command was called with the wrong arguments.
    #[error("usage: {0}")]
    Usage(String),
    /// No command has this name or alias.
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    /// The sender lacks the required permission.
    #[error("you do not have permission to use this command")]
    PermissionDenied,
    /// The named player is not connected.
    #[error("player `{0}` is not connected")]
    UnknownPlayer(String),
    /// A command-specific error.
    #[error("{0}")]
    Message(String),
}

/// Messages and effects produced by a successful command.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommandOutput {
    /// Messages to send back to the sender.
    pub messages: Vec<String>,
    /// Effects to apply on the session.
    pub effects: Vec<CommandEffect>,
}

/// One command invocation.
pub struct CommandInvocation<'a> {
    /// Sender.
    pub sender: &'a CommandSender,
    /// Positional arguments (the command name is not included).
    pub args: &'a [String],
    /// Runtime state.
    pub context: &'a CommandContext,
    /// The full usage line for this command.
    pub usage: String,
    /// The command list for `/help`.
    pub help: &'a [String],
    /// Messages produced by the handler.
    pub messages: Vec<String>,
    /// Effects produced by the handler.
    pub effects: Vec<CommandEffect>,
}

impl<'a> CommandInvocation<'a> {
    /// One positional argument.
    pub fn arg(&self, index: usize) -> Option<&str> {
        self.args
            .get(index)
            .map(String::as_str)
            .filter(|value| !value.is_empty())
    }

    /// Every raw argument.
    pub fn raw_args(&self) -> &[String] {
        self.args
    }

    /// The remaining arguments joined with spaces.
    pub fn rest(&self, start: usize) -> String {
        self.args
            .get(start..)
            .unwrap_or_default()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// A required positional argument.
    pub fn required(&self, index: usize, name: &str) -> Result<&str, CommandError> {
        self.arg(index)
            .ok_or_else(|| CommandError::Usage(format!("{} <{}>", self.usage, name)))
    }

    /// Parse a required `i32` argument.
    pub fn required_i32(&self, index: usize, name: &str) -> Result<i32, CommandError> {
        self.required(index, name)?
            .parse()
            .map_err(|_| CommandError::Usage(format!("{} <{}>", self.usage, name)))
    }

    /// Parse a required `f64` argument.
    pub fn required_f64(&self, index: usize, name: &str) -> Result<f64, CommandError> {
        let value: f64 = self
            .required(index, name)?
            .parse()
            .map_err(|_| CommandError::Usage(format!("{} <{}>", self.usage, name)))?;
        if value.is_finite() {
            Ok(value)
        } else {
            Err(CommandError::Usage(format!("{} <{}>", self.usage, name)))
        }
    }

    /// Resolve an optional target name, rejecting server-side commands that
    /// act on another player without the shared targeting permission.
    pub fn target(&self, index: usize) -> Result<Option<String>, CommandError> {
        let Some(name) = self.arg(index) else {
            return Ok(None);
        };
        self.authorize_target(name)?;
        Ok(Some(name.to_string()))
    }

    /// Check that the sender may act on `name`.
    pub fn authorize_target(&self, name: &str) -> Result<(), CommandError> {
        if name.eq_ignore_ascii_case(&self.sender.name)
            || self.sender.has_permission("server.commands.target")
        {
            Ok(())
        } else {
            Err(CommandError::PermissionDenied)
        }
    }

    /// Reject extra arguments.
    pub fn no_extra_args(&self, max: usize) -> Result<(), CommandError> {
        if self.args.len() > max {
            Err(CommandError::Usage(self.usage.clone()))
        } else {
            Ok(())
        }
    }

    /// Send a message back to the sender.
    pub fn reply(&mut self, message: impl Into<String>) {
        self.messages.push(message.into());
    }

    /// Request a runtime effect.
    pub fn effect(&mut self, effect: CommandEffect) {
        self.effects.push(effect);
    }

    /// Consume the invocation.
    pub fn finish(self) -> CommandOutput {
        CommandOutput {
            messages: self.messages,
            effects: self.effects,
        }
    }
}
