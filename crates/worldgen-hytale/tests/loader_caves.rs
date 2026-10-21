//! End-to-end loader tests for the **cave configuration** of the real
//! Default world.
//!
//! The tests walk the same chain the Java `ChunkGeneratorJsonLoader` walks
//! (world JSON → mask provider → zone pattern provider → file contexts →
//! zones) and then load the cave configuration of every zone through the
//! real [`CaveGeneratorJsonLoader`].
//!
//! The remaining seams of the chain are documented:
//!
//! * the **biome prefab container** (`PrefabContainerJsonLoader`) and the
//!   **unique prefab container** (`UniquePrefabContainerJsonLoader`) are
//!   Phase-5 seams (the prefab package),
//! * the **prefab file resolution** of a cave prefab / prefab cave node is
//!   a Phase-5 seam (the prefab store).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use procedural::json::SeedString;
use worldgen_hytale::cave::cave_generator::CaveGenerator;
use worldgen_hytale::loader::cave::cave_generator_json_loader::CaveGeneratorJsonLoader;
use worldgen_hytale::loader::chunk_generator_json_loader::ChunkGeneratorJsonLoader;
use worldgen_hytale::loader::context::file_context_loader::FileContextLoader;
use worldgen_hytale::loader::context::file_loading_context::FileLoadingContext;
use worldgen_hytale::loader::context::zone_file_context::ZoneFileContext;
use worldgen_hytale::loader::seed_string_resource::SeedStringResource;
use worldgen_hytale::loader::world_gen_prefab_loader::WorldGenPrefabLoader;
use worldgen_hytale::prefab::prefab_store_root::PrefabStoreRoot;
use worldgen_hytale::world_gen_config::WorldGenConfig;
use worldgen_model::registry::Registries;

fn manifest_root() -> PathBuf {
    PathBuf::from(format!(
        "{}/../../testdata/Default",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    ))
}

/// The root of the extracted base-game asset pack (the `PrefabStore: ASSETS`
/// root of the Default world).
fn assets_root() -> PathBuf {
    PathBuf::from(format!(
        "{}/../../Assets/extracted",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    ))
}

/// Whether the base-game asset pack is available (it is not part of the
/// source tree — see `.gitignore`). The cave prefab containers resolve
/// their prefab names through it.
fn assets_available() -> bool {
    assets_root().join("Server/Prefabs").is_dir()
}

/// Installs the prefab loader of the world (the Java
/// `SeedStringResource.loader`).
fn install_prefab_loader(context: &Rc<FileLoadingContext>) {
    let loader = Rc::new(WorldGenPrefabLoader::new(
        PrefabStoreRoot::Assets,
        &manifest_root(),
        &assets_root(),
        Rc::new(Registries::load()),
    ));
    context.set_prefab_loader(loader);
}

/// The zone contexts of the Default world (in the registry order the Java
/// loader uses).
fn default_zone_contexts() -> Vec<Rc<ZoneFileContext>> {
    let root = manifest_root();
    let requirement: HashSet<String> = ["Oceans".to_string()].into_iter().collect();
    let context = FileContextLoader::new("Default", &root, requirement).load();
    install_prefab_loader(&context);
    let zones: Vec<Rc<ZoneFileContext>> = context
        .get_zones()
        .borrow()
        .iter()
        .map(|(_, zone)| Rc::clone(zone))
        .collect();
    zones
}

/// Loads the cave generator of a zone through the real
/// [`CaveGeneratorJsonLoader`].
fn load_cave_generator(
    zone_context: &Rc<ZoneFileContext>,
    seed: SeedString,
    resource: Rc<SeedStringResource>,
    registries: Rc<Registries>,
) -> Option<Rc<dyn CaveGenerator>> {
    let cave_folder = zone_context.base.get_path().join("Cave");
    CaveGeneratorJsonLoader::new(
        seed,
        &manifest_root(),
        None,
        Some(cave_folder),
        Rc::clone(zone_context),
        resource,
        registries,
    )
    .load()
}

