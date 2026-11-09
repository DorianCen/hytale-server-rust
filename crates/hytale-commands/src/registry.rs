//! Command registration, dispatch and client-tree generation.

use std::collections::HashMap;

use hytale_protocol::generated::common::{CommandArgInfo, CommandTreeEntry};
use hytale_protocol::generated::interface_::CommandTreeSync;

use crate::command::{
    CommandContext, CommandError, CommandInvocation, CommandOutput, CommandSender,
};
use crate::tokenizer::tokenize;

/// A command handler.
pub type CommandHandler = fn(&mut CommandInvocation<'_>) -> Result<(), CommandError>;

/// The kind of a command argument (used by the client tree and usage text).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgKind {
    /// A free-form string.
    String,
    /// An integer.
    Integer,
    /// A floating-point number.
    Float,
    /// A player name.
    Player,
    /// An item id.
    Item,
    /// A game mode.
    GameMode,
    /// Three coordinates.
    Coordinates,
}

impl ArgKind {
    /// Display name for `CommandArgInfo.argTypeName`.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::String => "String",
            Self::Integer => "Integer",
            Self::Float => "Float",
            Self::Player => "Player",
            Self::Item => "Item",
            Self::GameMode => "GameMode",
            Self::Coordinates => "Coordinates",
        }
    }
}

/// One required command argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArgSpec {
    /// Argument name shown in usage and the client tree.
    pub name: &'static str,
    /// Argument kind.
    pub kind: ArgKind,
    /// Argument description.
    pub description: &'static str,
}

/// A registered command.
#[derive(Debug, Clone)]
pub struct Command {
    /// Primary name (without `/`).
    pub name: &'static str,
    /// Aliases.
    pub aliases: &'static [&'static str],
    /// Description shown in `/help` and the client tree.
    pub description: &'static str,
    /// Usage text after the command name, for example `tp <x> <y> <z>`.
    pub usage: &'static str,
    /// Optional permission required to run or see the command.
    pub permission: Option<&'static str>,
    /// Required arguments.
    pub args: &'static [ArgSpec],
    /// Handler.
    pub handler: CommandHandler,
}

impl Command {
    fn usage_line(&self) -> String {
        format!("Usage: /{}", self.usage)
    }
}

/// A command table with aliases.
#[derive(Default)]
pub struct CommandRegistry {
    commands: Vec<Command>,
    aliases: HashMap<String, usize>,
}

impl std::fmt::Debug for CommandRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CommandRegistry")
            .field("commands", &self.commands.len())
            .field("aliases", &self.aliases.len())
            .finish()
    }
}

impl CommandRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// The built-in console/in-game command set.
    pub fn builtin() -> Self {
        crate::builtins::registry()
    }

    /// Register a command and its aliases.
    pub fn register(&mut self, command: Command) -> Result<(), CommandError> {
        let canonical = command.name.to_ascii_lowercase();
        if self.aliases.contains_key(&canonical) {
            return Err(CommandError::Message(format!(
                "command `{}` is already registered",
                command.name
            )));
        }
        for alias in command.aliases {
            let alias = alias.to_ascii_lowercase();
            if self.aliases.contains_key(&alias) {
                return Err(CommandError::Message(format!(
                    "command alias `{alias}` is already registered"
                )));
            }
        }
        let index = self.commands.len();
        self.aliases.insert(canonical, index);
        for alias in command.aliases {
            self.aliases.insert(alias.to_ascii_lowercase(), index);
        }
        self.commands.push(command);
        Ok(())
    }

    /// Resolve a command name or alias.
    pub fn resolve(&self, name: &str) -> Option<&Command> {
        self.aliases
            .get(&name.to_ascii_lowercase())
            .and_then(|index| self.commands.get(*index))
    }

    /// Every registered command.
    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    /// Parse and execute one command line.
    pub fn execute(
        &self,
        line: &str,
        sender: &CommandSender,
        context: &CommandContext,
    ) -> Result<CommandOutput, CommandError> {
        let tokens = tokenize(line).map_err(|error| CommandError::Message(error.to_string()))?;
        let Some(name) = tokens.first() else {
            return Err(CommandError::Usage("Usage: /<command>".to_string()));
        };
        let command = self
            .resolve(name)
            .ok_or_else(|| CommandError::UnknownCommand(name.clone()))?;
        if let Some(permission) = command.permission {
            if !sender.has_permission(permission) {
                return Err(CommandError::PermissionDenied);
            }
        }

        let help = self.help_lines(sender);
        let mut invocation = CommandInvocation {
            sender,
            args: &tokens[1..],
            context,
            usage: command.usage_line(),
            help: &help,
            messages: Vec::new(),
            effects: Vec::new(),
        };
        (command.handler)(&mut invocation)?;
        Ok(invocation.finish())
    }

    /// Build the `CommandTreeSync` packet for one sender.
    pub fn tree(&self, sender: &CommandSender) -> CommandTreeSync {
        let commands = self
            .commands
            .iter()
            .filter(|command| command_allowed(command, sender))
            .map(tree_entry)
            .collect();
        CommandTreeSync {
            commands: Some(commands),
        }
    }

    fn help_lines(&self, sender: &CommandSender) -> Vec<String> {
        self.commands
            .iter()
            .filter(|command| command_allowed(command, sender))
            .map(|command| format!("/{} - {}", command.name, command.description))
            .collect()
    }
}

