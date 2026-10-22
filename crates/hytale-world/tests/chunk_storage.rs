//! Java-written world storage fixture: BSON chunk documents in real
//! `IndexedStorage` region files.
//!
//! Reference data: `testdata/golden/server/storage/java-world/`, produced by
//! `tools/java-dumper/run-storage.sh` (`StorageDumper`). The same seed-0
//! chunks as the chunk-packet goldens are generated in Rust, converted into the
//! runtime model, and compared field by field with the Java document decoded
//! through [`ChunkCodec`].

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use hytale_assets::registry::AssetRegistry;
use hytale_assets::AssetPack;
use hytale_world::chunk::column::{ChunkColumn, SECTION_COUNT};
use hytale_world::chunk::palette::NameLookup;
use hytale_world::chunk::storage::{chunk_blob_index, region_file_name, ChunkCodec, RegionFile};
use hytale_world::registry::registries_from_assets;
use serde_json::Value;
use worldgen_hytale::chunk::chunk_generator::ChunkGenerator;
use worldgen_hytale::chunk::chunk_generator_execution::ChunkGeneratorExecution;
use worldgen_hytale::chunk::chunk_generator_resource::ChunkGeneratorResource;
use worldgen_model::registry::Registries;

const GOLDEN_DIR: &str = "../../testdata/golden/server/storage/java-world";
const SEED: i32 = 0;

/// Name lookup over the golden registry dump.
///
/// The world generator still resolves its block ids from `registries.json`, so
/// the fixture comparison must use the same id table as the generated chunk.
struct GoldenLookup<'a> {
    registries: &'a Registries,
    store: GoldenStore,
}

#[derive(Clone, Copy)]
enum GoldenStore {
    Blocks,
    Fluids,
    Environments,
}

impl NameLookup for GoldenLookup<'_> {
    fn name_of(&self, id: i32) -> Option<&str> {
        match self.store {
            GoldenStore::Blocks => self.registries.blocks.name(id),
            GoldenStore::Fluids => self.registries.fluids.name(id),
            GoldenStore::Environments => self.registries.environments.name(id),
        }
    }

    fn id_of(&self, name: &str) -> Option<i32> {
        match self.store {
            GoldenStore::Blocks => self.registries.blocks.index_of(name),
            GoldenStore::Fluids => self.registries.fluids.index_of(name),
            GoldenStore::Environments => self.registries.environments.index_of(name),
        }
    }
}

fn manifest_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn golden_dir() -> PathBuf {
    manifest_root().join(GOLDEN_DIR)
}

fn assets_available() -> bool {
    manifest_root().join("../../Assets/Assets.zip").is_file()
}

fn load_registry() -> AssetRegistry {
    let pack = AssetPack::open(manifest_root().join("../../Assets/Assets.zip"))
        .expect("open the base-game pack");
    AssetRegistry::load_p0(&pack).expect("load the P0 asset registry")
}

fn default_world() -> Rc<ChunkGenerator> {
    let root = manifest_root().join("../../testdata/Default");
    let config = worldgen_hytale::world_gen_config::WorldGenConfig::new(
        root,
        "Default".to_string(),
        "0.0.0".to_string(),
    );
    let loader =
        worldgen_hytale::loader::chunk_generator_json_loader::ChunkGeneratorJsonLoader::new(
            procedural::json::SeedString::new("ChunkGenerator"),
            config,
        );
    loader.load()
}

fn generate_chunk(
    generator: &Rc<ChunkGenerator>,
    registries: &Rc<Registries>,
    x: i32,
    z: i32,
) -> ChunkColumn {
    let chunk = worldgen_model::chunk::GeneratedChunk::with_coordinates(
        hytale_math::chunk_util::index_chunk(x, z),
        x,
        z,
    );
    let mut execution = ChunkGeneratorExecution::with_registry(
        SEED,
        Rc::clone(generator),
        chunk,
        Rc::clone(registries),
    );
    let mut resource = ChunkGeneratorResource::new();
    execution.execute(SEED, &mut resource);
    ChunkColumn::from_generated_chunk_with_registries(execution.get_chunk(), registries)
        .expect("convert the generated chunk")
}

#[test]
fn java_world_region_decodes_to_the_generated_column() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let golden = golden_dir();
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(golden.join("manifest.json")).expect("read the storage manifest"),
    )
    .expect("parse the storage manifest");

    let registries = Rc::new(Registries::load());
    let generator = default_world();
    let blocks = GoldenLookup {
        registries: &registries,
        store: GoldenStore::Blocks,
    };
    let fluids = GoldenLookup {
        registries: &registries,
        store: GoldenStore::Fluids,
    };
    let environments = GoldenLookup {
        registries: &registries,
        store: GoldenStore::Environments,
    };
    let codec = ChunkCodec::from_lookups(&blocks, &fluids, &environments);

    let mut regions: BTreeMap<String, RegionFile> = BTreeMap::new();
    let mut compared = 0usize;

    for entry in manifest["chunks"].as_array().expect("chunks array") {
        let chunk_x = entry["chunk"][0].as_i64().expect("chunk x") as i32;
        let chunk_z = entry["chunk"][1].as_i64().expect("chunk z") as i32;
        let region_name = entry["region"].as_str().expect("region name").to_string();
        let index = entry["index"].as_u64().expect("blob index") as u32;
        assert_eq!(index, chunk_blob_index(chunk_x, chunk_z));

        let region = regions.entry(region_name.clone()).or_insert_with(|| {
            let path = golden.join("chunks").join(&region_name);
            RegionFile::open_read_only(&path).expect("open the Java region file")
        });
        let bytes = region
            .read_chunk(chunk_x, chunk_z)
            .expect("read the chunk document")
            .unwrap_or_else(|| {
                panic!("chunk ({chunk_x}, {chunk_z}) is missing from {region_name}")
            });

        let decoded = codec
            .decode(&bytes)
            .unwrap_or_else(|error| panic!("decode the Java chunk: {error}"));
        let generated = generate_chunk(&generator, &registries, chunk_x, chunk_z);

        compare_columns(chunk_x, chunk_z, &decoded, &generated);
        compared += 1;
    }

    assert_eq!(compared, 2);
}

