//! Runtime chunk model.
//!
//! `worldgen_model::GeneratedChunk` is the generation buffer; this module owns
//! the runtime section/palette model used by networking and storage.

pub mod column;
pub mod packet;
pub mod palette;
pub mod section;
pub mod storage;
pub mod tint;

pub use column::{
    ChunkColumn, ChunkFlags, ChunkSection, Heightmap, BLOCK_COUNT as COLUMN_BLOCK_COUNT,
    COLUMN_COUNT, NO_HEIGHT, SECTION_COUNT,
};
pub use palette::{
    ByteKey, KeyCodec, NameLookup, NamedKey, Palette, PaletteEntry, PaletteError, PaletteType,
    SetResult, ShortKey, BLOCK_COUNT, BYTE_MAX_SIZE, HALF_BYTE_MAX_SIZE, SHORT_MAX_SIZE,
};
pub use section::{
    BlockSection, BlockSectionPacket, EnvironmentSection, FluidSection, LightData,
    SectionEnvironmentsMode, SectionError,
};
pub use storage::{
    chunk_blob_index, parse_region_file_name, region_coords, region_file_name, region_path,
    ChunkCodec, ChunkStorageError, RegionFile, RegistryNames, BLOCK_CHUNK_VERSION,
    BLOCK_SECTION_VERSION, ENVIRONMENT_SECTION_VERSION, FLUID_SECTION_VERSION, REGION_BLOB_COUNT,
    REGION_SEGMENT_SIZE,
};