fn command_allowed(command: &Command, sender: &CommandSender) -> bool {
    command
        .permission
        .is_none_or(|permission| sender.has_permission(permission))
}

fn tree_entry(command: &Command) -> CommandTreeEntry {
    let required_args = command
        .args
        .iter()
        .map(|argument| CommandArgInfo {
            name: Some(argument.name.to_string()),
            arg_type_id: None,
            arg_type_name: Some(argument.kind.display_name().to_string()),
            value_count: 1,
            description: Some(argument.description.to_string()),
        })
        .collect();
    CommandTreeEntry {
        name: Some(command.name.to_string()),
        aliases: Some(
            command
                .aliases
                .iter()
                .map(|alias| (*alias).to_string())
                .collect(),
        ),
        usage_text: Some(format!("/{}", command.usage)),
        description: Some(command.description.to_string()),
        variants: Some(Vec::new()),
        subcommands: Some(Vec::new()),
        subcommand_hints: Some(Vec::new()),
        required_args: Some(required_args),
        optional_args: Some(Vec::new()),
        suggestion_overrides: Some(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::{ArgKind, ArgSpec, Command, CommandRegistry};
    use crate::command::{CommandContext, CommandError, CommandSender};

    fn noop(_: &mut crate::command::CommandInvocation<'_>) -> Result<(), CommandError> {
        Ok(())
    }

    fn registry() -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        registry
            .register(Command {
                name: "test",
                aliases: &["t"],
                description: "test command",
                usage: "test",
                permission: Some("test.admin"),
                args: &[ArgSpec {
                    name: "value",
                    kind: ArgKind::String,
                    description: "a value",
                }],
                handler: noop,
            })
            .expect("register");
        registry
    }

    #[test]
    fn resolves_names_and_aliases_case_insensitively() {
        let registry = registry();
        assert_eq!(registry.resolve("TEST").expect("name").name, "test");
        assert_eq!(registry.resolve("t").expect("alias").name, "test");
    }

    #[test]
    fn rejects_duplicate_names_and_aliases() {
        let mut registry = registry();
        assert!(registry
            .register(Command {
                name: "other",
                aliases: &["t"],
                description: "",
                usage: "other",
                permission: None,
                args: &[],
                handler: noop,
            })
            .is_err());
    }

    #[test]
    fn permission_denial_uses_the_sender_operator_flag() {
        let registry = registry();
        let context = CommandContext::default();
        let player = CommandSender::player("Bob");
        assert_eq!(
            registry.execute("test value", &player, &context),
            Err(CommandError::PermissionDenied)
        );
        assert!(registry
            .execute("test value", &player.operator(), &context)
            .is_ok());
    }

    #[test]
    fn tree_filters_permission_commands_and_carries_args() {
        let registry = registry();
        let player = CommandSender::player("Bob");
        let tree = registry.tree(&player);
        assert_eq!(tree.commands.expect("commands").len(), 0);
        let tree = registry.tree(&player.operator());
        let entry = tree.commands.expect("commands").remove(0);
        assert_eq!(entry.name.as_deref(), Some("test"));
        assert_eq!(entry.aliases, Some(vec!["t".to_string()]));
        assert_eq!(entry.required_args.expect("required args").len(), 1);
    }

    #[test]
    fn command_tree_round_trips_through_the_protocol_codec() {
        use hytale_protocol::generated::wire::Wire;
        use hytale_protocol::io::{Reader, Writer};

        let registry = registry();
        let tree = registry.tree(&CommandSender::player("Bob").operator());
        let mut writer = Writer::new();
        tree.encode(&mut writer).expect("encode tree");
        let decoded = hytale_protocol::generated::interface_::CommandTreeSync::decode(
            &mut Reader::new(writer.as_slice()),
        )
        .expect("decode tree");
        let commands = decoded.commands.expect("commands");
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name.as_deref(), Some("test"));
        assert_eq!(commands[0].aliases, Some(vec!["t".to_string()]));
    }
}
