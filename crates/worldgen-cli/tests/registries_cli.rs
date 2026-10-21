//! The `registries` command: inspect a snapshot and compare two snapshots by
//! name. Id swaps are tolerated (the vanilla pipeline schedules them); a
//! different block set or metadata is a structural failure.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn temp_snapshot(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "worldgen-registries-{tag}-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    path
}

fn run(path: &Path, compare: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_worldgen-cli"))
        .arg("registries")
        .arg("--path")
        .arg(path)
        .arg("--compare")
        .arg(compare)
        .output()
        .expect("run worldgen-cli registries")
}

#[test]
fn the_canonical_dump_is_structurally_identical_to_itself() {
    let dump = repo_root().join("testdata/golden/registries.json");
    let output = run(&dump, &dump);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("6135 blocks"), "{stdout}");
    assert!(stdout.contains("structurally identical"), "{stdout}");
}

#[test]
fn pure_id_swaps_are_reported_but_not_structural() {
    let dump = repo_root().join("testdata/golden/registries.json");
    let mut json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&dump).unwrap()).unwrap();
    // Shift every block id by one: the name set and all metadata stay the
    // same, only the numeric assignment changes (the observed scheduling
    // effect, scaled up so the assertion is unambiguous).
    for asset in json["blocks"]["assets"].as_array_mut().unwrap() {
        let index = asset["index"].as_i64().unwrap();
        asset["index"] = serde_json::json!(index + 1);
    }

    let swapped = temp_snapshot("swaps");
    std::fs::write(&swapped, serde_json::to_string(&json).unwrap()).unwrap();

    let output = run(&swapped, &dump);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("6135 id swaps"), "{stdout}");
    assert!(stdout.contains("structurally identical"), "{stdout}");

    let _ = std::fs::remove_file(&swapped);
}

#[test]
fn a_missing_block_is_a_structural_difference() {
    let dump = repo_root().join("testdata/golden/registries.json");
    let mut json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&dump).unwrap()).unwrap();
    json["blocks"]["assets"].as_array_mut().unwrap().pop();

    let reduced = temp_snapshot("missing");
    std::fs::write(&reduced, serde_json::to_string(&json).unwrap()).unwrap();

    let output = run(&reduced, &dump);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!output.status.success(), "{stdout}");
    assert!(
        stdout.contains("only in"),
        "the missing block must be reported: {stdout}"
    );

    let _ = std::fs::remove_file(&reduced);
}
