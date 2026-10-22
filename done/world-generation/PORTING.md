# Porting conventions (read this before writing any Rust)

Goal: **bit-exact parity** with the Java WorldGenerator. Same seed → identical chunks.

## Java → Rust semantic mapping (critical)
- Java `double` → Rust `f64`, Java `float` → Rust `f32`. **Never** change precision.
- Java `int` → `i32` (wrapping!), `long` → `i64` (wrapping). Java integer arithmetic
  overflows silently with two's complement wrapping; in Rust use `wrapping_add` /
  `wrapping_mul` / `<<` etc. so debug builds don't panic. Java `>>>` is
  `logical_shr` via `((x as u32) >> n) as i32`.
- Java `Math.*` functions: use the exact same operation (`f64::sin` etc.).
  If Java uses a custom approximation (e.g. `MathUtil.fastSin`), port the approximation,
  not the std function.
- Java `Random`/`FastRandom`: port the exact LCG bit math, including any `(int)` casts
  of intermediate values. Do NOT substitute `rand` crate.
- Integer division and `%` in Java truncate toward zero — same as Rust `/` for ints.
  Java `%` on negatives: `-7 % 3 == -1` — same as Rust. Good.
- Java `Math.floorDiv` / `Math.floorMod` → `i32::div_euclid`-style: use
  `x.rem_euclid(y)` only when Java uses floorMod. Be careful: floorMod ≠ %.

## Style
- Multi-crate workspace. Crate boundaries:
  - `hytale-math`: FastRandom, HashUtil, MathUtil, ChunkUtil, TrigMathUtil, NumberUtil, ranges.
  - `procedural`: noise generators, NoiseProperty DSL, point generators, conditions,
    CoordinateRandomizer (Java package `com.hypixel.hytale.procedurallib.**`).
  - `worldgen-model`, `worldgen-hytale`, `worldgen-cli`: later phases.
- Java class `FooBar` → Rust module `foo_bar.rs`, struct `FooBar`. Keep 1:1 class mapping
  so the architecture doc can reference it. Include a `// Ported from:` doc comment with
  the Java file path.
- Public API must be deterministic and allocation-light in hot paths. Prefer
  `&mut` scratch buffers (the Java code uses ThreadLocal pools; we pass `&mut GenContext`
  later — for now take `&mut Vec`/`&mut ResultBuffer` params mirroring the Java signatures).
- No `unsafe`. No `rand` crate in generation code.
- Tests: unit tests ported from the Java `*Test.java` files where they exist
  (`Math/src/**`, `ProceduralLibrary/src/test/**`), plus `#[cfg(test)]` sanity checks.
- Run `cargo build -p <crate>` and `cargo test -p <crate>` before finishing. Code must
  compile with zero warnings (`cargo clippy -p <crate> -- -D warnings`).

## Golden data
- `testdata/golden/` holds JSON dumps produced by the Java side (`tools/java-dumper/`).
- Never hand-craft expected values — either transcribe a Java unit test assertion or
  wait for the golden dumps.

## Java source roots (do not copy files, port them)
- `HytaleServer/Math/src/main/java/com/hypixel/hytale/math/**`
- `HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/**`
- `HytaleServer/WorldGenerator/src/main/java/com/hypixel/hytale/server/worldgen/**`
  (all under `.hytale-source/hytale-shared-source/`)
