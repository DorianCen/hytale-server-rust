//! Command sender, context and result types.

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
}

impl CommandSender {
    /// A player sender.
    pub fn player(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: SenderKind::Player,
            operator: false,
        }
    }

    /// A console sender (always operator-level).
    pub fn console() -> Self {
        Self {
            name: "console".to_string(),
            kind: SenderKind::Console,
            operator: true,
        }
    }

    /// Mark the sender as an operator.
    pub fn operator(mut self) -> Self {
        self.operator = true;
        self
    }

    /// Whether the sender may run a command protected by `permission`.
    pub fn has_permission(&self, _permission: &str) -> bool {
        self.operator || self.kind == SenderKind::Console
    }
}

/// A side effect requested by a command.
///
/// The command crate stays independent of the server runtime; the session
/// applies these effects and reports success or failure back to the client.
#[derive(Debug, Clone, PartialEq)]
pub enum CommandEffect {
    /// Move the sender to an absolute position.
    Teleport {
        /// X coordinate.
        x: f64,
        /// Y coordinate.
        y: f64,
        /// Z coordinate.
        z: f64,
    },
    /// Add an item stack to the sender's inventory.
    GiveItem {
        /// Item asset id.
        item_id: String,
        /// Positive quantity.
        quantity: i32,
    },
    /// Change the sender's game mode (`adventure` or `creative`).
    SetGameMode {
        /// Normalized game-mode name.
        mode: String,
    },
    /// Kill the sender.
    Kill,
    /// Respawn the sender.
    Respawn,
    /// Persist the world and the sender.
    SaveWorld,
    /// Request a graceful listener shutdown.
    StopServer,
}

/// The runtime state a command may inspect.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommandContext {
    /// Sender position.
    pub position: [f64; 3],
    /// Connected player names.
    pub players: Vec<String>,
    /// Current game time in seconds, if known.
    pub game_time_seconds: Option<i64>,
    /// Whether the world clock is paused, if known.
    pub time_paused: Option<bool>,
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
