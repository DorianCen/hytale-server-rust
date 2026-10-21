//! Runtime chunk model.
//!
//! `worldgen_model::GeneratedChunk` is the generation buffer; this module owns
//! the runtime section/palette model used by networking and storage.

pub mod palette;

pub use palette::{
    ByteKey, KeyCodec, Palette, PaletteEntry, PaletteError, PaletteType, SetResult, ShortKey,
    BLOCK_COUNT, BYTE_MAX_SIZE, HALF_BYTE_MAX_SIZE, SHORT_MAX_SIZE,
};
