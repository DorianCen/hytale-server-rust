//! Runtime chunk sections.
//!
//! A chunk column holds ten 32³ sections. Each section combines the block,
//! fluid and environment components; the layouts mirror Java's `BlockSection`,
//! `FluidSection` and `EnvironmentSection` storage/packet formats.

pub mod block;
pub mod environment;
pub mod fluid;
pub mod light;

pub use block::{BlockSection, BlockSectionPacket};
pub use environment::{EnvironmentSection, SectionEnvironmentsMode};
pub use fluid::FluidSection;
pub use light::{LightData, LIGHT_BLOCK_COUNT};

use hytale_protocol::io::ProtocolError;

use crate::chunk::palette::PaletteError;

/// Errors from a section operation.
#[derive(Debug, thiserror::Error)]
pub enum SectionError {
    /// The palette rejected a value.
    #[error(transparent)]
    Palette(#[from] PaletteError),
    /// The wire/storage reader rejected the input.
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    /// A palette type byte had no declaration.
    #[error("invalid palette type {0}")]
    InvalidPaletteType(u8),
    /// The input did not match the section layout.
    #[error("invalid section data: {0}")]
    InvalidData(&'static str),
}

/// Result alias for sections.
pub type Result<T> = std::result::Result<T, SectionError>;
