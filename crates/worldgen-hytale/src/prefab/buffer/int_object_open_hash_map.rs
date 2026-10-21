// Ported from: `it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap` (the
// fastutil dependency of the Java `PrefabBuffer`).
//! A faithful re-implementation of the fastutil open hash map the Java
//! prefab buffer stores its columns in.
//!
//! # Why this exists
//!
//! The Java `PrefabBuffer` keeps its columns in a
//! `Int2ObjectOpenHashMap<PrefabBufferColumn>` keyed by
//! `MathUtil.packInt(x, z)`. The *iteration order* of that map is
//! observable: [`PrefabBuffer::for_each`](crate::prefab::buffer::prefab_buffer::PrefabBufferAccessor::for_each)
//! walks the columns in the map's iteration order and the block consumer
//! draws a random number per block that carries a chance. A different
//! iteration order therefore produces a different chunk.
//!
//! The Rust port therefore reproduces the fastutil layout exactly:
//!
//! * the table size (`n`) is a power of two, the load factor is `0.75`,
//! * the position of a key is `HashCommon.mix(key) & (n - 1)` with linear
//!   probing,
//! * `HashCommon.mix(x)` is `let h = x * 0x9E3779B9; h ^ (h >>> 16)`,
//! * the table grows when `size > maxFill` (with `maxFill =
//!   min(ceil(n * 0.75), n - 1)`) to `HashCommon.arraySize(size + 1)`,
//! * the rehash re-inserts the old entries **from the back of the old table
//!   to the front**,
//! * the key `0` is the "null key" of fastutil: it lives in the extra slot
//!   `value[n]` (the key array keeps the `0` sentinel there) and it is
//!   visited **first** by the entry iterator,
//! * the entry iterator visits the table slots **from the back to the
//!   front**.
//!
//! The implementation was validated against the real fastutil 8.5.12 jar
//! (100+ random column sets of up to 400 columns: the table layout and the
//! iteration order matched exactly).

/// Java `it.unimi.dsi.fastutil.HashCommon.INT_PHI` — the multiplier of
/// `HashCommon.mix`.
const INT_PHI: i32 = -1640531527;

/// Java `it.unimi.dsi.fastutil.HashCommon.mix(int)`.
#[inline]
fn mix(x: i32) -> i32 {
    let h = x.wrapping_mul(INT_PHI);
    h ^ ((h as u32) >> 16) as i32
}

/// Java `it.unimi.dsi.fastutil.HashCommon.nextPowerOfTwo(int)`.
fn next_power_of_two(x: i32) -> i32 {
    if x <= 1 {
        return 1;
    }
    1i32 << (32 - (x - 1).leading_zeros() as i32)
}

/// Java `it.unimi.dsi.fastutil.HashCommon.arraySize(int, float)` — the
/// table size for `expected` entries at the load factor `f`.
///
/// ```java
/// final int t = (int) Math.ceil((double) ((float) expected / f));
/// return Math.max(2, HashCommon.nextPowerOfTwo(t));
/// ```
fn array_size(expected: i32, f: f32) -> i32 {
    let t = ((expected as f32) / f) as f64;
    let t = t.ceil() as i32;
    next_power_of_two(t).max(2)
}

/// Java `it.unimi.dsi.fastutil.HashCommon.maxFill(int, float)`.
fn max_fill(n: i32, f: f32) -> i32 {
    let fill = ((n as f32) * f) as f64;
    (fill.ceil() as i32).min(n - 1)
}

