//! Crafting recipe catalog.
//!
//! Java stores both item recipes (`Item.Recipe`) and standalone
//! `Server/Item/Recipes/**/*.json` documents. This module loads both into the
//! protocol `CraftingRecipe` shape used by `UpdateRecipes` and
//! `UpdateKnownRecipes`, and keeps the bench requirement for window checks.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use hytale_protocol::generated::common::{
    BenchRequirement, BenchType, CraftingRecipe, MaterialQuantity,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Item definitions below an extracted asset root.
pub const ITEMS_ROOT: &str = "Server/Item/Items";
/// Standalone recipe documents below an extracted asset root.
pub const RECIPES_ROOT: &str = "Server/Item/Recipes";

/// Errors from the recipe catalog.
#[derive(Debug, Error)]
pub enum CraftingError {
    /// A directory/file could not be read.
    #[error("recipe catalog I/O at {path}: {source}")]
    Io {
        /// Path that failed.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// A JSON document is malformed.
    #[error("invalid recipe document {path}: {source}")]
    Json {
        /// Path that failed.
        path: PathBuf,
        /// serde error.
        #[source]
        source: serde_json::Error,
    },
}

/// Item ids grouped by their `ResourceTypes` entries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceTypeIndex {
    items: BTreeMap<String, BTreeSet<String>>,
}

impl ResourceTypeIndex {
    /// Add one item to a resource type (test/plugin seam).
    pub fn insert(&mut self, resource_type: impl Into<String>, item_id: impl Into<String>) {
        self.items
            .entry(resource_type.into())
            .or_default()
            .insert(item_id.into());
    }

    /// Item ids carrying one resource type, in deterministic order.
    pub fn items(&self, resource_type: &str) -> Option<&BTreeSet<String>> {
        self.items.get(resource_type)
    }

    /// Number of resource types.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether no resource type is indexed.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn add_item(&mut self, item_id: &str, value: &Value) {
        collect_resource_types(value, &mut |resource_type| {
            self.items
                .entry(resource_type.to_string())
                .or_default()
                .insert(item_id.to_string());
        });
    }
}

/// Per-player crafting progression (knowledge, memories, bench tiers,
/// augment tags).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CraftingKnowledge {
    /// Learned knowledge-gated recipe ids.
    #[serde(default)]
    pub learned: BTreeSet<String>,
    /// Memories level.
    #[serde(default)]
    pub memories_level: i32,
    /// Bench id → tier level.
    #[serde(default)]
    pub bench_tiers: BTreeMap<String, i32>,
    /// Known bench augment tags.
    #[serde(default)]
    pub augment_tags: BTreeSet<String>,
}

/// Why a recipe is not craftable yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CraftingGate {
    /// The recipe needs learned knowledge.
    Knowledge,
    /// The player's memories level is too low.
    Memories,
    /// The open bench's tier is too low.
    BenchTier,
    /// A required bench augment tag is missing.
    Augment,
}

impl CraftingKnowledge {
    /// Whether the recipe's knowledge and memories gates pass.
    pub fn knows(&self, recipe: &CraftingRecipe) -> bool {
        let id = recipe.id.as_deref().unwrap_or_default();
        (!recipe.knowledge_required || self.learned.contains(id))
            && recipe.required_memories_level <= self.memories_level
    }

    /// Full craft gate including bench tier and augment tags.
    pub fn can_craft(&self, recipe: &CraftingRecipe) -> Result<(), CraftingGate> {
        let id = recipe.id.as_deref().unwrap_or_default();
        if recipe.knowledge_required && !self.learned.contains(id) {
            return Err(CraftingGate::Knowledge);
        }
        if recipe.required_memories_level > self.memories_level {
            return Err(CraftingGate::Memories);
        }
        for requirement in recipe.bench_requirement.iter().flatten() {
            if requirement.required_tier_level > self.bench_tier(&requirement.id) {
                return Err(CraftingGate::BenchTier);
            }
            if let Some(tags) = &requirement.required_augment_tags {
                if !tags.iter().all(|tag| self.augment_tags.contains(tag)) {
                    return Err(CraftingGate::Augment);
                }
            }
        }
        Ok(())
    }

