// Ported from: `.hytale-source/.../worldgen/util/LogUtil.java`
//! Java `com.hypixel.hytale.server.worldgen.util.LogUtil` — the world
//! generator's logger.
//!
//! Java uses the server's `HytaleLogger`; the Rust port uses the `log` crate
//! facade with the same logger name (`"WorldGenerator"`), so an embedder can
//! install any `log` implementation and filter on that target.

/// Java `HytaleLogger.get("WorldGenerator")` — the logger name / target.
pub const LOGGER_NAME: &str = "WorldGenerator";

/// Java `LOGGER.at(Level.SEVERE).log(msg)`.
pub fn error(msg: &str) {
    log::error!(target: LOGGER_NAME, "{msg}");
}

/// Java `LOGGER.at(Level.WARNING).log(msg)`.
pub fn warn(msg: &str) {
    log::warn!(target: LOGGER_NAME, "{msg}");
}

/// Java `LOGGER.at(Level.INFO).log(msg)`.
pub fn info(msg: &str) {
    log::info!(target: LOGGER_NAME, "{msg}");
}

/// Java `LOGGER.at(Level.FINE).log(msg)`.
pub fn debug(msg: &str) {
    log::debug!(target: LOGGER_NAME, "{msg}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logger_name_matches_java() {
        assert_eq!(LOGGER_NAME, "WorldGenerator");
    }

    #[test]
    fn logging_calls_do_not_panic() {
        // The `log` facade is a no-op without an installed logger.
        error("error");
        warn("warn");
        info("info");
        debug("debug");
    }
}
