//! # hytale-math
//!
//! Deterministic math primitives for the Hytale server port, ported 1:1 from
//! the Java `HytaleServer/Math` sources so the same seed produces identical
//! results as the original WorldGenerator (see `PORTING.md`).
//!
//! Ported classes (Java file → Rust module):
//! - `util/FastRandom.java` → [`fast_random`] — the seedable 48-bit LCG used
//!   by world generation (a bit-exact port of the `java.util.Random` core).
//! - `util/HashUtil.java` → [`hash_util`] — hash-based pseudo randomness.
//! - `util/MathUtil.java` → [`math_util`] — floors, clamps, lerps, packing.
//! - `util/ChunkUtil.java` → [`chunk_util`] — chunk/index coordinate packing.
//! - `util/NumberUtil.java` → [`number_util`] — wrapping `short` arithmetic.
//! - `util/TrigMathUtil.java` → [`trig_math_util`] — sin/cos/atan2 lookups.
//! - `random/RandomExtra.java` → **not ported** (see module note below).
//! - `Range.java` + `range/FloatRange.java` + `range/IntRange.java` →
//!   [`range`].
//!
//! ## About `random/RandomExtra.java`
//!
//! Not ported. It is a stateless helper over `java.util.concurrent.ThreadLocalRandom`
//! (it extends nothing — `final class` with a private constructor and only
//! static methods), i.e. every method draws from a *global, non-seedable*
//! source whose internal state cannot be reproduced bit-exactly from a Java
//! run to a Rust run. Grepping the `WorldGenerator` module shows **no
//! usages** of `RandomExtra` at all (all call sites live in `CoreServer`/`NPC`
//! gameplay systems such as flocking and entity actions), so none of its
//! algorithm portions are required for bit-exact chunk generation. If it is
//! ever needed, its deterministic-looking helpers (reservoir sampling,
//! weighted picking) should be re-expressed over an injected [`FastRandom`].
//!
//! ## Determinism rules
//!
//! - No external crates; `std` only.
//! - All Java integer arithmetic wraps (`wrapping_*`), all `(int)`-style
//!   casts saturate like Java, `>>>` becomes an unsigned shift.
//! - Nondeterministic Java helpers (`Math.random()`, `ThreadLocalRandom`)
//!   were intentionally left out.

#![deny(clippy::all)]

pub mod chunk_util;
pub mod fast_random;
pub mod hash_util;
pub mod math_util;
pub mod number_util;
pub mod range;
pub mod trig_math_util;

// Convenience re-exports mirroring the Java import paths that other crates
// will use most.
pub use range::{FloatRange, IntRange, Range};
pub use trig_math_util::{atan2, atan2_f64, cos, cos_f64, sin, sin_f64};