    /// Learn one recipe id.
    pub fn learn(&mut self, id: impl Into<String>) {
        self.learned.insert(id.into());
    }

    /// Bench tier for one bench id (0 when unknown).
    pub fn bench_tier(&self, bench_id: &str) -> i32 {
        self.bench_tiers.get(bench_id).copied().unwrap_or(0)
    }
}

/// All known recipes by id plus the item/resource-type index.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecipeCatalog {
    recipes: BTreeMap<String, CraftingRecipe>,
    resource_types: ResourceTypeIndex,
}

impl RecipeCatalog {
    /// Load item-embedded and standalone recipes below an extracted asset root.
    pub fn load_root(assets_root: &Path) -> Result<Self, CraftingError> {
        let mut recipes = BTreeMap::new();
        let mut resource_types = ResourceTypeIndex::default();
        let items = assets_root.join(ITEMS_ROOT);
        walk_json(&items, &mut |path| {
            let Some(key) = path.file_stem().and_then(|stem| stem.to_str()) else {
                return Ok(());
            };
            let Some(value) = read_json(path)? else {
                return Ok(());
            };
            resource_types.add_item(key, &value);
            if let Some(recipe) = value.get("Recipe") {
                if let Some(recipe) = recipe_from_value(key, recipe) {
                    recipes.insert(key.to_string(), recipe);
                }
            }
            Ok(())
        })?;
        let standalone = assets_root.join(RECIPES_ROOT);
        walk_json(&standalone, &mut |path| {
            let Some(key) = path.file_stem().and_then(|stem| stem.to_str()) else {
                return Ok(());
            };
            let Some(value) = read_json(path)? else {
                return Ok(());
            };
            if let Some(recipe) = recipe_from_value(key, &value) {
                recipes.insert(key.to_string(), recipe);
            }
            Ok(())
        })?;
        Ok(Self {
            recipes,
            resource_types,
        })
    }

    /// One recipe.
    pub fn get(&self, id: &str) -> Option<&CraftingRecipe> {
        self.recipes.get(id)
    }

    /// Number of recipes.
    pub fn len(&self) -> usize {
        self.recipes.len()
    }

    /// Whether the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }

    /// Every recipe in id order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &CraftingRecipe)> {
        self.recipes.iter()
    }

    /// Recipes known by every player (Java's `UpdateKnownRecipes` init).
    pub fn known(&self) -> BTreeMap<String, CraftingRecipe> {
        self.recipes.clone()
    }

    /// Recipes visible to a player with the given learned recipe ids and
    /// memories level. Knowledge-gated recipes are excluded until learned;
    /// recipes above the player's memories level are excluded.
    pub fn known_for(
        &self,
        learned: &BTreeSet<String>,
        memories_level: i32,
    ) -> BTreeMap<String, CraftingRecipe> {
        self.recipes
            .iter()
            .filter(|(id, recipe)| {
                (!recipe.knowledge_required || learned.contains(*id))
                    && recipe.required_memories_level <= memories_level
            })
            .map(|(id, recipe)| (id.clone(), recipe.clone()))
            .collect()
    }

    /// Recipes that list a bench type in their `BenchRequirement`.
    pub fn for_bench(&self, bench_type: BenchType) -> BTreeMap<String, CraftingRecipe> {
        self.recipes
            .iter()
            .filter(|(_, recipe)| {
                recipe
                    .bench_requirement
                    .as_ref()
                    .is_some_and(|requirements| {
                        requirements
                            .iter()
                            .any(|requirement| requirement.r#type == bench_type)
                    })
            })
            .map(|(id, recipe)| (id.clone(), recipe.clone()))
            .collect()
    }

    /// Recipes for the initial `UpdateRecipes` burst.
    pub fn all(&self) -> BTreeMap<String, CraftingRecipe> {
        self.recipes.clone()
    }

    /// Bench recipes reachable with the player's knowledge, memories, tier and
    /// augment tags.
    pub fn for_bench_with_tier(
        &self,
        bench_type: BenchType,
        knowledge: &CraftingKnowledge,
    ) -> BTreeMap<String, CraftingRecipe> {
        self.for_bench(bench_type)
            .into_iter()
            .filter(|(_, recipe)| knowledge.can_craft(recipe).is_ok())
            .collect()
    }

    /// Item/resource-type index used by resource inputs.
    pub fn resource_types(&self) -> &ResourceTypeIndex {
        &self.resource_types
    }
}