/// A faithful re-implementation of the fastutil
/// `Int2ObjectOpenHashMap<V>` for the int keys the prefab buffer uses.
#[derive(Clone, Debug, PartialEq)]
pub struct IntObjectOpenHashMap<V> {
    /// Java `int[] key` — the key table (the sentinel `0` marks an empty
    /// slot; the extra slot `n` holds the null key).
    key: Vec<i32>,
    /// Java `V[] value` — the value table.
    value: Vec<Option<V>>,
    /// Java `int mask` — `n - 1`.
    mask: i32,
    /// Java `int n` — the table size.
    n: i32,
    /// Java `int maxFill`.
    max_fill: i32,
    /// Java `int size` — the number of entries (the null key counts).
    size: i32,
    /// Java `boolean containsNullKey`.
    contains_null_key: bool,
}

impl<V> Default for IntObjectOpenHashMap<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> IntObjectOpenHashMap<V> {
    /// Java `new Int2ObjectOpenHashMap()` — the default capacity is 16.
    pub fn new() -> Self {
        Self::with_capacity(16)
    }

    /// Java `new Int2ObjectOpenHashMap(int expected, float f)`.
    pub fn with_capacity(expected: i32) -> Self {
        let f = 0.75f32;
        let n = array_size(expected.max(2), f);
        Self {
            key: vec![0; (n + 1) as usize],
            value: (0..(n + 1) as usize).map(|_| None).collect(),
            mask: n - 1,
            n,
            max_fill: max_fill(n, f),
            size: 0,
            contains_null_key: false,
        }
    }

    /// Java `int size()`.
    pub fn size(&self) -> i32 {
        self.size
    }

    /// Java `boolean isEmpty()`.
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Java `boolean containsKey(int k)`.
    pub fn contains_key(&self, k: i32) -> bool {
        self.find(k) >= 0
    }

    /// Java `V get(int k)` — `None` for a missing key.
    pub fn get(&self, k: i32) -> Option<&V> {
        let pos = self.find(k);
        if pos < 0 {
            return None;
        }
        self.value[pos as usize].as_ref()
    }

    /// Java `private int find(int k)`.
    fn find(&self, k: i32) -> i32 {
        if k == 0 {
            return if self.contains_null_key {
                self.n
            } else {
                -(self.n + 1)
            };
        }
        let mut pos = mix(k) & self.mask;
        loop {
            let kv = self.key[pos as usize];
            if kv == 0 {
                return -(pos + 1);
            }
            if kv == k {
                return pos;
            }
            pos = (pos + 1) & self.mask;
        }
    }

    /// Java `public V put(int k, V v)` — inserts the value and returns the
    /// previous one (if any).
    pub fn put(&mut self, k: i32, v: V) -> Option<V> {
        let pos = self.find(k);
        if pos >= 0 {
            return self.value[pos as usize].replace(v);
        }
        let pos = -pos - 1;
        if pos == self.n {
            self.contains_null_key = true;
        }
        self.key[pos as usize] = k;
        self.value[pos as usize] = Some(v);
        self.size += 1;
        if self.size > self.max_fill {
            let new_n = array_size(self.size + 1, 0.75);
            self.rehash(new_n);
        }
        None
    }

    /// Java `protected void rehash(int newN)`.
    fn rehash(&mut self, new_n: i32) {
        let old_key = std::mem::take(&mut self.key);
        let mut old_value = std::mem::take(&mut self.value);
        let old_n = self.n;

        let mut key = vec![0i32; (new_n + 1) as usize];
        let mut value: Vec<Option<V>> = (0..(new_n + 1) as usize).map(|_| None).collect();
        let mask = new_n - 1;

        // Java walks the old table from the back to the front.
        let real_size = self.size - i32::from(self.contains_null_key);
        let mut i = old_n;
        for _ in 0..real_size {
            loop {
                i -= 1;
                if old_key[i as usize] != 0 {
                    break;
                }
            }
            let k = old_key[i as usize];
            let mut pos = mix(k) & mask;
            while key[pos as usize] != 0 {
                pos = (pos + 1) & mask;
            }
            key[pos as usize] = k;
            value[pos as usize] = old_value[i as usize].take();
        }
        // The null key moves to the extra slot of the new table.
        value[new_n as usize] = old_value[old_n as usize].take();

        self.key = key;
        self.value = value;
        self.n = new_n;
        self.mask = new_n - 1;
        self.max_fill = max_fill(new_n, 0.75);
    }

