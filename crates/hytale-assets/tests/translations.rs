//! Translation catalog parity against the base-game pack.

use std::path::PathBuf;

use hytale_assets::{load_translations, AssetPack};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets_available() -> bool {
    if matches!(
        std::env::var("HYTALE_TEST_ASSETS").as_deref(),
        Ok("0" | "false" | "off")
    ) {
        return false;
    }
    repo_root().join("Assets/Assets.zip").is_file()
}

#[test]
fn translations_carry_server_catalog_and_language_fallback() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open the pack");
    let en_us = load_translations(&pack, "en-US").expect("load en-US");
    assert!(
        en_us.len() > 10_000,
        "expected the full server catalog, got {} entries",
        en_us.len()
    );
    assert_eq!(
        en_us
            .get("server.items.Container_Bucket.name")
            .map(String::as_str),
        Some("Wooden Bucket")
    );
    assert!(en_us.contains_key("server.items.Alchemy_Cauldron.name"));

    // en-GB has no directory, so `fallback.lang` resolves it to en-US.
    let en_gb = load_translations(&pack, "en-GB").expect("load en-GB");
    assert_eq!(en_gb, en_us);

    // Unknown languages fall back to en-US rather than an empty map.
    let unknown = load_translations(&pack, "xx-XX").expect("load unknown");
    assert_eq!(unknown, en_us);
}