fn collect_resource_types(value: &Value, visit: &mut impl FnMut(&str)) {
    match value {
        Value::Object(object) => {
            if let Some(entries) = object.get("ResourceTypes").and_then(Value::as_array) {
                for entry in entries {
                    if let Some(id) = entry.get("Id").and_then(Value::as_str) {
                        visit(id);
                    }
                }
            }
            for child in object.values() {
                collect_resource_types(child, visit);
            }
        }
        Value::Array(array) => {
            for child in array {
                collect_resource_types(child, visit);
            }
        }
        _ => {}
    }
}

fn recipe_from_value(id: &str, value: &Value) -> Option<CraftingRecipe> {
    let input = materials(value.get("Input"));
    let output = materials(value.get("Output"));
    let primary_output = value.get("PrimaryOutput").map(material);
    let bench_requirement = value
        .get("BenchRequirement")
        .and_then(Value::as_array)
        .map(|entries| entries.iter().map(bench_requirement).collect());
    if input.is_none() && output.is_none() && primary_output.is_none() {
        return None;
    }
    Some(CraftingRecipe {
        id: Some(id.to_string()),
        inputs: input,
        outputs: output,
        primary_output,
        bench_requirement,
        knowledge_required: value
            .get("KnowledgeRequired")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        time_seconds: value
            .get("TimeSeconds")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        required_memories_level: value
            .get("RequiredMemoriesLevel")
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or(0),
    })
}

fn materials(value: Option<&Value>) -> Option<Vec<MaterialQuantity>> {
    let array = value?.as_array()?;
    (!array.is_empty()).then(|| array.iter().map(material).collect())
}

fn material(value: &Value) -> MaterialQuantity {
    MaterialQuantity {
        item_id: value
            .get("ItemId")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        item_tag: value
            .get("ItemTag")
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or(0),
        resource_type_id: value
            .get("ResourceTypeId")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        quantity: value
            .get("Quantity")
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or(1),
    }
}

fn bench_requirement(value: &Value) -> BenchRequirement {
    BenchRequirement {
        r#type: match value.get("Type").and_then(Value::as_str) {
            Some("Processing") => BenchType::Processing,
            Some("DiagramCrafting") => BenchType::DiagramCrafting,
            Some("StructuralCrafting") => BenchType::StructuralCrafting,
            _ => BenchType::Crafting,
        },
        id: value
            .get("Id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        categories: value
            .get("Categories")
            .and_then(Value::as_array)
            .map(|categories| {
                categories
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect()
            }),
        required_tier_level: value
            .get("RequiredTierLevel")
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or(0),
        required_augment_tags: value
            .get("RequiredAugmentTags")
            .and_then(Value::as_array)
            .map(|tags| {
                tags.iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect()
            }),
    }
}

