//! The server adapter must match the direct generation path, honour the
//! `stillNeeded` predicate and drain in-flight work on shutdown.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::Arc;

use worldgen_hytale::chunk::chunk_generator_resource::ChunkGeneratorResource;
use worldgen_hytale::loader::chunk_generator_json_loader::ChunkGeneratorJsonLoader;
use worldgen_hytale::world_gen::{
    generate_chunk, WorldGenError, WorldGenOptions, WorldGenRequest, WorldGenService,
};
use worldgen_hytale::world_gen_config::WorldGenConfig;
use worldgen_model::chunk::GeneratedChunk;
use worldgen_model::registry::Registries;
use worldgen_server::{ChunkProvider, ChunkProviderError, ChunkRequest, WorldGenChunkProvider};

fn manifest_dir() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
}

fn assets_available() -> bool {
    manifest_dir()
        .join("../../Assets/extracted/Server/Prefabs")
        .is_dir()
}

fn default_config() -> WorldGenConfig {
    WorldGenConfig::new(
        manifest_dir().join("../../testdata/Default"),
        "Default".to_string(),
        "0.0.0".to_string(),
    )
}

fn provider(workers: usize) -> WorldGenChunkProvider {
    WorldGenChunkProvider::new(
        default_config(),
        WorldGenOptions {
            workers,
            ..Default::default()
        },
    )
    .expect("the Default world loads")
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
fn the_provider_matches_the_direct_generation() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let generator = ChunkGeneratorJsonLoader::new(
        procedural::json::SeedString::new("ChunkGenerator"),
        default_config(),
    )
    .load();
    let registry = Rc::new(Registries::load());
    let mut resource = ChunkGeneratorResource::new();
    let provider = provider(2);

    for (x, z) in [(0, 0), (7, 3)] {
        let direct = generate_chunk(
            &generator,
            Rc::clone(&registry),
            &mut resource,
            &WorldGenRequest::new(0, x, z),
        )
        .expect("the direct generation");
        let provided = provider
            .generate(ChunkRequest::new(0, x, z).with_server_priority(1.0))
            .expect("the provider generation");
        assert_eq!(
            fingerprint(&direct.chunk),
            fingerprint(&provided.chunk),
            "the adapter must not change the generated chunk ({x}, {z})"
        );
    }

    assert_eq!(provider.stats().chunks, 2);
    provider.shutdown();
}

#[test]
fn a_chunk_outside_the_generatable_area_is_rejected() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let provider = provider(1);
    let (min, max) = provider.generatable_chunk_bounds();
    for (x, z) in [(min - 1, 0), (0, max + 1)] {
        assert!(matches!(
            provider.generate(ChunkRequest::new(0, x, z)),
            Err(ChunkProviderError::OutsideGeneratableArea)
        ));
    }
    assert_eq!(provider.stats().chunks, 0);
    provider.shutdown();
}

#[test]
fn an_unwanted_chunk_is_not_generated() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let provider = provider(1);
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_in_predicate = Arc::clone(&calls);
    let request = ChunkRequest::new(0, 0, 0).with_still_needed(move |_index| {
        calls_in_predicate.fetch_add(1, Ordering::Relaxed);
        false
    });

    assert!(matches!(
        provider.generate(request),
        Err(ChunkProviderError::NotNeeded)
    ));
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(provider.stats().chunks, 0);
    provider.shutdown();
}

#[test]
fn a_cancelled_token_is_reported() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let provider = provider(1);
    let cancel = Arc::new(AtomicBool::new(true));
    let result = provider.generate(ChunkRequest::new(0, 0, 0).with_cancel(cancel));
    assert!(matches!(
        result,
        Err(ChunkProviderError::Generation(WorldGenError::Cancelled))
    ));
    provider.shutdown();
}

#[test]
fn shutdown_drains_the_queue_and_rejects_later_requests() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let provider = Arc::new(provider(2));

    // Submit a request, wait until a worker has popped it (so it is provably
    // in flight), then shut down: the graceful shutdown must let it finish.
    let worker = {
        let provider = Arc::clone(&provider);
        std::thread::spawn(move || provider.generate(ChunkRequest::new(0, 0, 0)))
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while provider.stats().generating == 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "the request was never picked up by a worker"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    provider.shutdown();
    let result = worker.join().expect("the request thread");
    assert!(
        result.is_ok(),
        "the drained request must complete: {result:?}"
    );
    assert_eq!(provider.stats().chunks, 1);

    assert!(matches!(
        provider.generate(ChunkRequest::new(0, 1, 1)),
        Err(ChunkProviderError::Shutdown)
    ));
    // Shutdown is idempotent.
    provider.shutdown();
}

#[test]
fn the_provider_is_object_safe() {
    fn takes_provider(_provider: &dyn ChunkProvider) {}
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let provider = provider(1);
    takes_provider(&provider);
    provider.shutdown();
}

#[test]
fn an_existing_service_can_be_wrapped() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let service = WorldGenService::new(
        default_config(),
        WorldGenOptions {
            workers: 1,
            ..Default::default()
        },
    )
    .expect("the Default world loads");
    let provider = WorldGenChunkProvider::from_service(service);
    assert!(provider.generate(ChunkRequest::new(0, 0, 0)).is_ok());
    provider.shutdown();
}
