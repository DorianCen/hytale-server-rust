//! The Phase-6 integration boundary: the worker-pool [`WorldGenService`] must
//! produce exactly the same chunk as the direct single-threaded path, apply
//! the cancellation token and shut down gracefully.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use worldgen_hytale::chunk::chunk_generator::ChunkGenerator;
use worldgen_hytale::chunk::chunk_generator_resource::ChunkGeneratorResource;
use worldgen_hytale::loader::chunk_generator_json_loader::ChunkGeneratorJsonLoader;
use worldgen_hytale::world_gen::{
    generate_chunk, WorldGen, WorldGenError, WorldGenOptions, WorldGenRequest, WorldGenService,
};
use worldgen_hytale::world_gen_config::WorldGenConfig;
use worldgen_model::chunk::GeneratedChunk;

fn assets_available() -> bool {
    PathBuf::from(format!(
        "{}/../../Assets/extracted/Server/Prefabs",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    ))
    .is_dir()
}

fn default_world() -> Rc<ChunkGenerator> {
    let root = PathBuf::from(format!(
        "{}/../../testdata/Default",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    ));
    let config = WorldGenConfig::new(root, "Default".to_string(), "0.0.0".to_string());
    // The provider label (Java `HytaleWorldGenProvider`), so the service and
    // the direct path load an identical world graph.
    ChunkGeneratorJsonLoader::new(procedural::json::SeedString::new("ChunkGenerator"), config)
        .load()
}

/// A deterministic fingerprint of the generated chunk.
fn fingerprint(chunk: &GeneratedChunk) -> (u64, usize) {
    let mut hash = 0xcbf29ce484222325u64;
    let mut solid = 0usize;
    for y in 0..worldgen_model::block_priority::HEIGHT {
        for x in 0..32 {
            for z in 0..32 {
                let block = chunk.get_block(x, y, z);
                if block != 0 {
                    solid += 1;
                }
                hash ^= block as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
    }
    (hash, solid)
}

#[test]
fn the_worker_pool_matches_the_direct_generation() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let generator = default_world();
    // The loader keeps the registry snapshot on the generator; the direct
    // path and the workers must reuse the same handle (no second dump load).
    let registry = Rc::clone(
        generator
            .get_registries()
            .expect("the loader stores its registry snapshot"),
    );

    let root = PathBuf::from(format!(
        "{}/../../testdata/Default",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    ));
    let config = WorldGenConfig::new(root, "Default".to_string(), "0.0.0".to_string());
    let service = WorldGenService::new(
        config,
        WorldGenOptions {
            workers: 2,
            ..Default::default()
        },
    )
    .expect("the service loads the Default world");

    let mut resource = ChunkGeneratorResource::new();
    for (chunk_x, chunk_z) in [(0, 0), (7, 3), (-12, -25)] {
        let request = WorldGenRequest::new(0, chunk_x, chunk_z);
        let direct = generate_chunk(&generator, Rc::clone(&registry), &mut resource, &request)
            .expect("the direct generation");
        let pooled = service
            .generate(WorldGenRequest::new(0, chunk_x, chunk_z))
            .expect("the pooled generation");
        assert_eq!(
            fingerprint(&direct.chunk),
            fingerprint(&pooled.chunk),
            "the worker pool must produce the same chunk as the direct path ({chunk_x}, {chunk_z})"
        );
        assert!(
            pooled.timings.total_nanos >= pooled.timings.blocks_nanos,
            "the stage timings must be nested in the total time"
        );
    }

    let stats = service.stats();
    assert_eq!(stats.chunks, 3);
    service.shutdown();
    assert!(matches!(
        service.generate(WorldGenRequest::new(0, 0, 0)),
        Err(WorldGenError::Shutdown)
    ));
}

#[test]
fn a_cancelled_request_never_generates() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let root = PathBuf::from(format!(
        "{}/../../testdata/Default",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    ));
    let config = WorldGenConfig::new(root, "Default".to_string(), "0.0.0".to_string());
    let service = WorldGenService::new(
        config,
        WorldGenOptions {
            workers: 1,
            ..Default::default()
        },
    )
    .expect("the service loads the Default world");

    let cancel = Arc::new(AtomicBool::new(true));
    let result = service.generate(WorldGenRequest::new(0, 0, 0).with_cancel(cancel));
    assert!(matches!(result, Err(WorldGenError::Cancelled)));
    assert_eq!(service.stats().chunks, 0);
    service.shutdown();
}

#[test]
fn a_registry_snapshot_path_is_loaded_by_the_workers() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    // A deliberately invalid snapshot must fail the worker load: this proves
    // the configured path (not the repository default) is the one read, i.e.
    // a server can hand the service its own registry dump.
    let temp = std::env::temp_dir().join(format!(
        "worldgen-registry-snapshot-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&temp).expect("create the temporary directory");
    let snapshot = temp.join("registries.json");
    std::fs::write(&snapshot, "{}").expect("write the broken snapshot");

    let root = PathBuf::from(format!(
        "{}/../../testdata/Default",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    ));
    let config = WorldGenConfig::new(root, "Default".to_string(), "0.0.0".to_string());
    let result = WorldGenService::new(
        config,
        WorldGenOptions {
            workers: 1,
            registries_path: Some(snapshot),
            ..Default::default()
        },
    );
    assert!(matches!(result, Err(WorldGenError::Load(_))));

    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn a_bad_world_path_is_a_load_error() {
    let config = WorldGenConfig::new(
        PathBuf::from("/definitely/not/a/world"),
        "Default".to_string(),
        "0.0.0".to_string(),
    );
    let result = WorldGenService::new(
        config,
        WorldGenOptions {
            workers: 1,
            ..Default::default()
        },
    );
    assert!(matches!(result, Err(WorldGenError::Load(_))));
}

#[test]
fn the_cancellation_token_is_observed() {
    let cancel = Arc::new(AtomicBool::new(false));
    let request = WorldGenRequest::new(1, 2, 3).with_cancel(Arc::clone(&cancel));
    cancel.store(true, Ordering::Relaxed);
    assert!(request.cancel.as_ref().unwrap().load(Ordering::Relaxed));
}