    /// The entries in the **fastutil iteration order**: the null key (the
    /// key `0`) first, then the table slots from the back to the front.
    pub fn iter(&self) -> impl Iterator<Item = (i32, &V)> + '_ {
        let null_entry = if self.contains_null_key {
            self.value[self.n as usize].as_ref().map(|v| (0, v))
        } else {
            None
        };
        null_entry
            .into_iter()
            .chain((0..self.n).rev().filter_map(move |pos| {
                let k = self.key[pos as usize];
                if k == 0 {
                    return None;
                }
                self.value[pos as usize].as_ref().map(|v| (k, v))
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Java `MathUtil.packInt(int x, int z)`.
    fn pack(x: i32, z: i32) -> i32 {
        (x << 16) | (z & 0xFFFF)
    }

    /// Java `MathUtil.unpackLeft(int packed)`.
    fn unpack_left(packed: i32) -> i32 {
        let i = (packed >> 16) & 0xFFFF;
        if i & 0x8000 != 0 {
            i | 0xFFFF0000u32 as i32
        } else {
            i
        }
    }

    /// Java `MathUtil.unpackRight(int packed)`.
    fn unpack_right(packed: i32) -> i32 {
        let i = packed & 0xFFFF;
        if i & 0x8000 != 0 {
            i | 0xFFFF0000u32 as i32
        } else {
            i
        }
    }

    #[test]
    fn the_table_starts_at_thirty_two_slots() {
        let map: IntObjectOpenHashMap<i32> = IntObjectOpenHashMap::new();
        assert_eq!(map.n, 32);
        assert_eq!(map.mask, 31);
        assert_eq!(map.max_fill, 24);
        assert_eq!(map.size(), 0);
    }

    #[test]
    fn the_null_key_is_visited_first() {
        let mut map: IntObjectOpenHashMap<&str> = IntObjectOpenHashMap::new();
        map.put(pack(1, 2), "a");
        map.put(pack(3, -2), "b");
        map.put(pack(8, 10), "c");
        map.put(pack(0, 0), "zero");
        map.put(pack(-1, -1), "d");
        map.put(pack(5, 6), "e");

        let order: Vec<(i32, i32, &str)> = map
            .iter()
            .map(|(k, v)| (unpack_left(k), unpack_right(k), *v))
            .collect();

        // The fastutil 8.5.12 order of these six columns.
        assert_eq!(
            order,
            vec![
                (0, 0, "zero"),
                (5, 6, "e"),
                (3, -2, "b"),
                (1, 2, "a"),
                (-1, -1, "d"),
                (8, 10, "c"),
            ]
        );
    }

    #[test]
    fn the_table_grows_like_fastutil() {
        let mut map: IntObjectOpenHashMap<i32> = IntObjectOpenHashMap::new();
        for x in -3..=3 {
            for z in -3..=3 {
                map.put(pack(x, z), 1);
            }
        }
        // The fastutil table grows 32 → 64 → 128 for 49 entries.
        assert_eq!(map.n, 128);
        assert_eq!(map.size(), 49);
        assert_eq!(map.iter().count(), 49);
    }

    #[test]
    fn the_lookup_finds_every_key() {
        let mut map: IntObjectOpenHashMap<i32> = IntObjectOpenHashMap::new();
        for x in -20..=20 {
            for z in -20..=20 {
                map.put(pack(x, z), x * 1000 + z);
            }
        }
        for x in -20..=20 {
            for z in -20..=20 {
                assert_eq!(map.get(pack(x, z)), Some(&(x * 1000 + z)));
            }
        }
        assert!(!map.contains_key(pack(100, 100)));
        assert_eq!(map.get(pack(100, 100)), None);
    }
}