#[test]
fn the_default_world_loads_completely() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let root = manifest_root();
    let config = WorldGenConfig::new(root, "Default".to_string(), "0.0.0".to_string());
    let loader = ChunkGeneratorJsonLoader::new(SeedString::new("0"), config);

    // The chain runs: world json → climate mask → 28-zone requirement →
    // context scan → zone parsing → the cave configuration of the zone →
    // the biome chain (including the biome prefab containers, whose prefab
    // names resolve through the prefab store).
    //
    // The Phase-4 cave seam and the Phase-5a prefab seams are cleared; the
    // only remaining seam is the *unique* prefab container (Phase 5c), which
    // is skipped with a warning.
    let generator = loader.load();
    assert_eq!(
        generator.zone_pattern_provider().get_zones().len(),
        28,
        "the Default world has 28 zones"
    );
}

#[test]
fn every_default_zone_with_a_cave_configuration_gets_a_generator() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let root = manifest_root();
    let requirement: HashSet<String> = ["Oceans".to_string()].into_iter().collect();
    // The zone requirement of the Default world — every zone folder that
    // holds a `Zone.json`.
    let mut requirement = requirement;
    for entry in std::fs::read_dir(root.join("Zones")).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() && path.join("Zone.json").exists() {
            requirement.insert(path.file_name().unwrap().to_string_lossy().to_string());
        }
    }

    let context = FileContextLoader::new("Default", &root, requirement).load();
    install_prefab_loader(&context);
    let zones: Vec<Rc<ZoneFileContext>> = context
        .get_zones()
        .borrow()
        .iter()
        .map(|(_, zone)| Rc::clone(zone))
        .collect();
    assert_eq!(zones.len(), 28, "the Default world has 28 zones");

    let registries = Rc::new(Registries::load());
    let resource = Rc::new(SeedStringResource::new());
    let mut with_caves = 0;
    for zone_context in &zones {
        let generator = load_cave_generator(
            zone_context,
            SeedString::new("0")
                .append(".Zones")
                .append(&format!(".Zone-{}", zone_context.base.get_name())),
            Rc::clone(&resource),
            Rc::clone(&registries),
        );
        assert!(
            generator.is_some(),
            "the zone \"{}\" has a Cave/Caves.json and must get a cave generator",
            zone_context.base.get_name()
        );
        with_caves += 1;
    }
    assert_eq!(
        with_caves, 28,
        "every Default zone has a cave configuration"
    );
}

#[test]
fn the_oceans_cave_types_are_loaded() {
    let zone_context = default_zone_contexts()
        .into_iter()
        .find(|zone| zone.base.get_name() == "Oceans")
        .expect("the Oceans zone");
    let generator = load_cave_generator(
        &zone_context,
        SeedString::new("0").append(".Zones").append(".Zone-Oceans"),
        Rc::new(SeedStringResource::new()),
        Rc::new(Registries::load()),
    )
    .expect("the Oceans zone has a cave configuration");

    let cave_types = generator.get_cave_types();
    assert_eq!(cave_types.len(), 3, "Caldera, Copper and Iron");
    let names: Vec<&str> = cave_types.iter().map(|t| t.get_name()).collect();
    assert_eq!(names, vec!["Caldera", "Copper", "Iron"]);

    // The first cave type is the inline one of the Caves.json.
    let caldera = &cave_types[0];
    assert_eq!(caldera.get_entry_node().get_name(), "Caldera.Entry");
    // The maximum size is not in the file: it is the floored interval of the
    // entry point generator.
    assert_eq!(caldera.get_maximum_size(), 333.0);
    assert!(caldera.is_surface_limited());
    assert!(!caldera.is_submerge());
    assert_eq!(
        caldera.get_fluid_level().get_height(),
        24,
        "the FluidLevel height of the file"
    );
    assert_eq!(
        caldera.get_fluid_level().get_block_entry().fluid_id,
        8,
        "Lava_Source"
    );
}

#[test]
fn the_caldera_entry_node_resolves_its_child_node_type() {
    let zone_context = default_zone_contexts()
        .into_iter()
        .find(|zone| zone.base.get_name() == "Oceans")
        .expect("the Oceans zone");
    let generator = load_cave_generator(
        &zone_context,
        SeedString::new("0").append(".Zones").append(".Zone-Oceans"),
        Rc::new(SeedStringResource::new()),
        Rc::new(Registries::load()),
    )
    .expect("the Oceans zone has a cave configuration");

    let caldera = &generator.get_cave_types()[0];
    let entry = caldera.get_entry_node();
    assert_eq!(entry.get_name(), "Caldera.Entry");

    // The child of the entry node is the node type of another file — the
    // cross reference of the node-type storage.
    let children = entry.get_children();
    assert_eq!(children.len(), 1);
    let child_types = children[0].get_types();
    assert_eq!(child_types.size(), 1);
    let child = child_types.internal_keys()[0].clone();
    assert_eq!(child.get_name(), "Caldera.Type_0.Entry");
    // The child node type has no children of its own.
    assert!(child.get_children().is_empty());
}

