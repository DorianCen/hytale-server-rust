// Ported from: `.hytale-source/.../worldgen/util/ArrayUtli.java`
//! Java `com.hypixel.hytale.server.worldgen.util.ArrayUtli` — Fisher–Yates
//! array shuffle.
//!
//! The Java class name contains a typo ("Utli"); it is kept as-is in the Rust
//! module name (`array_utli`) so the 1:1 class mapping stays intact.

use hytale_math::fast_random::FastRandom;

/// Java `ArrayUtli.shuffleArray(int[] ar, Random rnd)`.
///
/// The Java method takes a `java.util.Random`; world generation always passes
/// a `FastRandom` (which *is* a `java.util.Random`), so the Rust port takes a
/// [`FastRandom`].
pub fn shuffle_array_i32(ar: &mut [i32], rnd: &mut FastRandom) {
    shuffle_array(ar, rnd);
}

/// Java `ArrayUtli.shuffleArray(T[] ar, Random rnd)`.
///
/// Java's generic overload becomes the generic Rust function; the `int[]`
/// overload is kept as [`shuffle_array_i32`] for call sites that want the exact
/// Java signature.
pub fn shuffle_array<T>(ar: &mut [T], rnd: &mut FastRandom) {
    // Java: for (int i = ar.length - 1; i > 0; i--)
    let len = ar.len();
    if len < 2 {
        return;
    }
    for i in (1..len).rev() {
        // Java: int index = rnd.nextInt(i + 1);
        let index = rnd.next_int_bound((i + 1) as i32);
        // Java: simple swap of ar[index] and ar[i]
        ar.swap(index as usize, i);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn shuffle_is_deterministic_for_the_same_seed() {
        let mut a = [1, 2, 3, 4, 5, 6, 7, 8];
        let mut b = [1, 2, 3, 4, 5, 6, 7, 8];
        let mut ra = FastRandom::new(1234);
        let mut rb = FastRandom::new(1234);
        shuffle_array(&mut a, &mut ra);
        shuffle_array(&mut b, &mut rb);
        assert_eq!(a, b, "same seed ⇒ same permutation");
    }

    #[test]
    fn shuffle_preserves_all_elements() {
        let mut ar: Vec<i32> = (0..32).collect();
        let original: HashSet<i32> = ar.iter().copied().collect();
        let mut rnd = FastRandom::new(42);
        shuffle_array(&mut ar, &mut rnd);
        assert_eq!(ar.len(), 32);
        let after: HashSet<i32> = ar.iter().copied().collect();
        assert_eq!(original, after, "shuffle must be a permutation");
    }

    #[test]
    fn shuffle_of_short_arrays_is_a_no_op() {
        let mut empty: [i32; 0] = [];
        let mut rnd = FastRandom::new(7);
        shuffle_array(&mut empty, &mut rnd);
        assert!(empty.is_empty());

        let mut single = [42];
        shuffle_array(&mut single, &mut rnd);
        assert_eq!(single, [42]);
    }

    #[test]
    fn int_overload_shuffles_in_place() {
        let mut ar = [9, 8, 7, 6, 5];
        let mut rnd = FastRandom::new(99);
        shuffle_array_i32(&mut ar, &mut rnd);
        let mut sorted = ar;
        sorted.sort_unstable();
        assert_eq!(sorted, [5, 6, 7, 8, 9]);
    }
}
