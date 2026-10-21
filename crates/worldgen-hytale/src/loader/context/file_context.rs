// Ported from: `.hytale-source/.../worldgen/loader/context/FileContext.java`
//! Java `loader.context.FileContext` (base class + `Registry` +
//! `RootContext`).
//!
//! Java models the loader context tree with a generic parent type; the Rust
//! port replaces the generics with a [`ParentContext`] enum over `Rc`s
//! (`RootContext.INSTANCE` becomes [`ParentContext::Root`]).
//!
//! The parent links form cycles (`FileLoadingContext` ↔ `ZoneFileContext` ↔
//! `BiomeFileContext`), exactly like the Java object graph; the `Rc` cycles
//! live until process end — the same effective lifetime of a loaded world.

use std::rc::Rc;

use super::file_loading_context::FileLoadingContext;
use super::zone_file_context::ZoneFileContext;

/// Java `FileContext<T>` base state (`id`, `name`, `filepath`,
/// `parentContext`) with the parent typed as an enum.
pub struct FileContext {
    /// Java `private final int id`.
    pub id: i32,
    /// Java `@Nonnull private final String name`.
    pub name: String,
    /// Java `@Nonnull private final Path filepath`.
    pub filepath: std::path::PathBuf,
    /// Java `@Nonnull private final T parentContext`.
    pub parent: ParentContext,
    /// Java `private transient String rootPath` (lazily computed cache).
    root_path: std::cell::RefCell<Option<String>>,
    /// Java `private transient String contentPath` (lazily computed cache).
    content_path: std::cell::RefCell<Option<String>>,
}

/// The typed parent link (Java: the generic `T extends FileContext<?>`).
pub enum ParentContext {
    /// Java `FileContext.RootContext.INSTANCE`.
    Root,
    /// Java a `FileLoadingContext` parent.
    Loading(Rc<FileLoadingContext>),
    /// Java a `ZoneFileContext` parent.
    Zone(Rc<ZoneFileContext>),
}

impl FileContext {
    /// Java `FileContext(int id, String name, Path filepath, T parentContext)`.
    pub fn new(id: i32, name: String, filepath: std::path::PathBuf, parent: ParentContext) -> Self {
        Self {
            id,
            name,
            filepath,
            parent,
            root_path: std::cell::RefCell::new(None),
            content_path: std::cell::RefCell::new(None),
        }
    }

    /// Java `int getId()`.
    pub fn get_id(&self) -> i32 {
        self.id
    }

    /// Java `String getName()`.
    pub fn get_name(&self) -> &str {
        &self.name
    }

    /// Java `Path getPath()`.
    pub fn get_path(&self) -> &std::path::Path {
        &self.filepath
    }

    /// Java `String getRootPath()`.
    pub fn get_root_path(&self) -> String {
        if let Some(cached) = self.root_path.borrow().as_ref() {
            return cached.clone();
        }
        let root = self
            .get_root()
            .base
            .filepath
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        *self.root_path.borrow_mut() = Some(root.clone());
        root
    }

    /// Java `String getContentPath()` — the path components below the root,
    /// joined with `'.'`, extension trimmed on the filename.
    pub fn get_content_path(&self) -> String {
        if let Some(cached) = self.content_path.borrow().as_ref() {
            return cached.clone();
        }
        let content = Self::to_content_path(&self.filepath, &self.parent);
        *self.content_path.borrow_mut() = Some(content.clone());
        content
    }

    /// Java `FileContext<?> getRoot()` — walks the parents to the loading
    /// context.
    pub fn get_root(&self) -> &FileLoadingContext {
        let mut current: &ParentContext = &self.parent;
        loop {
            match current {
                ParentContext::Root => unreachable!("FileContext with Root parent is the root"),
                ParentContext::Loading(loading) => return loading,
                ParentContext::Zone(zone) => {
                    current = &zone.base.parent;
                }
            }
        }
    }

    /// Java `static String toContentPath(Path filepath, FileContext<?> parent)`.
    fn to_content_path(filepath: &std::path::Path, parent: &ParentContext) -> String {
        let names: Vec<String> = filepath
            .components()
            .filter(|c| {
                !matches!(
                    c,
                    std::path::Component::RootDir | std::path::Component::CurDir
                )
            })
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();

        // Java: `start = parent == RootContext.INSTANCE ? 0 :
        // parent.getRoot().filepath.getNameCount()`.
        let start = match parent {
            ParentContext::Root => 0,
            other => {
                let root_path = match other {
                    ParentContext::Loading(loading) => loading.base.filepath.clone(),
                    ParentContext::Zone(zone) => zone.base.get_root().base.filepath.clone(),
                    ParentContext::Root => unreachable!(),
                };
                root_path
                    .components()
                    .filter(|c| {
                        !matches!(
                            c,
                            std::path::Component::RootDir | std::path::Component::CurDir
                        )
                    })
                    .count()
            }
        };

        let mut sb = String::new();
        for (i, name) in names.iter().enumerate().skip(start) {
            if i > start {
                sb.push('.');
            }

            let mut end = name.len(); // exclusive

            // If this is the filename part of the path, trim off the file
            // extension (if present).
            if i == names.len() - 1 {
                if let Some(ext) = name.rfind('.') {
                    end = ext;
                }
            }

            sb.push_str(&name[..end]);
        }

        sb
    }
}

/// Java `FileContext.Registry<T>` — an insertion-ordered named registry
/// (Java `Object2ObjectLinkedOpenHashMap`).
pub struct Registry<T> {
    /// Java `private final String registryName`.
    registry_name: String,
    /// Java `Object2ObjectMap<String, T> backing` (linked = insertion order).
    backing: Vec<(String, Rc<T>)>,
    index: std::collections::HashMap<String, usize>,
}

impl<T> Registry<T> {
    /// Java `Registry(String name)`.
    pub fn new(registry_name: &str) -> Self {
        Self {
            registry_name: registry_name.to_string(),
            backing: Vec::new(),
            index: std::collections::HashMap::new(),
        }
    }

    /// Java `int size()`.
    pub fn size(&self) -> usize {
        self.backing.len()
    }

    /// Java `String getName()`.
    pub fn get_name(&self) -> &str {
        &self.registry_name
    }

    /// Java `boolean contains(String name)`.
    pub fn contains(&self, name: &str) -> bool {
        self.index.contains_key(name)
    }

    /// Java `T get(String name)` — Java `Error` when missing.
    pub fn get(&self, name: &str) -> Rc<T> {
        match self.index.get(name) {
            Some(&i) => self.backing[i].1.clone(),
            None => panic!("Missing {} entry for key {}", self.registry_name, name),
        }
    }

    /// Java `T getOrNull` equivalent — Java callers use `contains` first.
    pub fn get_opt(&self, name: &str) -> Option<Rc<T>> {
        self.index.get(name).map(|&i| self.backing[i].1.clone())
    }

    /// Java `void register(String name, T value)` — Java `Error` on duplicates.
    pub fn register(&mut self, name: &str, value: Rc<T>) {
        if self.index.contains_key(name) {
            panic!(
                "Duplicate {} entry registered for key {}",
                self.registry_name, name
            );
        }
        self.index.insert(name.to_string(), self.backing.len());
        self.backing.push((name.to_string(), value));
    }

    /// Java `Iterator<Map.Entry<String, T>> iterator()` — insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Rc<T>)> {
        self.backing
            .iter()
            .map(|(name, value)| (name.as_str(), value))
    }
}

impl<T> Default for Registry<T> {
    fn default() -> Self {
        Self::new("")
    }
}
