//! Golden parity for the initial chunk network packets.
//!
//! Reference data: `testdata/golden/server/chunks-pkts/`, produced by the real
//! Java `ChunkPacketDumper` from seed 0 chunks `(0,0)` and `(4,-5)` (see the
//! fixture README). The same generator boot as `worldgen-hytale/tests/
//! chunks_golden.rs` produces the Rust chunk, it is converted through
//! [`ChunkColumn::from_generated_chunk_with_registries`], and each packet is
//! encoded by the generated protocol types and compared byte-for-byte.
//!
//! `sentAtInit == true` entries (`SetColumn`, `SetChunk`,
//! `SetChunkEnvironments`) are the initial join set. `SetChunkTintmap` and
//! `SetFluids` are real Java serializations of the same chunk state but are
//! only sent on updates; they are checked too, for encoder coverage.

use std::path::PathBuf;
use std::rc::Rc;

use hytale_protocol::generated::wire::Wire;
use hytale_protocol::io::Writer;
use hytale_world::chunk::column::ChunkColumn;
use serde_json::Value;
use worldgen_hytale::chunk::chunk_generator::ChunkGenerator;
use worldgen_hytale::chunk::chunk_generator_execution::ChunkGeneratorExecution;
use worldgen_hytale::chunk::chunk_generator_resource::ChunkGeneratorResource;
use worldgen_model::registry::Registries;

const GOLDEN_DIR: &str = "../../testdata/golden/server/chunks-pkts";
const SEED: i32 = 0;

fn manifest_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn golden_dir() -> PathBuf {
    manifest_root().join(GOLDEN_DIR)
}

/// The golden chunks need the base-game prefabs. CI has no copy of the pack
/// (it is git-ignored), so the test is skipped there; the committed fixture
/// still gates every developer run and the full tier.
fn assets_available() -> bool {
    manifest_root()
        .join("../../Assets/extracted/Server/Prefabs")
        .is_dir()
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
) -> ChunkGeneratorExecution {
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
    execution
}

fn encode<T: Wire>(packet: &T) -> Vec<u8> {
    let mut writer = Writer::new();
    packet.encode(&mut writer).expect("encode the packet");
    writer.into_inner()
}

fn compare(name: &str, section: Option<(i64, i64, i64)>, expected: &[u8], actual: &[u8]) {
    if expected == actual {
        return;
    }
    let first = expected.iter().zip(actual.iter()).position(|(a, b)| a != b);
    panic!(
        "{name} {section:?} differs: golden {} bytes, rust {} bytes, first difference {first:?}\n\
         golden {:02x?}\nrust   {:02x?}",
        expected.len(),
        actual.len(),
        &expected
            [first.unwrap_or(0).saturating_sub(16)..expected.len().min(first.unwrap_or(0) + 48)],
        &actual[first.unwrap_or(0).saturating_sub(16)..actual.len().min(first.unwrap_or(0) + 48)],
    );
}

#[test]
fn the_initial_chunk_packets_match_the_java_serializers() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let golden = golden_dir();
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(golden.join("manifest.json")).expect("read the manifest"),
    )
    .expect("parse the manifest");
    assert_eq!(manifest["seed"].as_i64(), Some(SEED as i64));

    let registries = Rc::new(Registries::load());
    let generator = default_world();

    let mut compared = 0usize;
    for chunk in manifest["chunks"].as_array().expect("chunks array") {
        let chunk_x = chunk["chunk"][0].as_i64().expect("chunk x") as i32;
        let chunk_z = chunk["chunk"][1].as_i64().expect("chunk z") as i32;
        let execution = generate_chunk(&generator, &registries, chunk_x, chunk_z);
        let column =
            ChunkColumn::from_generated_chunk_with_registries(execution.get_chunk(), &registries)
                .expect("convert the generated chunk");

        for packet in chunk["packets"].as_array().expect("packets array") {
            let name = packet["name"].as_str().expect("packet name");
            let section = packet.get("section").map(|section| {
                (
                    section[0].as_i64().expect("section x"),
                    section[1].as_i64().expect("section y"),
                    section[2].as_i64().expect("section z"),
                )
            });
            let expected = std::fs::read(golden.join(packet["file"].as_str().expect("file")))
                .expect("read the golden payload");
            assert_eq!(
                expected.len(),
                packet["size"].as_u64().expect("size") as usize,
                "manifest size for {name} {section:?}"
            );

            let actual = match name {
                "SetColumn" => encode(&column.set_column_packet().expect("SetColumn")),
                "SetChunkTintmap" => encode(&column.tintmap_packet()),
                "SetChunk" => {
                    let (_, section_y, _) = section.expect("SetChunk section");
                    encode(&column.set_chunk_packet(section_y as i32).expect("SetChunk"))
                }
                "SetChunkEnvironments" => {
                    let (_, section_y, _) = section.expect("SetChunkEnvironments section");
                    encode(
                        &column
                            .set_chunk_environments_packet(section_y as i32)
                            .expect("SetChunkEnvironments"),
                    )
                }
                "SetFluids" => {
                    let (_, section_y, _) = section.expect("SetFluids section");
                    encode(
                        &column
                            .set_fluids_packet(section_y as i32)
                            .expect("the golden section holds fluid"),
                    )
                }
                other => panic!("unknown golden packet {other}"),
            };
            compare(name, section, &expected, &actual);
            compared += 1;
        }
    }
    assert_eq!(compared, 46, "every golden packet was compared");
}
