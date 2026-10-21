// Ported from: `.hytale-source/.../worldgen/loader/**`
//! Java `worldgen.loader` package — the serde/JSON world loading path
//! (Subtask 7). The Java loader classes hand-parse gson trees; the Rust port
//! uses `procedural::json`'s [`procedural::json::JsonLoaderBase`] (the ported
//! `JsonLoader` base) plus per-loader structs with the same names, defaults
//! and error messages.

pub mod biome;
pub mod cave;
pub mod chunk_generator_json_loader;
pub mod climate;
pub mod color_util;
pub mod container;
pub mod context;
pub mod mask_provider_json_loader;
pub mod prefab;
pub mod seed_string_resource;
#[cfg(test)]
pub mod test_prefab_store;
pub mod util;
pub mod world_gen_prefab_loader;
pub mod zone;
pub mod zones_json_loader;

use std::path::Path;

use serde_json::Value;

/// Extracts the message of a caught panic.
///
/// A Rust panic carries either a `String` (from `panic!("…")` with a
/// format string) or a `&'static str` (from `panic!("…")` with a literal);
/// the Java error messages are compared verbatim in the tests, so both
/// payloads have to be handled.
pub(crate) fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        return (*message).to_string();
    }
    "<non-string panic>".to_string()
}

/// document. The Java `FileIO`/gson error text is wrapped by the callers that
/// need it (e.g. `ZonesJsonLoader`), so this helper keeps a generic message.
///
/// Java loads the documents with gson in its **lenient** mode, which accepts
/// `//` and `/* … */` comments; the vanilla world data uses them heavily.
/// The comments are stripped before the document is parsed (see
/// [`procedural::json::strip_json_comments`]).
pub fn load_json_object(path: &Path) -> Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to load file {}: {e}", path.display()));
    // Java gson (lenient mode) tolerates a leading UTF-8 BOM (`\uFEFF`); the
    // real Default-world `Zone.json` files carry one.
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let value: Value = serde_json::from_str(&procedural::json::strip_json_comments(text))
        .unwrap_or_else(|e| panic!("Failed to load file {}: {e}", path.display()));
    if !value.is_object() {
        panic!("Failed to load file {}: not a JSON object", path.display());
    }
    value
}
