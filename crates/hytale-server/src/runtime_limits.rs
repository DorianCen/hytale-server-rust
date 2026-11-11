//! Listener limits that can change while the server is running.
//!
//! The CLI seeds these values, but `/maxplayers` and `/maxviewradius` update
//! them at runtime. All sessions share one [`RuntimeLimits`], so the handshake,
//! inbound view-radius clamps and the console observe the same values. Reducing
//! the view radius is also pushed to live sessions through a control action.

use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};

/// Java `HytaleServerConfig.MAX_VIEW_RADIUS_LIMIT`.
pub const MAX_VIEW_RADIUS_CHUNKS: u32 = 32;

/// Shared mutable listener limits.
#[derive(Debug)]
pub struct RuntimeLimits {
    max_players: AtomicI32,
    max_view_radius: AtomicU32,
}

impl RuntimeLimits {
    /// Create limits from the CLI values.
    pub fn new(max_players: i32, max_view_radius: u32) -> Self {
        Self {
            max_players: AtomicI32::new(max_players),
            max_view_radius: AtomicU32::new(max_view_radius.clamp(1, MAX_VIEW_RADIUS_CHUNKS)),
        }
    }

    /// Current `ServerInfo.maxPlayers` value (`<= 0` means unlimited).
    pub fn max_players(&self) -> i32 {
        self.max_players.load(Ordering::Relaxed)
    }

    /// Current maximum view radius in chunks.
    pub fn max_view_radius(&self) -> u32 {
        self.max_view_radius.load(Ordering::Relaxed)
    }

    /// Set the maximum player count; returns the value that was stored.
    pub fn set_max_players(&self, max_players: i32) -> i32 {
        self.max_players.store(max_players, Ordering::Relaxed);
        max_players
    }

    /// Set the view-radius ceiling, rejecting values outside Java's `1..=32`.
    pub fn set_max_view_radius(&self, max_view_radius: u32) -> Result<u32, &'static str> {
        if max_view_radius == 0 {
            return Err("view radius must be at least 1 chunk");
        }
        if max_view_radius > MAX_VIEW_RADIUS_CHUNKS {
            return Err("view radius cannot be higher than 32 chunks");
        }
        self.max_view_radius
            .store(max_view_radius, Ordering::Relaxed);
        Ok(max_view_radius)
    }

    /// Clamp a requested radius to the current ceiling.
    pub fn clamp_view_radius(&self, requested: u32) -> u32 {
        requested.min(self.max_view_radius())
    }

    /// Whether another player may join when `players` are already connected.
    pub fn is_full(&self, players: usize) -> bool {
        let max_players = self.max_players();
        max_players > 0 && players >= max_players as usize
    }
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self::new(20, 16)
    }
}

#[cfg(test)]
mod tests {
    use super::{RuntimeLimits, MAX_VIEW_RADIUS_CHUNKS};

    #[test]
    fn limits_update_and_clamp_view_radius() {
        let limits = RuntimeLimits::new(10, 8);
        assert_eq!(limits.max_players(), 10);
        assert_eq!(limits.clamp_view_radius(12), 8);
        assert!(!limits.is_full(9));
        assert!(limits.is_full(10));

        assert_eq!(limits.set_max_players(0), 0);
        assert!(!limits.is_full(100), "zero players means unlimited");

        assert_eq!(limits.set_max_view_radius(2), Ok(2));
        assert_eq!(limits.clamp_view_radius(8), 2);
        assert!(limits.set_max_view_radius(0).is_err());
        assert!(limits
            .set_max_view_radius(MAX_VIEW_RADIUS_CHUNKS + 1)
            .is_err());
    }
}
