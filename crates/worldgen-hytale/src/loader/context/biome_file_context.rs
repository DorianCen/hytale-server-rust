// Ported from: `.hytale-source/.../worldgen/loader/context/BiomeFileContext.java`
//! Java `loader.context.BiomeFileContext`.

use std::path::PathBuf;
use std::rc::Rc;

use super::file_context::FileContext;
use super::zone_file_context::ZoneFileContext;

/// Java `BiomeFileContext`.
pub struct BiomeFileContext {
    /// Java `extends FileContext<ZoneFileContext>`.
    pub base: FileContext,
    /// Java `private final Type type`.
    biome_type: BiomeType,
}

impl BiomeFileContext {
    /// Java `BiomeFileContext(int id, String name, Path filepath, Type type, ZoneFileContext parent)`.
    pub fn new(
        id: i32,
        name: &str,
        filepath: PathBuf,
        biome_type: BiomeType,
        parent: Rc<ZoneFileContext>,
    ) -> Rc<Self> {
        Rc::new(Self {
            base: FileContext::new(
                id,
                name.to_string(),
                filepath,
                super::file_context::ParentContext::Zone(parent),
            ),
            biome_type,
        })
    }

    /// Java `Type getType()`.
    pub fn get_type(&self) -> BiomeType {
        self.biome_type
    }

    /// Java `getParentContext()` — the owning zone context (typed).
    pub fn get_parent_context(&self) -> Rc<ZoneFileContext> {
        match &self.base.parent {
            super::file_context::ParentContext::Zone(zone) => zone.clone(),
            _ => unreachable!("BiomeFileContext parent is always a ZoneFileContext"),
        }
    }

    /// Java `static Type getBiomeType(AssetPath path)` — derives the type from
    /// the filename prefix (`Tile.` / `Custom.`); all types share the `.json`
    /// suffix.
    pub fn get_biome_type(file_name: &str) -> BiomeType {
        for type_ in [BiomeType::Tile, BiomeType::Custom] {
            if file_name.starts_with(type_.get_prefix()) && file_name.ends_with(type_.get_suffix())
            {
                return type_;
            }
        }
        panic!("Unable to determine biome Type from file: {file_name}");
    }
}

/// Java `BiomeFileContext.Type` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BiomeType {
    /// Java `Tile("Tile.", "Tile Biome")`.
    Tile,
    /// Java `Custom("Custom.", "Custom Biome")`.
    Custom,
}

impl BiomeType {
    /// Java `String getPrefix()`.
    pub fn get_prefix(&self) -> &'static str {
        match self {
            Self::Tile => "Tile.",
            Self::Custom => "Custom.",
        }
    }

    /// Java `String getSuffix()` — the enum constructor appends `".json"`.
    pub fn get_suffix(&self) -> &'static str {
        ".json"
    }

    /// Java `String getDisplayName()`.
    pub fn get_display_name(&self) -> &'static str {
        match self {
            Self::Tile => "Tile Biome",
            Self::Custom => "Custom Biome",
        }
    }
}
