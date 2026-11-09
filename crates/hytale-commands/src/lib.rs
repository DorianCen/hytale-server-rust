//! `hytale-commands` — console and in-game command framework.
//!
//! Provides argument parsing, command registration and dispatch, permission
//! checks, the `CommandTreeSync` view sent to clients, and the core built-in
//! commands (`help`, `tp`, `give`, `save`, `stop`, ...).
//!
//! Commands are pure with respect to the server runtime: a handler validates
//! its arguments and returns [`CommandEffect`]s. The owning world/session
//! applies those effects (teleport, inventory, save, ...) and reports success.

#![deny(clippy::all)]

mod builtins;
mod command;
mod registry;
mod tokenizer;

pub use command::{
    CommandContext, CommandEffect, CommandError, CommandInvocation, CommandOutput, CommandSender,
    SenderKind,
};
pub use registry::{ArgKind, ArgSpec, Command, CommandHandler, CommandRegistry};
pub use tokenizer::{tokenize, TokenizeError};

/// Build the built-in command registry.
pub fn builtin_registry() -> CommandRegistry {
    builtins::registry()
}
