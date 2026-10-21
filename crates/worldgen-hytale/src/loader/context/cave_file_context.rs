// Ported from: `.hytale-source/.../worldgen/loader/context/CaveFileContext.java`
//! Java `loader.context.CaveFileContext` — the file context of the cave
//! configuration of a zone.
//!
//! The context points at `<zone folder>/Cave/<name>`, where the dots of the
//! name become the path separators (the Java `name.replace(".", separator)`).

use std::path::PathBuf;
use std::rc::Rc;

use super::file_context::FileContext;
use super::zone_file_context::ZoneFileContext;
use crate::loader::zone::zone_json_loader::constants::PATH_CAVE;

/// Java `CaveFileContext`.
pub struct CaveFileContext {
    /// Java `extends FileContext<ZoneFileContext>`.
    pub base: FileContext,
}

impl CaveFileContext {
    /// Java `CaveFileContext(String name, ZoneFileContext parentContext)`.
    ///
    /// The path is `<zone folder>/Cave/<name with dots as separators>`.
    pub fn new(name: &str, parent: Rc<ZoneFileContext>) -> Rc<Self> {
        let path = Self::resolve_path(&parent, name);
        Rc::new(Self {
            base: FileContext::new(
                0,
                name.to_string(),
                path,
                super::file_context::ParentContext::Zone(parent),
            ),
        })
    }

    /// Java `CaveFileContext(String name, Path relativePath, ZoneFileContext
    /// parentContext)`.
    ///
    /// The path is used verbatim (the Java constructor takes the path of the
    /// cave file the context belongs to).
    pub fn with_path(name: &str, path: PathBuf, parent: Rc<ZoneFileContext>) -> Rc<Self> {
        Rc::new(Self {
            base: FileContext::new(
                0,
                name.to_string(),
                path,
                super::file_context::ParentContext::Zone(parent),
            ),
        })
    }

    /// Java `static Path resolvePath(ZoneFileContext context, String name)`.
    fn resolve_path(context: &Rc<ZoneFileContext>, name: &str) -> PathBuf {
        let filepath = name.replace('.', std::path::MAIN_SEPARATOR_STR);
        context.base.get_path().join(PATH_CAVE).join(filepath)
    }

    /// Java `getParentContext()` — the owning zone context (typed).
    pub fn get_parent_context(&self) -> Rc<ZoneFileContext> {
        match &self.base.parent {
            super::file_context::ParentContext::Zone(zone) => zone.clone(),
            _ => unreachable!("CaveFileContext parent is always a ZoneFileContext"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::context::file_loading_context::FileLoadingContext;

    /// A zone context rooted at `<tmp>/Zones/Oceans`.
    fn zone_context() -> Rc<ZoneFileContext> {
        let loading = FileLoadingContext::new("test", PathBuf::from("/tmp/world"));
        loading.create_zone("Oceans", PathBuf::from("/tmp/world/Zones/Oceans"))
    }

    #[test]
    fn the_path_points_into_the_cave_folder_of_the_zone() {
        let context = CaveFileContext::new("Caldera", zone_context());
        assert_eq!(
            context.base.get_path(),
            std::path::Path::new("/tmp/world/Zones/Oceans/Cave/Caldera")
        );
        assert_eq!(context.base.get_name(), "Caldera");
        assert_eq!(context.base.get_id(), 0);
    }

    #[test]
    fn the_dots_of_the_name_become_separators() {
        let context = CaveFileContext::new("Caldera.Type_0", zone_context());
        assert_eq!(
            context.base.get_path(),
            std::path::Path::new("/tmp/world/Zones/Oceans/Cave/Caldera/Type_0")
        );
    }

    #[test]
    fn an_explicit_path_is_used_verbatim() {
        let context = CaveFileContext::with_path(
            "Entry",
            PathBuf::from("/tmp/world/Zones/Oceans/Cave/Caldera/Entry"),
            zone_context(),
        );
        assert_eq!(
            context.base.get_path(),
            std::path::Path::new("/tmp/world/Zones/Oceans/Cave/Caldera/Entry")
        );
    }

    #[test]
    fn the_parent_is_the_zone_context() {
        let zone = zone_context();
        let context = CaveFileContext::new("Caldera", zone.clone());
        assert!(Rc::ptr_eq(&context.get_parent_context(), &zone));
    }
}
