//! Java-compatible BSON chunk storage and indexed region files.
//!
//! Ported from `CoreServer/.../world/storage/BufferChunkSaver`,
//! `BufferChunkLoader`, `ChunkStore`'s holder codec and the
//! `IndexedStorageChunkStorageProvider` region layout.
//!
//! A stored chunk is a BSON document with a `Components` map keyed by the Java
//! component ids:
//!
//! ```text
//! {
//!   "Components": {
//!     "WorldChunk": {},
//!     "BlockChunk": {"Version": 4, "Data": <needsPhysics + tint>},
//!     "ChunkColumn": {"Sections": [null | {"Components": {...}}, ...]},
//!     "HeightmapColumn": {"Heightmap": {"Id": "Legacy", "Height": <bytes>}}
//!   }
//! }
//! ```
//!
//! Section holders carry `Block` (`BlockSection`, version 7), `Fluid`
//! (`FluidSection`, version 0) and `Environment` (`EnvironmentSection`,
//! version 0) components. The byte payloads are the storage layouts already
//! implemented by the section modules; block, fluid and environment palette
//! keys are asset names resolved through the P0 asset registry.
//!
//! Region files are `chunks/<regionX>.<regionZ>.region.bin`; chunk
//! `(x, z)` lives in region `(x >> 5, z >> 5)` at blob index
//! `ChunkUtil.indexColumn(x & 31, z & 31)`.

use std::path::{Path, PathBuf};

use bson::spec::BinarySubtype;
use bson::{Binary, Bson, Document};
use hytale_assets::registry::AssetRegistry;
use hytale_math::chunk_util::{BITS, SIZE, SIZE_MASK};
use hytale_storage::IndexedStorageFile;
use thiserror::Error;

use crate::chunk::column::{ChunkColumn, Heightmap, NO_HEIGHT, SECTION_COUNT};
use crate::chunk::palette::{NameLookup, NamedKey};
use crate::chunk::section::{BlockSection, EnvironmentSection, FluidSection, SectionError};
use crate::chunk::tint::TintPalette;

/// Version written for `BlockChunk`.
pub const BLOCK_CHUNK_VERSION: i32 = 4;
/// Version written for `Block`.
pub const BLOCK_SECTION_VERSION: i32 = 7;
/// Version written for `Fluid`.
pub const FLUID_SECTION_VERSION: i32 = 0;
/// Version written for `Environment`.
pub const ENVIRONMENT_SECTION_VERSION: i32 = 0;
/// Default blob count of a region file.
pub const REGION_BLOB_COUNT: u32 = hytale_storage::DEFAULT_BLOB_COUNT;
/// Default segment size of a region file.
pub const REGION_SEGMENT_SIZE: u32 = hytale_storage::DEFAULT_SEGMENT_SIZE;