fn compare_columns(chunk_x: i32, chunk_z: i32, decoded: &ChunkColumn, generated: &ChunkColumn) {
    for y in 0..(SECTION_COUNT as i32 * 32) {
        for z in 0..32 {
            for x in 0..32 {
                assert_eq!(
                    decoded.get_block(x, y, z),
                    generated.get_block(x, y, z),
                    "block ({x}, {y}, {z}) in chunk ({chunk_x}, {chunk_z})"
                );
                assert_eq!(
                    decoded.get_filler(x, y, z),
                    generated.get_filler(x, y, z),
                    "filler ({x}, {y}, {z}) in chunk ({chunk_x}, {chunk_z})"
                );
                assert_eq!(
                    decoded.get_rotation(x, y, z),
                    generated.get_rotation(x, y, z),
                    "rotation ({x}, {y}, {z}) in chunk ({chunk_x}, {chunk_z})"
                );
                assert_eq!(
                    decoded.get_fluid_id(x, y, z),
                    generated.get_fluid_id(x, y, z),
                    "fluid ({x}, {y}, {z}) in chunk ({chunk_x}, {chunk_z})"
                );
                assert_eq!(
                    decoded.get_fluid_level(x, y, z),
                    generated.get_fluid_level(x, y, z),
                    "fluid level ({x}, {y}, {z}) in chunk ({chunk_x}, {chunk_z})"
                );
                assert_eq!(
                    decoded.get_environment(x, y, z),
                    generated.get_environment(x, y, z),
                    "environment ({x}, {y}, {z}) in chunk ({chunk_x}, {chunk_z})"
                );
            }
        }
    }

    for z in 0..32 {
        for x in 0..32 {
            assert_eq!(
                decoded.tint(x, z),
                generated.tint(x, z),
                "tint ({x}, {z}) in chunk ({chunk_x}, {chunk_z})"
            );
        }
    }

    let (Some(decoded_heights), Some(generated_heights)) = (
        decoded.heightmap().serialize_to_ints(),
        generated.heightmap().serialize_to_ints(),
    ) else {
        panic!("both heightmaps are legacy");
    };
    let mut diffs = Vec::new();
    for (index, (decoded, generated)) in decoded_heights.iter().zip(&generated_heights).enumerate()
    {
        if decoded != generated {
            diffs.push((index, *decoded, *generated));
        }
    }
    assert!(
        diffs.is_empty(),
        "chunk ({chunk_x}, {chunk_z}) heightmap differences: {} of {} (first 8: {diffs:?}); blocks 31,31: {:?}",
        diffs.len(),
        decoded_heights.len(),
        (0..320).map(|y| (y, generated.get_block(31,y,31), decoded.get_block(31,y,31))).filter(|(_,g,d)| g != d || *g != 0 || *d != 0).collect::<Vec<_>>()
    );
    assert!(decoded.needs_physics());
}

#[test]
fn chunk_document_and_region_round_trip() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let assets = load_registry();
    let registries = Rc::new(registries_from_assets(&assets).expect("bridge the asset registry"));
    let generator = default_world();
    let codec = ChunkCodec::new(&assets);

    let (x, z) = (0, 0);
    let generated = generate_chunk(&generator, &registries, x, z);
    let bytes = codec.encode(&generated).expect("encode the chunk");
    let decoded = codec.decode(&bytes).expect("decode the chunk");
    // A generated palette's table order is not preserved across a load (Java's
    // deserialized palettes rebuild the fastutil map with its default sizing),
    // so compare the second cycle, which is the steady state of a saved world.
    let first_resave = codec.encode(&decoded).expect("re-encode the chunk");
    let reloaded = codec.decode(&first_resave).expect("decode the resave");
    let second_resave = codec.encode(&reloaded).expect("re-encode the reload");
    assert_eq!(first_resave, second_resave, "resave is stable");

    let dir = std::env::temp_dir().join(format!("hytale-region-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("chunks")).unwrap();
    let path = dir.join("chunks").join(region_file_name(0, 0));
    {
        let mut region = RegionFile::open(&path).expect("create the region");
        region.write_chunk(x, z, &bytes).expect("write the chunk");
        region.flush().expect("flush");
    }
    {
        let mut region = RegionFile::open(&path).expect("reopen the region");
        let stored = region.read_chunk(x, z).expect("read").expect("present");
        assert_eq!(stored, bytes);
        region.remove_chunk(x, z).expect("remove");
        assert!(region.read_chunk(x, z).expect("read").is_none());
    }
    std::fs::remove_dir_all(dir).ok();
}
