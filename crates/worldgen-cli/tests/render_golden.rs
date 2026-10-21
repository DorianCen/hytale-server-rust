//! Visual-regression artefacts: the fixed `render-map` views must stay
//! pixel-identical to the committed goldens.
//!
//! The images are **not** Java golden data; they are produced by the Rust
//! renderer itself and protect it against accidental changes (colour ramp,
//! mask/zone/biome sampling, orientation). Numeric parity is covered by the
//! golden JSON suites.
//!
//! Regenerate them with:
//!
//! ```sh
//! for mode in height zone biome; do
//!   cargo run -p worldgen-cli -- render-map \
//!     --world testdata/Default --seed 0 --center 0,0 --size 64 --scale 16 \
//!     --mode "$mode" --out "testdata/golden/render/$mode.png"
//! done
//! ```

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The image a single `render-map` invocation produces.
struct Image {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets_available() -> bool {
    repo_root().join("Assets/extracted/Server/Prefabs").is_dir()
}

fn decode(path: &Path) -> Image {
    let decoder = png::Decoder::new(File::open(path).expect("open the PNG"));
    let mut reader = decoder.read_info().expect("read the PNG header");
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).expect("decode the PNG");
    buffer.truncate(info.buffer_size());
    Image {
        width: info.width,
        height: info.height,
        pixels: buffer,
    }
}

#[test]
fn the_fixed_render_views_match_the_committed_goldens() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let root = repo_root();
    let world = root.join("testdata/Default");
    let temp = std::env::temp_dir().join(format!(
        "worldgen-render-golden-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&temp).expect("create the temporary directory");

    for mode in ["height", "zone", "biome"] {
        let produced_path = temp.join(format!("{mode}.png"));
        let status = Command::new(env!("CARGO_BIN_EXE_worldgen-cli"))
            .arg("render-map")
            .arg("--world")
            .arg(&world)
            .args([
                "--seed", "0", "--center", "0,0", "--size", "64", "--scale", "16",
            ])
            .args(["--mode", mode, "--out"])
            .arg(&produced_path)
            .status()
            .expect("run worldgen-cli render-map");
        assert!(status.success(), "render-map failed for mode {mode}");

        let produced = decode(&produced_path);
        let golden = decode(&root.join(format!("testdata/golden/render/{mode}.png")));
        assert_eq!(
            (produced.width, produced.height),
            (golden.width, golden.height),
            "the {mode} image size changed"
        );
        assert_eq!(
            produced.pixels, golden.pixels,
            "the {mode} image differs from testdata/golden/render/{mode}.png \
             (regenerate the golden if the change is intended)"
        );
    }

    let _ = std::fs::remove_dir_all(&temp);
}