/// Errors from the chunk storage codecs and region files.
#[derive(Debug, Error)]
pub enum ChunkStorageError {
    /// BSON decoding failed.
    #[error("invalid chunk BSON: {0}")]
    Bson(#[from] bson::de::Error),
    /// BSON encoding failed.
    #[error("failed to encode chunk BSON: {0}")]
    BsonSerialize(#[from] bson::ser::Error),
    /// The document is missing a required field or holds the wrong type.
    #[error("invalid chunk document: {0}")]
    InvalidDocument(String),
    /// A section payload was malformed.
    #[error(transparent)]
    Section(#[from] SectionError),
    /// The indexed storage file failed.
    #[error(transparent)]
    Storage(#[from] hytale_storage::StorageError),
    /// The BSON byte array was not a binary value.
    #[error("component {component}.{field} is not a BSON binary array")]
    NotBinary {
        /// Component id.
        component: &'static str,
        /// Field name.
        field: &'static str,
    },
    /// Underlying filesystem operation failed.
    #[error("storage path error: {0}")]
    Io(#[from] std::io::Error),
    /// The stored value uses a layout this port does not implement yet.
    #[error("unsupported storage layout: {0}")]
    Unsupported(&'static str),
}

/// Result alias.
pub type Result<T> = std::result::Result<T, ChunkStorageError>;

// ---------------------------------------------------------------------------
// Registry key lookups
// ---------------------------------------------------------------------------

/// Asset-name lookup over the P0 registry for one store.
pub struct RegistryNames<'a> {
    registry: &'a AssetRegistry,
    kind: StoreKind,
    fallback_name: &'a str,
    fallback_id: i32,
}

#[derive(Clone, Copy)]
enum StoreKind {
    Block,
    Fluid,
    Environment,
}

impl<'a> RegistryNames<'a> {
    /// Block names.
    pub fn blocks(registry: &'a AssetRegistry) -> Self {
        Self {
            registry,
            kind: StoreKind::Block,
            fallback_name: "Unknown",
            fallback_id: unknown_id(&registry.block_types, "Unknown"),
        }
    }

    /// Fluid names.
    pub fn fluids(registry: &'a AssetRegistry) -> Self {
        Self {
            registry,
            kind: StoreKind::Fluid,
            fallback_name: "Unknown",
            fallback_id: unknown_id(&registry.fluids, "Unknown"),
        }
    }

    /// Environment names.
    pub fn environments(registry: &'a AssetRegistry) -> Self {
        Self {
            registry,
            kind: StoreKind::Environment,
            fallback_name: "Unknown",
            fallback_id: unknown_id(&registry.environments, "Unknown"),
        }
    }

    /// Name written for ids missing from the store.
    pub const fn fallback_name(&self) -> &'a str {
        self.fallback_name
    }

    /// Id returned for names missing from the store.
    pub const fn fallback_id(&self) -> i32 {
        self.fallback_id
    }
}

fn unknown_id<T>(collection: &hytale_assets::registry::AssetCollection<T>, key: &str) -> i32 {
    collection.id_of(key).unwrap_or(0)
}

impl NameLookup for RegistryNames<'_> {
    fn name_of(&self, id: i32) -> Option<&str> {
        match self.kind {
            StoreKind::Block => self
                .registry
                .block_types
                .get_by_id(id)
                .map(|r| r.key.as_str()),
            StoreKind::Fluid => self.registry.fluids.get_by_id(id).map(|r| r.key.as_str()),
            StoreKind::Environment => self
                .registry
                .environments
                .get_by_id(id)
                .map(|r| r.key.as_str()),
        }
    }

    fn id_of(&self, name: &str) -> Option<i32> {
        match self.kind {
            StoreKind::Block => self.registry.block_types.id_of(name),
            StoreKind::Fluid => self.registry.fluids.id_of(name),
            StoreKind::Environment => self.registry.environments.id_of(name),
        }
    }

    fn fallback_name(&self) -> &str {
        self.fallback_name
    }

    fn fallback_id(&self) -> i32 {
        self.fallback_id
    }
}

// ---------------------------------------------------------------------------
// Chunk BSON codec
// ---------------------------------------------------------------------------

/// BSON chunk codec bound to name/id lookups.
pub struct ChunkCodec<'a> {
    blocks: LookupRef<'a>,
    fluids: LookupRef<'a>,
    environments: LookupRef<'a>,
}

enum LookupRef<'a> {
    Registry(RegistryNames<'a>),
    Borrowed(&'a dyn NameLookup),
}

impl NameLookup for LookupRef<'_> {
    fn name_of(&self, id: i32) -> Option<&str> {
        match self {
            Self::Registry(lookup) => lookup.name_of(id),
            Self::Borrowed(lookup) => lookup.name_of(id),
        }
    }

    fn id_of(&self, name: &str) -> Option<i32> {
        match self {
            Self::Registry(lookup) => lookup.id_of(name),
            Self::Borrowed(lookup) => lookup.id_of(name),
        }
    }
}

impl<'a> ChunkCodec<'a> {
    /// Create a codec backed by the P0 asset registry.
    pub fn new(registry: &'a AssetRegistry) -> Self {
        Self {
            blocks: LookupRef::Registry(RegistryNames::blocks(registry)),
            fluids: LookupRef::Registry(RegistryNames::fluids(registry)),
            environments: LookupRef::Registry(RegistryNames::environments(registry)),
        }
    }

    /// Create a codec from explicit name lookups.
    ///
    /// This is used by tests that decode a Java fixture with the golden
    /// registry ids (the world generator currently resolves its block ids from
    /// the golden registry dump).
    pub fn from_lookups(
        blocks: &'a dyn NameLookup,
        fluids: &'a dyn NameLookup,
        environments: &'a dyn NameLookup,
    ) -> Self {
        Self {
            blocks: LookupRef::Borrowed(blocks),
            fluids: LookupRef::Borrowed(fluids),
            environments: LookupRef::Borrowed(environments),
        }
    }

    /// Encode a runtime column as the Java `ChunkStore` holder document.
    pub fn encode(&self, column: &ChunkColumn) -> Result<Vec<u8>> {
        let block_names = &self.blocks;
        let fluid_names = &self.fluids;
        let environment_names = &self.environments;

        let mut sections = Vec::with_capacity(SECTION_COUNT);
        for section_index in 0..SECTION_COUNT {
            let section = column.section(section_index as i32 * 32);
            let components = match section {
                Some(section) => {
                    let block_data = section.block.serialize_with_keys(
                        &NamedKey::new(
                            block_names,
                            block_names.fallback_name(),
                            block_names.fallback_id(),
                        ),
                        &crate::chunk::palette::ShortKey,
                        &crate::chunk::palette::ByteKey,
                    )?;
                    let fluid_data = section.fluid.serialize_with_key(&NamedKey::new(
                        fluid_names,
                        fluid_names.fallback_name(),
                        fluid_names.fallback_id(),
                    ))?;
                    let environment_data = section.environment.serialize_with_lookup(
                        environment_names,
                        environment_names.fallback_name(),
                    )?;

                    let mut components = Document::new();
                    components.insert(
                        "Block",
                        Bson::Document(versioned_bytes(BLOCK_SECTION_VERSION, block_data)),
                    );
                    components.insert("Fluid", Bson::Document(component_bytes(fluid_data)));
                    components.insert(
                        "Environment",
                        Bson::Document(component_bytes(environment_data)),
                    );
                    components
                }
                None => empty_section_components(environment_names)?,
            };

            let mut holder = Document::new();
            holder.insert("Components", Bson::Document(components));
            sections.push(Bson::Document(holder));
        }

        let mut block_chunk_data = Vec::with_capacity(1 + column.tint_bytes().len());
        block_chunk_data.push(u8::from(column.needs_physics()));
        block_chunk_data.extend_from_slice(&column.tint_bytes());

        let mut chunk_components = Document::new();
        chunk_components.insert("WorldChunk", Bson::Document(Document::new()));
        chunk_components.insert(
            "BlockChunk",
            Bson::Document(versioned_bytes(BLOCK_CHUNK_VERSION, block_chunk_data)),
        );
        chunk_components.insert(
            "ChunkColumn",
            Bson::Document(bson::doc! { "Sections": Bson::Array(sections) }),
        );
        chunk_components.insert(
            "HeightmapColumn",
            Bson::Document({
                let mut document = Document::new();
                document.insert(
                    "Heightmap",
                    Bson::Document(encode_heightmap(column.heightmap())?),
                );
                document
            }),
        );

        let mut document = Document::new();
        document.insert("Components", Bson::Document(chunk_components));
        let mut bytes = Vec::new();
        document.to_writer(&mut bytes)?;
        Ok(bytes)
    }

    /// Decode a Java `ChunkStore` holder document.
    pub fn decode(&self, bytes: &[u8]) -> Result<ChunkColumn> {
        let document = Document::from_reader(bytes)?;
        let components = component_map(&document, "chunk")?;
        let block_names = &self.blocks;
        let fluid_names = &self.fluids;
        let environment_names = &self.environments;

        let mut column = ChunkColumn::new(0, 0);
        column.flags_mut().on_disk = true;

        if let Ok(block_chunk) = components.get_document("BlockChunk") {
            let data = component_bytes_field(block_chunk, "BlockChunk", "Data")?;
            if data.is_empty() {
                return Err(ChunkStorageError::InvalidDocument(
                    "BlockChunk.Data is empty".to_string(),
                ));
            }
            column.set_needs_physics(data[0] != 0);
            let tint = TintPalette::deserialize(&data[1..]).ok_or_else(|| {
                ChunkStorageError::InvalidDocument("invalid BlockChunk tint palette".to_string())
            })?;
            column.set_tint_palette(tint);
        }

        if let Ok(column_component) = components.get_document("ChunkColumn") {
            if let Some(Bson::Array(section_values)) = column_component.get("Sections") {
                for (section_index, value) in section_values.iter().enumerate() {
                    if section_index >= SECTION_COUNT {
                        break;
                    }
                    let Bson::Document(holder) = value else {
                        continue;
                    };
                    let section_components = component_map(holder, "section")?;

                    let section = column
                        .ensure_section(section_index as i32 * 32)
                        .expect("section index is in range");

                    if let Ok(block) = section_components.get_document("Block") {
                        require_version(block, "Block", 6)?;
                        let data = component_bytes_field(block, "Block", "Data")?;
                        section.block = BlockSection::deserialize_with_keys(
                            data,
                            &NamedKey::new(
                                block_names,
                                block_names.fallback_name(),
                                block_names.fallback_id(),
                            ),
                            &crate::chunk::palette::ShortKey,
                            &crate::chunk::palette::ByteKey,
                        )?;
                    }

                    if let Ok(fluid) = section_components.get_document("Fluid") {
                        let data = component_bytes_field(fluid, "Fluid", "Data")?;
                        section.fluid = FluidSection::deserialize_with_key(
                            data,
                            &NamedKey::new(
                                fluid_names,
                                fluid_names.fallback_name(),
                                fluid_names.fallback_id(),
                            ),
                        )?;
                    }

                    if let Ok(environment) = section_components.get_document("Environment") {
                        let data = component_bytes_field(environment, "Environment", "Data")?;
                        section.environment = EnvironmentSection::deserialize_with_lookup(
                            data,
                            environment_names,
                            environment_names.fallback_id(),
                        )?;
                    }
                }
            }
        }

        if let Ok(heightmap_column) = components.get_document("HeightmapColumn") {
            if let Some(Bson::Document(heightmap)) = heightmap_column.get("Heightmap") {
                column.set_heightmap(decode_heightmap(heightmap)?);
            }
        }

        Ok(column)
    }
}

fn empty_section_components(environment_names: &dyn NameLookup) -> Result<Document> {
    let mut components = Document::new();
    components.insert(
        "Block",
        Bson::Document(versioned_bytes(
            BLOCK_SECTION_VERSION,
            BlockSection::new().serialize_with_keys(
                &crate::chunk::palette::ShortKey,
                &crate::chunk::palette::ShortKey,
                &crate::chunk::palette::ByteKey,
            )?,
        )),
    );
    components.insert(
        "Fluid",
        Bson::Document(component_bytes(FluidSection::new().serialize()?)),
    );
    components.insert(
        "Environment",
        Bson::Document(component_bytes(
            EnvironmentSection::uniform(0)
                .serialize_with_lookup(environment_names, environment_names.fallback_name())?,
        )),
    );
    Ok(components)
}

fn versioned_bytes(version: i32, data: Vec<u8>) -> Document {
    let mut document = component_bytes(data);
    document.insert("Version", Bson::Int32(version));
    document
}

fn component_bytes(data: Vec<u8>) -> Document {
    let mut document = Document::new();
    document.insert(
        "Data",
        Bson::Binary(Binary {
            subtype: BinarySubtype::Generic,
            bytes: data,
        }),
    );
    document
}

fn component_map<'a>(document: &'a Document, context: &str) -> Result<&'a Document> {
    match document.get("Components") {
        Some(Bson::Document(components)) => Ok(components),
        Some(_) => Err(ChunkStorageError::InvalidDocument(format!(
            "{context}.Components is not a document"
        ))),
        None => Err(ChunkStorageError::InvalidDocument(format!(
            "{context}.Components is missing"
        ))),
    }
}

