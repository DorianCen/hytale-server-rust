// Ported from: `.hytale-source/.../procedurallib/json/SeedString.java`
//! Java `procedurallib.json.SeedString`.
//!
//! The seed-string tree: every JSON loader appends a suffix to the seed
//! string it was given, and the (Java) `String.hashCode` of the resulting
//! string — multiplied by `114512143` — becomes the integer seed offset the
//! noise layers derive their seeds from. The convention is verified by the
//! golden dumps (`tools/java-dumper` Phase-2 dumper reconstructs the same
//! hashes; see `crates/worldgen-hytale/tests/golden.rs`).
//!
//! The Java generic `SeedString<T extends SeedResource>` parameter is dropped:
//! the `SeedResource` payload only carries thread-local result buffers and the
//! optional seed-report logging hook, neither of which participates in the
//! seed math. Seed reporting is not reproduced (Java default
//! `shouldReportSeeds() == false`); enable the `log` target
//! `WorldGenerator` manually if a report is ever needed.

/// Java `String.hashCode()` — `h = 31 * h + c` over the **UTF-16 code units**,
/// with wrapping `i32` arithmetic. (ASCII strings match the byte-wise sum, but
/// the UTF-16 iteration is the faithful one.)
pub fn java_string_hash_code(s: &str) -> i32 {
    let mut hash: i32 = 0;
    for unit in s.encode_utf16() {
        hash = hash.wrapping_mul(31).wrapping_add(unit as i16 as i32);
    }
    hash
}

/// Java `procedurallib.json.SeedString`.
#[derive(Clone, Debug)]
pub struct SeedString {
    /// Java `protected final String original`.
    original: String,
    /// Java `protected final String seed`.
    seed: String,
    /// Java `protected final int hash` — `seed.hashCode() * 114512143`.
    hash: i32,
}

impl SeedString {
    /// Java `SeedString(String original, T t)` — the seed string equals the
    /// original.
    pub fn new(original: &str) -> Self {
        Self::with_seed(original, original)
    }

    /// Java `SeedString(String original, String seed, T t)`.
    pub fn with_seed(original: &str, seed: &str) -> Self {
        let hash = java_string_hash_code(seed).wrapping_mul(114_512_143);
        Self {
            original: original.to_string(),
            seed: seed.to_string(),
            hash,
        }
    }

    /// Java `SeedString<T> append(String suffix)` — appends to the **seed**
    /// string only (the original is kept).
    pub fn append(&self, suffix: &str) -> SeedString {
        SeedString::with_seed(&self.original, &format!("{}{}", self.seed, suffix))
    }

    /// Java `SeedString<T> appendToOriginal(String suffix)` — the seed
    /// becomes `original + suffix`; the original is left unchanged.
    pub fn append_to_original(&self, suffix: &str) -> SeedString {
        SeedString::with_seed(&self.original, &format!("{}{}", self.original, suffix))
    }

    /// Java `SeedString<T> alternateOriginal(String suffix)` — the original is
    /// replaced by `original + suffix` while the seed becomes
    /// `altOriginal + seed.substring(original.length())`, i.e. the suffix is
    /// inserted between the original and everything appended to the seed so far.
    pub fn alternate_original(&self, suffix: &str) -> SeedString {
        let alt_original = format!("{}{}", self.original, suffix);
        let alt_seed = format!("{}{}", alt_original, &self.seed[self.original.len()..]);
        SeedString::with_seed(&alt_original, &alt_seed)
    }

    /// Java `int hashCode()` — the precomputed `seed.hashCode() * 114512143`.
    #[inline]
    pub fn hash_code(&self) -> i32 {
        self.hash
    }

    /// Java `String original` (protected) — exposed for the loaders that read
    /// it back (e.g. `NoisePropertyJsonLoader` seed reports / `KEY_SEED`).
    pub fn original(&self) -> &str {
        &self.original
    }

    /// Java `String toString()` — the full seed string.
    pub fn seed(&self) -> &str {
        &self.seed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_string_hash_matches_reference_values() {
        // Reference values from the JVM: "abc".hashCode() == 96354,
        // "".hashCode() == 0, "hello world".hashCode() == 1794106052.
        assert_eq!(java_string_hash_code(""), 0);
        assert_eq!(java_string_hash_code("abc"), 96_354);
        assert_eq!(java_string_hash_code("hello world"), 1_794_106_052);
        // Wrapping case: "1234567890" overflows i32 in Java (no panic in Rust
        // release or debug thanks to wrapping_mul/add).
        let _ = java_string_hash_code("1234567890");
    }

    #[test]
    fn hash_is_seed_times_multiplier() {
        let s = SeedString::new("0");
        assert_eq!(
            s.hash_code(),
            java_string_hash_code("0").wrapping_mul(114_512_143)
        );
    }

    #[test]
    fn append_keeps_original() {
        let s = SeedString::new("42");
        let t = s.append(".Zones");
        assert_eq!(t.original(), "42");
        assert_eq!(t.seed(), "42.Zones");
        // Appending twice chains on the seed.
        let u = t.append(".Zones");
        assert_eq!(u.seed(), "42.Zones.Zones");
    }

    #[test]
    fn append_to_original_keeps_the_original_and_rebuilds_the_seed() {
        let s = SeedString::new("42").append(".NoiseProperty");
        let t = s.append_to_original("MySeed");
        // Java: `new SeedString<>(original, original + suffix, t)`.
        assert_eq!(t.original(), "42");
        assert_eq!(t.seed(), "42MySeed");
    }

    #[test]
    fn alternate_original_inserts_suffix_between_original_and_appends() {
        let s = SeedString::new("42").append(".X-Noise#0");
        let t = s.alternate_original(".X-Noise#0");
        // original: "42.X-Noise#0"; seed: altOriginal + seed[2..] =
        // "42.X-Noise#0" + ".X-Noise#0"
        assert_eq!(t.original(), "42.X-Noise#0");
        assert_eq!(t.seed(), "42.X-Noise#0.X-Noise#0");
    }
}
