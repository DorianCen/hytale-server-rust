use hytale_assets::{AssetPack, AssetRegistry};
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let pack = AssetPack::open(root.join("Assets/Assets.zip")).expect("pack");
    let registry = AssetRegistry::load_phase2(&pack).expect("registry");
    for (name, store) in [
        ("AudioCategory", &registry.audio_categories),
        ("AudioState", &registry.audio_states),
    ] {
        println!("=== {name} count={}", store.iter().count());
        for record in store.iter().take(5) {
            println!(
                "id={:?} key={} value={}",
                record.id, record.key, record.value
            );
        }
    }
}
