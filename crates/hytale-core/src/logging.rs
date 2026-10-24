//! Structured logging initialization.

use thiserror::Error;

use crate::config::LoggingConfig;

/// Logging initialization errors.
#[derive(Debug, Error)]
pub enum LoggingError {
    /// A global subscriber is already installed in this process.
    #[error("logging is already initialized: {0}")]
    AlreadyInitialized(String),
}

/// Install the process-wide tracing subscriber from `config`.
///
/// Text mode is human-readable; `LogFormat::Json` emits one JSON object per
/// event. The function is idempotent only in the sense that the second call
/// reports [`LoggingError::AlreadyInitialized`] instead of replacing the
/// subscriber.
pub fn init_logging(config: &LoggingConfig) -> Result<(), LoggingError> {
    let level = config.level.as_tracing();
    let result = match config.format {
        crate::config::LogFormat::Text => tracing_subscriber::fmt()
            .with_max_level(level)
            .with_ansi(config.ansi)
            .try_init(),
        crate::config::LogFormat::Json => tracing_subscriber::fmt()
            .json()
            .with_max_level(level)
            .with_ansi(config.ansi)
            .try_init(),
    };
    result.map_err(|error| LoggingError::AlreadyInitialized(error.to_string()))
}

#[cfg(test)]
mod tests {
    use crate::config::{LogFormat, LogLevel};

    #[test]
    fn levels_map_to_tracing() {
        assert_eq!(LogLevel::Trace.as_tracing(), tracing::Level::TRACE);
        assert_eq!(LogLevel::Debug.as_tracing(), tracing::Level::DEBUG);
        assert_eq!(LogLevel::Info.as_tracing(), tracing::Level::INFO);
        assert_eq!(LogLevel::Warn.as_tracing(), tracing::Level::WARN);
        assert_eq!(LogLevel::Error.as_tracing(), tracing::Level::ERROR);
    }

    #[test]
    fn formats_parse() {
        assert!("text".parse::<LogFormat>().unwrap() == LogFormat::Text);
        assert!("JSON".parse::<LogFormat>().unwrap() == LogFormat::Json);
        assert!("yaml".parse::<LogFormat>().is_err());
    }
}
