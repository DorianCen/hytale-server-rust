//! Store load-order graph.
//!
//! Mirrors the `loadsBefore` / `loadsAfter` edge handling from
//! `AssetStore/src/main/java/com/hypixel/hytale/assetstore/AssetStore.java`.
//! Ties are broken by the store's position in the input list so the order is
//! stable and reproducible.

use std::collections::{BTreeMap, BTreeSet};

use crate::pack::AssetError;

type Result<T> = std::result::Result<T, AssetError>;

/// One store and its ordering edges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreNode {
    /// Store/class name.
    pub name: String,
    /// Stores that must load after this one.
    pub loads_before: Vec<String>,
    /// Stores that must load before this one.
    pub loads_after: Vec<String>,
}

impl StoreNode {
    /// Construct a node.
    pub fn new(
        name: impl Into<String>,
        loads_before: impl IntoIterator<Item = impl Into<String>>,
        loads_after: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            name: name.into(),
            loads_before: loads_before.into_iter().map(Into::into).collect(),
            loads_after: loads_after.into_iter().map(Into::into).collect(),
        }
    }
}

/// Topologically order stores.
///
/// Edges that point at names outside `stores` are ignored: the Java loader also
/// references modules and asset classes that are not asset stores.
pub fn order_stores(stores: &[StoreNode]) -> Result<Vec<String>> {
    let position: BTreeMap<&str, usize> = stores
        .iter()
        .enumerate()
        .map(|(index, store)| (store.name.as_str(), index))
        .collect();

    // adjacency[a] = set of nodes that must come after a
    let mut adjacency: BTreeMap<&str, BTreeSet<&str>> = stores
        .iter()
        .map(|store| (store.name.as_str(), BTreeSet::new()))
        .collect();
    let mut indegree: BTreeMap<&str, usize> = stores
        .iter()
        .map(|store| (store.name.as_str(), 0))
        .collect();

    let mut add_edge = |before: &str, after: &str| {
        let (Some(before_index), Some(after_index)) =
            (position.get(before).copied(), position.get(after).copied())
        else {
            return;
        };
        let before_name = stores[before_index].name.as_str();
        let after_name = stores[after_index].name.as_str();
        if before_index != after_index
            && adjacency
                .get_mut(before_name)
                .is_some_and(|edges| edges.insert(after_name))
        {
            *indegree.get_mut(after_name).expect("node exists") += 1;
        }
    };

    for store in stores {
        for after in &store.loads_before {
            add_edge(&store.name, after);
        }
        for before in &store.loads_after {
            add_edge(before, &store.name);
        }
    }

    // Ready set ordered by input position for a stable tie-break.
    let mut ready: Vec<&str> = indegree
        .iter()
        .filter_map(|(name, degree)| (*degree == 0).then_some(*name))
        .collect();
    ready.sort_by_key(|name| position[name]);

    let mut ordered = Vec::with_capacity(stores.len());
    while let Some(name) = ready.first().copied() {
        ready.remove(0);
        ordered.push(name.to_string());
        let mut newly_ready = Vec::new();
        if let Some(edges) = adjacency.get(name) {
            for next in edges {
                let degree = indegree.get_mut(next).expect("node exists");
                *degree -= 1;
                if *degree == 0 {
                    newly_ready.push(*next);
                }
            }
        }
        ready.extend(newly_ready);
        ready.sort_by_key(|candidate| position[candidate]);
    }

    if ordered.len() != stores.len() {
        let cycle: Vec<&str> = indegree
            .iter()
            .filter_map(|(name, degree)| (*degree > 0).then_some(*name))
            .collect();
        return Err(AssetError::StoreCycle {
            cycle: cycle.join(" -> "),
        });
    }
    Ok(ordered)
}