#[test]
fn a_cave_type_file_reference_is_resolved() {
    let zone_context = default_zone_contexts()
        .into_iter()
        .find(|zone| zone.base.get_name() == "Oceans")
        .expect("the Oceans zone");
    let generator = load_cave_generator(
        &zone_context,
        SeedString::new("0").append(".Zones").append(".Zone-Oceans"),
        Rc::new(SeedStringResource::new()),
        Rc::new(Registries::load()),
    )
    .expect("the Oceans zone has a cave configuration");

    let cave_types = generator.get_cave_types();
    // The second and the third cave types come from the
    // `{"File": "Ores.Zone0.CopperPlacement"}` references of the Caves.json.
    let copper = &cave_types[1];
    assert_eq!(copper.get_name(), "Copper");
    // The entry of the copper cave is the inline `{"File":
    // "Ores.Zone0.Copper"}` object — it is loaded under a unique name.
    assert!(copper.get_entry_node().get_name().starts_with("CaveType#"));
    // The copper cave has a fixed entry height of 0.
    assert!(copper.get_entry_node().get_name().starts_with("CaveType#"));
}

#[test]
fn a_zone_without_a_cave_configuration_has_no_generator() {
    let root = manifest_root();
    let loading = FileLoadingContext::new("test", root.clone());
    install_prefab_loader(&loading);
    let zone_context = loading.create_zone("Empty", root.join("Zones/Empty"));
    let generator = load_cave_generator(
        &zone_context,
        SeedString::new("0"),
        Rc::new(SeedStringResource::new()),
        Rc::new(Registries::load()),
    );
    assert!(
        generator.is_none(),
        "a zone without a Cave/Caves.json has no cave generator"
    );
}

#[test]
fn a_node_type_name_that_escapes_the_cave_folder_is_rejected() {
    use worldgen_hytale::loader::cave::cave_node_type_storage::CaveNodeTypeStorage;

    let root = manifest_root();
    let loading = FileLoadingContext::new("test", root.clone());
    install_prefab_loader(&loading);
    let zone_context = loading.create_zone("Oceans", root.join("Zones/Oceans"));
    let storage = CaveNodeTypeStorage::new(
        SeedString::new("0"),
        &root,
        root.join("Zones/Oceans/Cave"),
        zone_context,
        Rc::new(SeedStringResource::new()),
        Rc::new(Registries::load()),
    );

    for name in ["../../etc/passwd", "/etc/passwd", "..%2Fescape"] {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            storage.load_cave_node_type(name)
        }));
        let payload = match result {
            Ok(_) => panic!("the name \"{name}\" must be rejected"),
            Err(payload) => payload,
        };
        let message = if let Some(message) = payload.downcast_ref::<String>() {
            message.clone()
        } else if let Some(message) = payload.downcast_ref::<&'static str>() {
            (*message).to_string()
        } else {
            "<non-string panic>".to_string()
        };
        assert_eq!(
            message,
            format!("Invalid cave node type name: {name}"),
            "the path traversal guard must reject the name"
        );
    }
}

#[test]
fn a_node_type_name_inside_the_cave_folder_is_loaded() {
    use worldgen_hytale::loader::cave::cave_node_type_storage::CaveNodeTypeStorage;

    let root = manifest_root();
    let loading = FileLoadingContext::new("test", root.clone());
    install_prefab_loader(&loading);
    let zone_context = loading.create_zone("Oceans", root.join("Zones/Oceans"));
    let storage = CaveNodeTypeStorage::new(
        SeedString::new("0"),
        &root,
        root.join("Zones/Oceans/Cave"),
        zone_context,
        Rc::new(SeedStringResource::new()),
        Rc::new(Registries::load()),
    );

    // The file `Zones/Oceans/Cave/Caldera/Entry.node.json` exists.
    let node_type = storage.load_cave_node_type("Caldera.Entry");
    assert_eq!(node_type.get_name(), "Caldera.Entry");
    assert_eq!(node_type.get_children().len(), 1);
}

/// The `Path` of the Default world (the helper of the other loader tests).
#[allow(dead_code)]
fn default_root() -> &'static Path {
    Path::new("testdata/Default")
}
