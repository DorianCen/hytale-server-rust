// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/Common/src/main/java/com/hypixel/hytale/common/map/DefaultMap.java`
//! Java `DefaultMap<K, V>` — a map wrapper that returns a default value when
//! a key is absent.
//!
//! ## Notes on the Java → Rust mapping
//!
//! * Java `DefaultMap` implements `Map<K, V>`; the Rust port exposes the same
//!   semantic surface as inherent methods instead of implementing `HashMap`.
//! * Java's `replaceNullWithDefault` / null-key handling does not apply in
//!   Rust (no null keys), so the 4-arg Java constructor collapses to
//!   [`DefaultMap::new`] and [`DefaultMap::with_delegate`].
//! * For nullable Java defaults, users choose `V = Option<U>`.
//! * Java's `getOrDefault(key, dv)` delegates to the delegate map and
//!   **bypasses** the configured default. The Rust equivalent is
//!   [`DefaultMap::get_or`].

use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

/// Java `DefaultMap<K, V>`.
pub struct DefaultMap<K, V> {
    delegate: HashMap<K, V>,
    default_value: V,
    allow_replacing: bool,
}

impl<K: Eq + Hash, V> DefaultMap<K, V> {
    /// Java `DefaultMap(V defaultValue)` — `allowReplacing = true`.
    pub fn new(default_value: V) -> Self {
        Self {
            delegate: HashMap::new(),
            default_value,
            allow_replacing: true,
        }
    }

    /// Java `DefaultMap(V defaultValue, Map<K, V> delegate, boolean allowReplacing)`.
    pub fn with_delegate(default_value: V, delegate: HashMap<K, V>, allow_replacing: bool) -> Self {
        Self {
            delegate,
            default_value,
            allow_replacing,
        }
    }

    /// Java `DefaultMap.getDefaultValue()`.
    pub fn get_default_value(&self) -> &V {
        &self.default_value
    }

    /// Java `DefaultMap.setDefaultValue(V)`.
    pub fn set_default_value(&mut self, default_value: V) {
        self.default_value = default_value;
    }

    /// Java `DefaultMap.getDelegate()`.
    pub fn get_delegate(&self) -> &HashMap<K, V> {
        &self.delegate
    }

    /// Java `Map.get(Object)` — returns `&default_value` when the key is
    /// absent (Java returns the default instead of null).
    pub fn get(&self, key: &K) -> &V {
        self.delegate.get(key).unwrap_or(&self.default_value)
    }

    /// Java `Map.getOrDefault(Object, V)` — delegates to the delegate map
    /// and **bypasses** the configured default (subtle: callers usually want
    /// [`DefaultMap::get`], which always falls back to the configured default).
    pub fn get_or<'a>(&'a self, key: &K, fallback: &'a V) -> &'a V {
        self.delegate.get(key).unwrap_or(fallback)
    }

    /// Java `Map.put(K, V)`.
    ///
    /// When `!allow_replacing` and the key is already present this panics with
    /// the Java `IllegalArgumentException` message
    /// `"Attachment ({key:?}) is already registered!"`.
    pub fn put(&mut self, key: K, value: V)
    where
        K: Debug,
    {
        if self.allow_replacing {
            self.delegate.insert(key, value);
        } else {
            // Java: delegate.putIfAbsent returns non-null → throw.
            match self.delegate.entry(key) {
                std::collections::hash_map::Entry::Occupied(entry) => {
                    panic!("Attachment ({:?}) is already registered!", entry.key());
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(value);
                }
            }
        }
    }

    /// Java `Map.remove(Object)`.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.delegate.remove(key)
    }

    /// Java `Map.containsKey(Object)`.
    pub fn contains_key(&self, key: &K) -> bool {
        self.delegate.contains_key(key)
    }

    /// Java `Map.size()`.
    pub fn size(&self) -> usize {
        self.delegate.len()
    }

    /// Java `Map.isEmpty()`.
    pub fn is_empty(&self) -> bool {
        self.delegate.is_empty()
    }

    /// Java `Map.clear()`.
    pub fn clear(&mut self) {
        self.delegate.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_returns_default_when_absent() {
        let mut m = DefaultMap::<&str, i32>::new(42);
        assert_eq!(m.get(&"missing"), &42);
        m.put("present", 7);
        assert_eq!(m.get(&"present"), &7);
        assert_eq!(m.get(&"missing"), &42);
    }

    #[test]
    fn default_value_roundtrip() {
        let mut m = DefaultMap::<&str, i32>::new(0);
        assert_eq!(m.get_default_value(), &0);
        m.set_default_value(99);
        assert_eq!(m.get_default_value(), &99);
        assert_eq!(m.get(&"absent"), &99);
    }

    #[test]
    fn with_delegate_uses_provided_map() {
        let mut delegate = HashMap::new();
        delegate.insert("a", 1);
        let m = DefaultMap::<&str, i32>::with_delegate(0, delegate, true);
        assert_eq!(m.get(&"a"), &1);
        assert_eq!(m.get(&"b"), &0);
        assert!(m.get_delegate().contains_key(&"a"));
    }

    #[test]
    fn put_replacing_allowed_by_default() {
        let mut m = DefaultMap::<&str, i32>::new(0);
        m.put("k", 1);
        m.put("k", 2);
        assert_eq!(m.get(&"k"), &2);
    }

    #[test]
    #[should_panic(expected = "Attachment (\"dup\") is already registered!")]
    fn put_non_replacing_duplicate_panics() {
        let mut m = DefaultMap::<&str, i32>::with_delegate(0, HashMap::new(), false);
        m.put("dup", 1);
        m.put("dup", 2);
    }

    #[test]
    fn put_non_replacing_distinct_keys_ok() {
        let mut m = DefaultMap::<&str, i32>::with_delegate(0, HashMap::new(), false);
        m.put("a", 1);
        m.put("b", 2);
        assert_eq!(m.size(), 2);
        assert_eq!(m.get(&"a"), &1);
        assert_eq!(m.get(&"b"), &2);
    }

    #[test]
    fn get_or_bypasses_configured_default() {
        // Java getOrDefault(key, dv) delegates to the delegate map and
        // BYPASSES the configured default.
        let mut m = DefaultMap::<&str, i32>::new(100);
        m.put("x", 5);
        assert_eq!(m.get(&"x"), &5);
        assert_eq!(m.get_or(&"x", &99), &5);
        assert_eq!(m.get_or(&"missing", &99), &99);
        // Configured default is still 100 for get():
        assert_eq!(m.get(&"missing"), &100);
    }

    #[test]
    fn remove_contains_size_clear() {
        let mut m = DefaultMap::<&str, i32>::new(0);
        assert!(m.is_empty());
        m.put("a", 1);
        m.put("b", 2);
        assert!(!m.is_empty());
        assert_eq!(m.size(), 2);
        assert!(m.contains_key(&"a"));
        assert!(!m.contains_key(&"z"));
        assert_eq!(m.remove(&"a"), Some(1));
        assert_eq!(m.remove(&"a"), None);
        m.clear();
        assert!(m.is_empty());
        assert_eq!(m.size(), 0);
    }

    #[test]
    fn nullable_via_option() {
        // For nullable Java defaults, users choose V = Option<U>.
        let m = DefaultMap::<&str, Option<i32>>::new(None);
        assert_eq!(m.get(&"absent"), &None);
        assert_eq!(m.get(&"absent"), &None::<i32>);
    }
}
