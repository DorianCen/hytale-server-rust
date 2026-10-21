// Ported from: `.hytale-source/.../common/util/PathUtil.java`
//! Java `com.hypixel.hytale.common.util.PathUtil` — the path helpers the
//! loaders use to resolve a *relative* path inside a directory without
//! escaping it.
//!
//! The three functions the loaders need are transcribed 1:1:
//!
//! * [`normalize_path`] — Java `Path.normalize()` (the *lexical*
//!   normalization: `.` is dropped and `..` pops the previous element; the
//!   file system is not touched).
//! * [`absolute_path`] — Java `Path.toAbsolutePath()` (the path is made
//!   absolute *without* normalizing it).
//! * [`resolve_path_within_dir`] — Java
//!   `PathUtil.resolvePathWithinDir(Path, String)`: the relative path is
//!   resolved inside the directory, normalized and then checked with
//!   [`is_child_of`]. A path that escapes the directory yields `None` (Java
//!   returns `null`).

use std::path::{Component, Path, PathBuf};

/// Java `Path.normalize()` — the lexical normalization.
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let pop = matches!(out.components().next_back(), Some(Component::Normal(_)));
                if pop {
                    out.pop();
                } else if !matches!(out.components().next_back(), Some(Component::RootDir)) {
                    out.push(component.as_os_str());
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Java `Path.toAbsolutePath()` — the path is made absolute without
/// normalizing it.
pub fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(dir) => dir.join(path),
            Err(_) => path.to_path_buf(),
        }
    }
}

/// Java `PathUtil.isChildOf(Path parent, Path child)`.
pub fn is_child_of(parent: &Path, child: &Path) -> bool {
    let parent = normalize_path(&absolute_path(parent));
    let child = normalize_path(&absolute_path(child));
    child.starts_with(parent)
}

/// Java `PathUtil.resolvePathWithinDir(Path directory, String
/// relativePath)` — resolves the relative path inside the directory and
/// rejects a path that escapes it.
///
/// The Java overload takes a `String`; the path separators of the string are
/// the platform ones, so the callers pass a path that was already built with
/// `File.separator` (the cave node type storage replaces the `.` of a node
/// type name with the separator).
pub fn resolve_path_within_dir(directory: &Path, relative_path: &str) -> Option<PathBuf> {
    let resolved = normalize_path(&directory.join(relative_path));
    if !is_child_of(directory, &resolved) {
        return None;
    }
    Some(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_normalization_resolves_the_dot_segments() {
        assert_eq!(
            normalize_path(Path::new("/a/b/../c/./d")),
            PathBuf::from("/a/c/d")
        );
        assert_eq!(
            normalize_path(Path::new("a/../../b")),
            PathBuf::from("../b")
        );
        assert_eq!(normalize_path(Path::new("/a/b/")), PathBuf::from("/a/b"));
    }

    #[test]
    fn a_relative_path_is_resolved_inside_the_directory() {
        let directory = Path::new("/data/Zones/Oceans/Cave");
        assert_eq!(
            resolve_path_within_dir(directory, "Caldera/Entry.node.json"),
            Some(PathBuf::from(
                "/data/Zones/Oceans/Cave/Caldera/Entry.node.json"
            ))
        );
    }

    #[test]
    fn a_path_that_escapes_the_directory_is_rejected() {
        let directory = Path::new("/data/Zones/Oceans/Cave");
        assert_eq!(
            resolve_path_within_dir(directory, "../Caves.node.json"),
            None
        );
        assert_eq!(
            resolve_path_within_dir(directory, "../../../etc/passwd"),
            None
        );
    }

    #[test]
    fn an_absolute_path_is_rejected_unless_it_is_inside() {
        let directory = Path::new("/data/Zones/Oceans/Cave");
        assert_eq!(resolve_path_within_dir(directory, "/etc/passwd"), None);
        assert_eq!(
            resolve_path_within_dir(directory, "/data/Zones/Oceans/Cave/Entry.node.json"),
            Some(PathBuf::from("/data/Zones/Oceans/Cave/Entry.node.json"))
        );
    }

    #[test]
    fn a_child_of_a_directory_is_detected() {
        assert!(is_child_of(
            Path::new("/data/Cave"),
            Path::new("/data/Cave/Entry.node.json")
        ));
        assert!(!is_child_of(
            Path::new("/data/Cave"),
            Path::new("/data/Other/Entry.node.json")
        ));
        // A sibling with a shared prefix is *not* a child.
        assert!(!is_child_of(
            Path::new("/data/Cave"),
            Path::new("/data/CaveOther/Entry.node.json")
        ));
    }
}
