//! Permission holders and the command permission registry.
//!
//! Java resolves command permissions through a holder/group hierarchy. The
//! server only needs the query seam: a set of granted permission strings per
//! player, plus named groups and a default set. `*` matches every permission
//! and a trailing `.*` matches a prefix (`server.commands.*`).

use std::collections::{BTreeMap, BTreeSet};

/// A set of granted permission strings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PermissionSet {
    granted: BTreeSet<String>,
}

impl PermissionSet {
    /// An empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// A set that grants every permission (`*`).
    pub fn all() -> Self {
        let mut set = Self::new();
        set.grant("*");
        set
    }

    /// Grant one permission string.
    pub fn grant(&mut self, permission: impl Into<String>) {
        let permission = permission.into();
        if !permission.is_empty() {
            self.granted.insert(permission);
        }
    }

    /// Grant every permission in `permissions`.
    pub fn extend<I, S>(&mut self, permissions: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for permission in permissions {
            self.grant(permission);
        }
    }

    /// Whether the set grants `permission`.
    pub fn contains(&self, permission: &str) -> bool {
        if self.granted.contains("*") || self.granted.contains(permission) {
            return true;
        }
        self.granted.iter().any(|granted| {
            granted
                .strip_suffix('*')
                .is_some_and(|prefix| permission.starts_with(prefix))
        })
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.granted.is_empty()
    }

    /// The granted entries, for diagnostics and tests.
    pub fn granted(&self) -> impl Iterator<Item = &str> {
        self.granted.iter().map(String::as_str)
    }
}

/// Named groups, per-player grants and the default player set.
#[derive(Debug, Clone, Default)]
pub struct PermissionRegistry {
    default: PermissionSet,
    players: BTreeMap<String, PermissionSet>,
    groups: BTreeMap<String, PermissionSet>,
    player_groups: BTreeMap<String, Vec<String>>,
}

impl PermissionRegistry {
    /// An empty registry: players start with no permissions.
    pub fn new() -> Self {
        Self::default()
    }

    /// A development/test registry where every player has `*`.
    ///
    /// This preserves the pre-permissions behavior for the loopback scenarios
    /// and CLI defaults that do not configure operators.
    pub fn permissive() -> Self {
        Self {
            default: PermissionSet::all(),
            ..Self::default()
        }
    }

    /// Replace the default set every player inherits.
    pub fn set_default(&mut self, permissions: PermissionSet) {
        self.default = permissions;
    }

    /// The default set every player inherits.
    pub fn default_permissions(&self) -> &PermissionSet {
        &self.default
    }

    /// Add or replace a named group.
    pub fn set_group(&mut self, group: impl Into<String>, permissions: PermissionSet) {
        self.groups.insert(normalize(&group.into()), permissions);
    }

    /// Add `player` to a named group.
    pub fn assign_group(&mut self, player: &str, group: &str) {
        self.player_groups
            .entry(normalize(player))
            .or_default()
            .push(normalize(group));
    }

    /// Grant one permission directly to a player.
    pub fn grant(&mut self, player: &str, permission: impl Into<String>) {
        self.players
            .entry(normalize(player))
            .or_default()
            .grant(permission);
    }

    /// Grant every permission to a player (an operator).
    pub fn grant_operator(&mut self, player: &str) {
        self.grant(player, "*");
    }

    /// Resolve the complete permission set for a player name.
    pub fn permissions_for(&self, player: &str) -> PermissionSet {
        let player = normalize(player);
        let mut resolved = self.default.clone();
        if let Some(groups) = self.player_groups.get(&player) {
            for group in groups {
                if let Some(permissions) = self.groups.get(group) {
                    resolved.extend(permissions.granted().map(str::to_string));
                }
            }
        }
        if let Some(permissions) = self.players.get(&player) {
            resolved.extend(permissions.granted().map(str::to_string));
        }
        resolved
    }

    /// Whether a player is an operator (`*`).
    pub fn is_operator(&self, player: &str) -> bool {
        self.permissions_for(player).contains("*")
    }
}

fn normalize(name: &str) -> String {
    name.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{PermissionRegistry, PermissionSet};

    #[test]
    fn exact_and_wildcard_permissions_match() {
        let mut set = PermissionSet::new();
        set.grant("server.commands.tp");
        set.grant("server.commands.give.*");
        assert!(set.contains("server.commands.tp"));
        assert!(set.contains("server.commands.give.others"));
        assert!(!set.contains("server.commands.stop"));
        assert!(PermissionSet::all().contains("anything"));
    }

    #[test]
    fn registry_unions_default_groups_and_player_grants() {
        let mut registry = PermissionRegistry::new();
        let mut default = PermissionSet::new();
        default.grant("server.commands.help");
        registry.set_default(default);
        let mut moderators = PermissionSet::new();
        moderators.grant("server.commands.kick");
        registry.set_group("moderator", moderators);
        registry.assign_group("Bob", "moderator");
        registry.grant("Bob", "server.commands.tp");
        registry.grant_operator("Alice");

        let bob = registry.permissions_for("bob");
        assert!(bob.contains("server.commands.help"));
        assert!(bob.contains("server.commands.kick"));
        assert!(bob.contains("server.commands.tp"));
        assert!(!bob.contains("server.commands.stop"));
        assert!(registry.is_operator("alice"));
        assert!(!registry.is_operator("bob"));
    }

    #[test]
    fn permissive_grants_operators() {
        let registry = PermissionRegistry::permissive();
        assert!(registry.is_operator("anyone"));
        assert!(registry
            .permissions_for("anyone")
            .contains("server.commands.stop"));
    }
}
