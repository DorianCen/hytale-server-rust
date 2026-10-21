//! Runtime chunk model.
//!
//! `worldgen_model::GeneratedChunk` is the generation buffer; this module owns
//! the runtime section/palette model used by networking and storage.

pub mod column;
pub mod palette;
pub mod section;

pub use column::{
    ChunkColumn, ChunkFlags, ChunkSection, Heightmap, BLOCK_COUNT as COLUMN_BLOCK_COUNT,
    COLUMN_COUNT, SECTION_COUNT,
};
pub use palette::{
    ByteKey, KeyCodec, Palette, PaletteEntry, PaletteError, PaletteType, SetResult, ShortKey,
    BLOCK_COUNT, BYTE_MAX_SIZE, HALF_BYTE_MAX_SIZE, SHORT_MAX_SIZE,
};
pub use section::{
    BlockSection, BlockSectionPacket, EnvironmentSection, FluidSection, LightData,
    SectionEnvironmentsMode, SectionError,
};