fn read_json(path: &Path) -> Result<Option<Value>, CraftingError> {
    if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(|source| CraftingError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let value = serde_json::from_slice(&bytes).map_err(|source| CraftingError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(Some(value))
}

fn walk_json(
    root: &Path,
    visit: &mut impl FnMut(&Path) -> Result<(), CraftingError>,
) -> Result<(), CraftingError> {
    if !root.is_dir() {
        return Ok(());
    }
    let mut entries = std::fs::read_dir(root)
        .map_err(|source| CraftingError::Io {
            path: root.to_path_buf(),
            source,
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| CraftingError::Io {
            path: root.to_path_buf(),
            source,
        })?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk_json(&path, visit)?;
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("json") {
            visit(&path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Assets/extracted")
    }

    #[test]
    fn loads_item_and_standalone_recipes() {
        if !assets_root().join(ITEMS_ROOT).is_dir() {
            eprintln!("the extracted asset tree is not available — the test is skipped");
            return;
        }
        let catalog = RecipeCatalog::load_root(&assets_root()).expect("recipes");
        assert!(catalog.len() > 400, "{} recipes", catalog.len());
        let armor = catalog.get("Armor_Bronze_Chest").expect("bronze chest");
        assert_eq!(armor.inputs.as_ref().map(Vec::len), Some(3));
        assert_eq!(armor.time_seconds, 2.0);
        assert!(armor.knowledge_required);
        let bench = armor
            .bench_requirement
            .as_ref()
            .and_then(|requirements| requirements.first())
            .expect("bench requirement");
        assert_eq!(bench.id, "TODO");
        assert_eq!(bench.categories, Some(vec!["Armor_Chest".to_string()]));

        let salvage = catalog
            .get("Salvage_Plant_Flower_Common_Blue")
            .expect("salvage recipe");
        assert_eq!(
            salvage
                .primary_output
                .as_ref()
                .and_then(|output| output.item_id.as_deref()),
            Some("Plant_Petals_Blue")
        );
        assert_eq!(salvage.time_seconds, 0.5);
    }

    #[test]
    fn bench_filter_selects_recipes_for_the_window_type() {
        if !assets_root().join(ITEMS_ROOT).is_dir() {
            eprintln!("the extracted asset tree is not available — the test is skipped");
            return;
        }
        let catalog = RecipeCatalog::load_root(&assets_root()).expect("recipes");
        let processing = catalog.for_bench(BenchType::Processing);
        assert!(
            !processing.is_empty(),
            "real furnace recipes are catalogued"
        );
        for (id, recipe) in &processing {
            assert!(
                recipe
                    .bench_requirement
                    .as_ref()
                    .is_some_and(|requirements| requirements
                        .iter()
                        .any(|requirement| requirement.r#type == BenchType::Processing)),
                "`{id}` is a processing-bench recipe"
            );
        }
    }

    #[test]
    fn crafting_knowledge_gates_knowledge_memories_tiers_and_augments() {
        if !assets_root().join(ITEMS_ROOT).is_dir() {
            eprintln!("the extracted asset tree is not available — the test is skipped");
            return;
        }
        let catalog = RecipeCatalog::load_root(&assets_root()).expect("recipes");
        let armor = catalog.get("Armor_Bronze_Chest").expect("bronze chest");
        let mut knowledge = CraftingKnowledge::default();
        assert_eq!(knowledge.can_craft(armor), Err(CraftingGate::Knowledge));
        knowledge.learn("Armor_Bronze_Chest");
        assert!(knowledge.can_craft(armor).is_ok());

        let tiered = catalog
            .get("Armor_Adamantite_Chest")
            .expect("adamantite chest");
        assert_eq!(knowledge.can_craft(tiered), Err(CraftingGate::BenchTier));
        assert!(!catalog
            .for_bench_with_tier(BenchType::Crafting, &knowledge)
            .contains_key("Armor_Adamantite_Chest"));
        knowledge.bench_tiers.insert("Armor_Bench".to_string(), 3);
        assert!(knowledge.can_craft(tiered).is_ok());
        assert!(catalog
            .for_bench_with_tier(BenchType::Crafting, &knowledge)
            .contains_key("Armor_Adamantite_Chest"));

        let augmented = catalog
            .get("Goblin_Painting_Breach_Replica")
            .expect("goblin painting");
        knowledge.learn("Goblin_Painting_Breach_Replica");
        assert_eq!(knowledge.can_craft(augmented), Err(CraftingGate::Augment));
        knowledge
            .augment_tags
            .insert("goblin_lab_power".to_string());
        assert!(knowledge.can_craft(augmented).is_ok());

        let gated = catalog
            .iter()
            .find(|(_, recipe)| recipe.required_memories_level > 0)
            .map(|(_, recipe)| recipe.clone());
        if let Some(recipe) = gated {
            let mut synthetic = recipe.clone();
            synthetic.id = Some("Synthetic_Memories".to_string());
            synthetic.knowledge_required = false;
            synthetic.bench_requirement = None;
            let mut memories = CraftingKnowledge::default();
            assert_eq!(memories.can_craft(&synthetic), Err(CraftingGate::Memories));
            memories.memories_level = synthetic.required_memories_level;
            assert!(memories.can_craft(&synthetic).is_ok());
        }
    }

    #[test]
    fn known_recipes_gate_on_learned_ids_and_memories_level() {
        if !assets_root().join(ITEMS_ROOT).is_dir() {
            eprintln!("the extracted asset tree is not available — the test is skipped");
            return;
        }
        let catalog = RecipeCatalog::load_root(&assets_root()).expect("recipes");
        let unknown = catalog.known_for(&BTreeSet::new(), 0);
        assert!(
            !unknown.contains_key("Armor_Bronze_Chest"),
            "knowledge-gated recipes stay hidden until learned"
        );
        assert!(
            unknown.contains_key("Salvage_Plant_Flower_Common_Blue"),
            "ungated salvage recipes are known by default"
        );

        let learned = ["Armor_Bronze_Chest".to_string()].into_iter().collect();
        let with_knowledge = catalog.known_for(&learned, 0);
        assert!(with_knowledge.contains_key("Armor_Bronze_Chest"));

        let gated = catalog
            .iter()
            .find(|(_, recipe)| recipe.required_memories_level > 0)
            .map(|(id, _)| id.clone());
        if let Some(id) = gated {
            assert!(!catalog.known_for(&BTreeSet::new(), 0).contains_key(&id));
            assert!(catalog
                .known_for(&BTreeSet::new(), i32::MAX)
                .contains_key(&id));
        }
    }

    #[test]
    fn resource_type_index_maps_real_items() {
        if !assets_root().join(ITEMS_ROOT).is_dir() {
            eprintln!("the extracted asset tree is not available — the test is skipped");
            return;
        }
        let catalog = RecipeCatalog::load_root(&assets_root()).expect("recipes");
        let index = catalog.resource_types();
        assert!(!index.is_empty());
        let rocks = index.items("Rock").expect("Rock resource type");
        assert!(rocks.contains("Rock_Stone"), "{rocks:?}");
        let wood = index.items("Wood_Trunk").expect("Wood_Trunk");
        assert!(wood.contains("Wood_Amber_Trunk"), "{wood:?}");
        assert!(index.items("Milk_Bucket").is_some());
    }

    #[test]
    fn processing_bench_type_is_preserved() {
        if !assets_root().join(RECIPES_ROOT).is_dir() {
            eprintln!("the extracted asset tree is not available — the test is skipped");
            return;
        }
        let catalog = RecipeCatalog::load_root(&assets_root()).expect("recipes");
        let recipe = catalog
            .get("Salvage_Plant_Flower_Common_Blue")
            .expect("salvage");
        let bench = recipe
            .bench_requirement
            .as_ref()
            .and_then(|requirements| requirements.first())
            .expect("bench requirement");
        assert_eq!(bench.r#type, BenchType::Processing);
        assert_eq!(bench.id, "Salvagebench");
    }
}
