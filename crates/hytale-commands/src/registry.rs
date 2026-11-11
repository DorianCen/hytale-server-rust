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

/// Maximum number of suggestions written in one `ArgValuesResponse`.
pub const MAX_WIRE_SUGGESTIONS: usize = 150;

/// One resolved argument-value suggestion batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionResponse {
    /// Suggested values.
    pub values: Vec<String>,
    /// Per-value continuation flags (always `false` for the built-in kinds).
    pub continuations: Vec<bool>,
    /// Whether the result contains every match.
    pub is_complete: bool,
}

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

    /// Stable suggestion type id for `CommandArgInfo.argTypeId`.
    ///
    /// Java generates a per-process UUID for each `ArgumentType` and shares it
    /// between types with identical suggestions. The Rust registry is stable
    /// across restarts, so these ids are descriptive rather than random; the
    /// client only uses them as opaque cache keys.
    pub const fn type_id(self) -> &'static str {
        match self {
            Self::String => "hytale:string",
            Self::Integer => "hytale:integer",
            Self::Float => "hytale:float",
            Self::Player => "hytale:player",
            Self::Item => "hytale:item",
            Self::GameMode => "hytale:game_mode",
            Self::Coordinates => "hytale:coordinates",
        }
    }

    /// Java `ArgumentType.getSuggestionValueCount` for this kind.
    pub const fn suggestion_value_count(self) -> i32 {
        match self {
            Self::GameMode => 4,
            Self::String
            | Self::Integer
            | Self::Float
            | Self::Player
            | Self::Item
            | Self::Coordinates => -1,
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
        let raw = raw_command_remainder(line);
        let mut invocation = CommandInvocation {
            sender,
            args: &tokens[1..],
            raw: raw.to_string(),
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

    /// Resolve an `ArgValuesRequest` for one registered suggestion type.
    ///
    /// `item_ids` supplies the dynamic item-asset catalog; callers without an
    /// asset registry pass an empty slice. Unknown type ids return `None` so
    /// the network layer can ignore a malformed or stale request, matching
    /// Java `GamePacketHandler.handle(ArgValuesRequest)`.
    pub fn suggest(
        &self,
        arg_type_id: &str,
        partial: &str,
        context: &CommandContext,
        item_ids: &[String],
    ) -> Option<SuggestionResponse> {
        if arg_type_id.len() > 128 || partial.len() > 256 {
            return None;
        }
        let candidates = match arg_type_id {
            "hytale:player" => context.players.clone(),
            "hytale:item" => item_ids.to_vec(),
            "hytale:game_mode" => ["adventure", "creative", "a", "c"]
                .map(str::to_string)
                .to_vec(),
            "hytale:coordinates" => {
                let [x, y, z] = context.position;
                vec!["~ ~ ~".to_string(), format!("{x:.2} {y:.2} {z:.2}")]
            }
            "hytale:string" | "hytale:integer" | "hytale:float" => Vec::new(),
            _ => return None,
        };
        let mut values = filter_suggestions(candidates, partial);
        let is_complete = values.len() <= MAX_WIRE_SUGGESTIONS;
        values.truncate(MAX_WIRE_SUGGESTIONS);
        let continuations = vec![false; values.len()];
        Some(SuggestionResponse {
            values,
            continuations,
            is_complete,
        })
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

fn raw_command_remainder(line: &str) -> &str {
    let line = line.trim_start();
    let line = line.strip_prefix('/').unwrap_or(line);
    line.split_once(char::is_whitespace)
        .map_or("", |(_, rest)| rest.trim_start())
}

fn filter_suggestions(candidates: Vec<String>, partial: &str) -> Vec<String> {
    if partial.is_empty() {
        return candidates;
    }
    let lower = partial.to_ascii_lowercase();
    let mut prefix = Vec::new();
    let mut contains = Vec::new();
    for candidate in candidates {
        let lower_candidate = candidate.to_ascii_lowercase();
        if lower_candidate.starts_with(&lower) {
            prefix.push(candidate);
        } else if lower_candidate.contains(&lower) {
            contains.push(candidate);
        }
    }
    let order =
        |left: &String, right: &String| left.len().cmp(&right.len()).then_with(|| left.cmp(right));
    prefix.sort_by(order);
    contains.sort_by(order);
    prefix.extend(contains);
    prefix
}

fn tree_entry(command: &Command) -> CommandTreeEntry {
    let required_args = command
        .args
        .iter()
        .map(|argument| CommandArgInfo {
            name: Some(argument.name.to_string()),
            arg_type_id: Some(argument.kind.type_id().to_string()),
            arg_type_name: Some(argument.kind.display_name().to_string()),
            value_count: argument.kind.suggestion_value_count(),
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
        let args = entry.required_args.expect("required args");
        assert_eq!(args.len(), 1);
        assert_eq!(args[0].arg_type_id.as_deref(), Some("hytale:string"));
        assert_eq!(args[0].value_count, -1);
    }

    #[test]
    fn suggestions_filter_players_items_and_game_modes() {
        let registry = CommandRegistry::builtin();
        let context = crate::command::CommandContext {
            players: vec!["Alice".to_string(), "Bob".to_string()],
            position: [12.5, 64.0, -3.0],
            ..crate::command::CommandContext::default()
        };
        let items = vec![
            "Rock_Stone".to_string(),
            "Rock_Sandstone".to_string(),
            "Potion_Health".to_string(),
        ];

        let players = registry
            .suggest("hytale:player", "a", &context, &items)
            .expect("player suggestions");
        assert_eq!(players.values, ["Alice"]);
        assert!(players.is_complete);
        assert_eq!(players.continuations, [false]);

        let items = registry
            .suggest("hytale:item", "rock_", &context, &items)
            .expect("item suggestions");
        assert_eq!(items.values, ["Rock_Stone", "Rock_Sandstone"]);

        let modes = registry
            .suggest("hytale:game_mode", "c", &context, &[])
            .expect("game mode suggestions");
        assert_eq!(modes.values, ["c", "creative"]);

        let coordinates = registry
            .suggest("hytale:coordinates", "", &context, &[])
            .expect("coordinate suggestions");
        assert_eq!(coordinates.values, ["~ ~ ~", "12.50 64.00 -3.00"]);

        assert!(registry
            .suggest("hytale:unknown", "", &context, &[])
            .is_none());
        assert!(registry
            .suggest(&"x".repeat(129), "", &context, &[])
            .is_none());
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