fn component_bytes_field<'a>(
    document: &'a Document,
    component: &'static str,
    field: &'static str,
) -> Result<&'a [u8]> {
    match document.get(field) {
        Some(Bson::Binary(binary)) => Ok(&binary.bytes),
        Some(_) => Err(ChunkStorageError::NotBinary { component, field }),
        None => Err(ChunkStorageError::InvalidDocument(format!(
            "{component}.{field} is missing"
        ))),
    }
}

fn require_version(document: &Document, component: &str, minimum: i32) -> Result<()> {
    let version = match document.get("Version") {
        Some(Bson::Int32(version)) => *version,
        Some(Bson::Int64(version)) => *version as i32,
        _ => 0,
    };
    if version < minimum {
        return Err(ChunkStorageError::InvalidDocument(format!(
            "{component} version {version} is older than {minimum}"
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Legacy heightmap
// ---------------------------------------------------------------------------

fn encode_heightmap(heightmap: &Heightmap) -> Result<Document> {
    let mut document = Document::new();
    match heightmap {
        Heightmap::Legacy(heights) => {
            document.insert("Id", Bson::String("Legacy".to_string()));
            document.insert("Height", Bson::Binary(binary(legacy_height_bytes(heights))));
            Ok(document)
        }
        Heightmap::Tree(_) => Err(ChunkStorageError::Unsupported(
            "Tree heightmap (only the legacy backend is ported)",
        )),
    }
}

fn decode_heightmap(document: &Document) -> Result<Heightmap> {
    let id = match document.get("Id") {
        Some(Bson::String(id)) => id.as_str(),
        _ => "Legacy",
    };
    match id {
        "Legacy" | "" => {
            let bytes = match document.get("Height") {
                Some(Bson::Binary(binary)) => &binary.bytes,
                Some(_) => {
                    return Err(ChunkStorageError::InvalidDocument(
                        "Heightmap.Height is not binary".to_string(),
                    ))
                }
                None => {
                    return Err(ChunkStorageError::InvalidDocument(
                        "Heightmap.Height is missing".to_string(),
                    ))
                }
            };
            Ok(Heightmap::Legacy(parse_legacy_height_bytes(bytes)?))
        }
        "Tree" => Err(ChunkStorageError::Unsupported(
            "Tree heightmap (only the legacy backend is ported)",
        )),
        other => Err(ChunkStorageError::InvalidDocument(format!(
            "unknown heightmap backend {other}"
        ))),
    }
}

/// Serialize a legacy heightmap exactly like `LegacyHeightmap`'s
/// `ShortBytePalette`: little-endian key count, keys, byte length and packed
/// 10-bit internal ids (LSB-first).
fn legacy_height_bytes(heights: &[i32]) -> Vec<u8> {
    const LENGTH: usize = 32 * 32;
    const BITS: usize = 10;
    const PACKED_BYTES: usize = (LENGTH * BITS).div_ceil(8);
    const NULL_KEY: i16 = i16::MIN;

    let mut keys: Vec<i16> = Vec::new();
    let mut internals = Vec::with_capacity(heights.len());
    for &height in heights.iter().take(LENGTH) {
        let key = if height == NO_HEIGHT || height < 0 {
            NULL_KEY
        } else if height >= 320 {
            319
        } else {
            height as i16
        };
        let internal = match keys.iter().position(|candidate| *candidate == key) {
            Some(internal) => internal,
            None => {
                keys.push(key);
                keys.len() - 1
            }
        };
        internals.push(internal as u16);
    }
    internals.resize(LENGTH, 0);
    if keys.is_empty() {
        keys.push(NULL_KEY);
    }

    let mut bytes = Vec::with_capacity(2 + keys.len() * 2 + 4 + PACKED_BYTES);
    bytes.extend_from_slice(&(keys.len() as u16).to_le_bytes());
    for key in &keys {
        bytes.extend_from_slice(&key.to_le_bytes());
    }
    bytes.extend_from_slice(&(PACKED_BYTES as i32).to_le_bytes());

    let mut packed = vec![0u8; PACKED_BYTES];
    for (index, internal) in internals.iter().enumerate() {
        let bit = index * BITS;
        let byte = bit / 8;
        let offset = bit % 8;
        let value = u32::from(*internal);
        packed[byte] |= (value << offset) as u8;
        if offset + BITS > 8 {
            packed[byte + 1] |= (value >> (8 - offset)) as u8;
        }
        if offset + BITS > 16 {
            packed[byte + 2] |= (value >> (16 - offset)) as u8;
        }
    }
    bytes.extend_from_slice(&packed);
    bytes
}

fn parse_legacy_height_bytes(bytes: &[u8]) -> Result<Vec<i32>> {
    const LENGTH: usize = 32 * 32;
    const BITS: usize = 10;
    const NULL_KEY: i16 = i16::MIN;

    if bytes.len() < 6 {
        return Err(ChunkStorageError::InvalidDocument(
            "legacy heightmap is truncated".to_string(),
        ));
    }
    let count = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;
    if count == 0 || count > LENGTH {
        return Err(ChunkStorageError::InvalidDocument(
            "legacy heightmap key count is out of range".to_string(),
        ));
    }
    let key_end = 2 + count * 2;
    if bytes.len() < key_end + 4 {
        return Err(ChunkStorageError::InvalidDocument(
            "legacy heightmap is truncated".to_string(),
        ));
    }
    let mut keys = Vec::with_capacity(count);
    for index in 0..count {
        let start = 2 + index * 2;
        keys.push(i16::from_le_bytes([bytes[start], bytes[start + 1]]));
    }
    let packed_len = i32::from_le_bytes(bytes[key_end..key_end + 4].try_into().unwrap()) as usize;
    let packed_start = key_end + 4;
    if bytes.len() < packed_start + packed_len {
        return Err(ChunkStorageError::InvalidDocument(
            "legacy heightmap packed array is truncated".to_string(),
        ));
    }
    let packed = &bytes[packed_start..packed_start + packed_len];
    let mut heights = vec![NO_HEIGHT; LENGTH];
    for (index, height) in heights.iter_mut().enumerate() {
        let bit = index * BITS;
        let byte = bit / 8;
        let offset = bit % 8;
        if byte >= packed.len() {
            break;
        }
        let mut accumulator = 0u32;
        for shift in 0..3 {
            if let Some(value) = packed.get(byte + shift) {
                accumulator |= u32::from(*value) << (shift * 8);
            }
        }
        let internal = ((accumulator >> offset) & ((1 << BITS) - 1)) as usize;
        let key = *keys.get(internal).ok_or_else(|| {
            ChunkStorageError::InvalidDocument(
                "legacy heightmap internal id out of range".to_string(),
            )
        })?;
        *height = if key == NULL_KEY {
            NO_HEIGHT
        } else {
            i32::from(key)
        };
    }
    Ok(heights)
}

fn binary(bytes: Vec<u8>) -> Binary {
    Binary {
        subtype: BinarySubtype::Generic,
        bytes,
    }
}

// ---------------------------------------------------------------------------
// Region files
// ---------------------------------------------------------------------------

/// Region-file coordinates for a chunk.
pub const fn region_coords(chunk_x: i32, chunk_z: i32) -> (i32, i32) {
    (chunk_x >> BITS, chunk_z >> BITS)
}

/// Blob index of a chunk inside its region (`ChunkUtil.indexColumn`).
pub const fn chunk_blob_index(chunk_x: i32, chunk_z: i32) -> u32 {
    ((chunk_z & SIZE_MASK) << BITS | (chunk_x & SIZE_MASK)) as u32
}

/// Region file name (`<regionX>.<regionZ>.region.bin`).
pub fn region_file_name(region_x: i32, region_z: i32) -> String {
    format!("{region_x}.{region_z}.region.bin")
}

/// Path of the region file that owns `(chunk_x, chunk_z)` inside a world save
/// directory.
pub fn region_path(save_path: &Path, chunk_x: i32, chunk_z: i32) -> PathBuf {
    let (region_x, region_z) = region_coords(chunk_x, chunk_z);
    save_path
        .join("chunks")
        .join(region_file_name(region_x, region_z))
}

/// One Java region file (`IndexedStorageFile` with 1024 blobs).
pub struct RegionFile {
    file: IndexedStorageFile,
}

impl RegionFile {
    /// Open an existing region file, or create it when it is missing.
    pub fn open(path: &Path) -> Result<Self> {
        let file = if path.exists() {
            IndexedStorageFile::open(path, false)?
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            IndexedStorageFile::create(path, REGION_BLOB_COUNT, REGION_SEGMENT_SIZE)?
        };
        Ok(Self { file })
    }

    /// Open an existing region file read-only.
    pub fn open_read_only(path: &Path) -> Result<Self> {
        Ok(Self {
            file: IndexedStorageFile::open(path, true)?,
        })
    }

    /// Open the region file for a chunk inside a world save directory.
    pub fn open_for_chunk(save_path: &Path, chunk_x: i32, chunk_z: i32) -> Result<Self> {
        Self::open(&region_path(save_path, chunk_x, chunk_z))
    }

    /// Read a chunk document, or `None` when the slot is empty.
    pub fn read_chunk(&mut self, chunk_x: i32, chunk_z: i32) -> Result<Option<Vec<u8>>> {
        Ok(self.file.read_blob(chunk_blob_index(chunk_x, chunk_z))?)
    }

    /// Write a chunk document.
    pub fn write_chunk(&mut self, chunk_x: i32, chunk_z: i32, data: &[u8]) -> Result<()> {
        self.file
            .write_blob(chunk_blob_index(chunk_x, chunk_z), data)?;
        Ok(())
    }

    /// Remove a chunk document.
    pub fn remove_chunk(&mut self, chunk_x: i32, chunk_z: i32) -> Result<()> {
        self.file.remove_blob(chunk_blob_index(chunk_x, chunk_z))?;
        Ok(())
    }

    /// Flush the underlying file.
    pub fn flush(&mut self) -> Result<()> {
        self.file.flush()?;
        Ok(())
    }

    /// Blob indexes present in the region.
    pub fn keys(&self) -> Vec<u32> {
        self.file.keys()
    }

    /// Chunk x coordinate from a blob index inside this region.
    pub const fn x_from_index(index: u32) -> i32 {
        (index as i32) & SIZE_MASK
    }

    /// Chunk z coordinate from a blob index inside this region.
    pub const fn z_from_index(index: u32) -> i32 {
        ((index as i32) >> BITS) & SIZE_MASK
    }
}

/// Number of chunks per region edge (32).
pub const REGION_SIZE: i32 = SIZE;

#[cfg(test)]
mod tests {
    use super::*;

    struct TestNames;

    impl NameLookup for TestNames {
        fn name_of(&self, id: i32) -> Option<&str> {
            Some(Box::leak(format!("id_{id}").into_boxed_str()))
        }

        fn id_of(&self, name: &str) -> Option<i32> {
            name.strip_prefix("id_")?.parse().ok()
        }
    }

    #[test]
    fn legacy_heightmap_round_trips_all_columns() {
        let mut heights = vec![NO_HEIGHT; 32 * 32];
        for (index, height) in heights.iter_mut().enumerate() {
            *height = (index % 320) as i32;
        }
        heights[31] = NO_HEIGHT;
        heights[1023] = 127;
        let bytes = legacy_height_bytes(&heights);
        let decoded = parse_legacy_height_bytes(&bytes).expect("parse the heightmap");
        assert_eq!(decoded, heights);
    }

    #[test]
    fn chunk_document_round_trips_named_palettes_without_assets() {
        let mut column = ChunkColumn::new(3, -4);
        let section = column.ensure_section(32).expect("section 1");
        section
            .block
            .set_block(0, 0, 0, 7, 3, 5)
            .expect("set a block");
        section.block.set_ticking(0, true);
        section.fluid.set_fluid(0, 9, 4).expect("set fluid");
        section.environment.set(1, 1, 1, 11);
        column.set_tint(2, 3, 0x1122_3344);
        column.set_needs_physics(false);
        let mut heights = vec![NO_HEIGHT; 32 * 32];
        heights[0] = 0;
        heights[1023] = 127;
        column.set_heightmap(Heightmap::Legacy(heights.clone()));

        let names = TestNames;
        let codec = ChunkCodec::from_lookups(&names, &names, &names);
        let bytes = codec.encode(&column).expect("encode");
        let decoded = codec.decode(&bytes).expect("decode");

        assert_eq!(decoded.get_block(0, 32, 0), 7);
        assert_eq!(decoded.get_filler(0, 32, 0), 5);
        assert_eq!(decoded.get_rotation(0, 32, 0), 3);
        assert_eq!(decoded.get_fluid_id(0, 32, 0), 9);
        assert_eq!(decoded.get_fluid_level(0, 32, 0), 4);
        assert_eq!(decoded.get_environment(1, 33, 1), 11);
        assert_eq!(decoded.tint(2, 3), 0x1122_3344);
        assert_eq!(decoded.heightmap(), &Heightmap::Legacy(heights));
        assert!(!decoded.needs_physics());
        assert!(decoded.section(32).is_some_and(|s| s.block.is_ticking(0)));
    }

    #[test]
    fn region_index_and_name_match_java() {
        assert_eq!(region_coords(0, 0), (0, 0));
        assert_eq!(region_coords(4, -5), (0, -1));
        assert_eq!(region_coords(63, 63), (1, 1));
        assert_eq!(chunk_blob_index(0, 0), 0);
        assert_eq!(chunk_blob_index(4, -5), (27 << 5) | 4);
        assert_eq!(region_file_name(0, -1), "0.-1.region.bin");
        assert_eq!(RegionFile::x_from_index(868), 4);
        assert_eq!(RegionFile::z_from_index(868), 27);
    }
}
