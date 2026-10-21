// Ported from: `.hytale-source/.../procedurallib/json/JsonLoader.java`
//! Java `procedurallib.json.JsonLoader` (and its `Loader` base class) — the
//! shared base every Java `*JsonLoader` extends, ported as a composed base
//! struct ([`JsonLoaderBase`]) instead of inheritance.
//!
//! Semantics reproduced faithfully:
//!
//! * The **`"File"` reference indirection**: a JSON object may carry
//!   `"File": "Some.DotSeparated.Path"` instead of inline content; the
//!   constructor and `get` resolve it against `dataFolder` as
//!   `Some/Dot/Separated/Path.json` (Java `JSON_FILEPATH_SEPARATOR = '.'`),
//!   with the path-traversal check.
//! * `has` / `get` / `getRaw` including the `JsonNull`-vs-absent distinction
//!   (a present `null` key satisfies `has` but fails the type predicates).
//! * The `mustGet*` family with their exact error messages.
//!
//! Java loader `Error`/`IllegalStateException` throwables become Rust panics:
//! world loading is a one-time, fatal operation in Java as well, and the
//! messages are kept verbatim for debuggability.

pub mod abstract_cell_jitter_json_loader;
pub mod basic_height_threshold_interpreter_json_loader;
pub mod blend_noise_property_json_loader;
pub mod branch_noise_json_loader;
pub mod cell_border_distance_function_json_loader;
pub mod cell_distance_function_json_loader;
pub mod cell_noise_json_loader;
pub mod constant_noise_json_loader;
pub mod coordinate_randomizer_json_loader;
pub mod coordinate_rotator_json_loader;
pub mod curve_noise_property_json_loader;
pub mod distance_noise_json_loader;
pub mod double_condition_json_loader;
pub mod double_range_json_loader;
pub mod double_threshold_json_loader;
pub mod float_range_json_loader;
pub mod gradient_noise_property_json_loader;
pub mod grid_noise_json_loader;
pub mod height_threshold_interpreter_json_loader;
pub mod mesh_noise_json_loader;
pub mod noise_function_json_loader;
pub mod noise_height_threshold_interpreter_json_loader;
pub mod noise_mask_condition_json_loader;
pub mod noise_property_json_loader;
pub mod noise_type_json;
pub mod old_simplex_noise_json_loader;
pub mod perlin_noise_json_loader;
pub mod point_evaluator_json_loader;
pub mod point_generator_json_loader;
pub mod point_noise_json_loader;
pub mod seed_string;
pub mod simplex_noise_json_loader;
pub mod value_noise_json_loader;

pub use noise_function_json_loader::NoiseFunctionObj;
pub use seed_string::SeedString;

use std::path::{Path, PathBuf};

use serde_json::Value;

/// Java `JsonLoader.Constants.KEY_FILE`.
pub const KEY_FILE: &str = "File";
/// Java `JsonLoader.Constants.JSON_FILEPATH_SEPARATOR`.
const JSON_FILEPATH_SEPARATOR: char = '.';

/// Strips the JSON comments that gson's lenient mode accepts.
///
/// Java loads every world-gen JSON document through
/// `JsonResourceLoader`, which sets gson to `Strictness.LENIENT` — gson then
/// accepts `//` line comments, `#` line comments and `/* … */` block
/// comments. The vanilla world data uses them heavily (the Default world has
/// ~16 000 line comments and ~470 block comments, mostly in the cave node
/// files), while `serde_json` rejects them.
///
/// The stripper keeps the string literals intact (a `//` inside a string is
/// *not* a comment) and replaces a comment with a single space, so the
/// offsets of the remaining tokens stay close to the original.
///
/// Deviation: gson's lenient mode also accepts single-quoted strings,
/// unquoted object keys, trailing commas and the `NaN` / `Infinity`
/// literals. The vanilla world data uses none of them, so the stripper only
/// covers the comments.
pub fn strip_json_comments(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut index = 0usize;
    // Whether the scanner is inside a string literal.
    let mut in_string = false;
    // Whether the last character of a string literal was a backslash.
    let mut escaped = false;

    // Copies the UTF-8 sequence that starts at `index` and returns the index
    // behind it.
    fn copy_char(text: &str, bytes: &[u8], index: usize, out: &mut String) -> usize {
        let mut end = index + 1;
        while end < bytes.len() && (bytes[end] & 0xC0) == 0x80 {
            end += 1;
        }
        out.push_str(&text[index..end]);
        end
    }

    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            if escaped {
                escaped = false;
                index = copy_char(text, bytes, index, &mut out);
                continue;
            }
            match byte {
                b'\\' => {
                    escaped = true;
                    index = copy_char(text, bytes, index, &mut out);
                }
                b'"' => {
                    in_string = false;
                    out.push('"');
                    index += 1;
                }
                _ => index = copy_char(text, bytes, index, &mut out),
            }
            continue;
        }

        match byte {
            b'"' => {
                in_string = true;
                out.push('"');
                index += 1;
            }
            b'/' if index + 1 < bytes.len() && bytes[index + 1] == b'/' => {
                // A line comment: skip to the end of the line.
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
                out.push(' ');
            }
            b'#' => {
                // gson also accepts `#` line comments.
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
                out.push(' ');
            }
            b'/' if index + 1 < bytes.len() && bytes[index + 1] == b'*' => {
                // A block comment: skip to the closing `*/`.
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    index += 1;
                }
                index = (index + 2).min(bytes.len());
                out.push(' ');
            }
            _ => index = copy_char(text, bytes, index, &mut out),
        }
    }

    out
}

/// Java `JsonLoader`'s `protected` state (`seed`, `dataFolder`, `json`) and
/// methods, composed into every ported loader.
pub struct JsonLoaderBase {
    /// Java `protected SeedString<K> seed` (non-final in Java: a few loaders
    /// reassign it).
    pub seed: SeedString,
    /// Java `protected final Path dataFolder`.
    pub data_folder: PathBuf,
    /// Java `@Nullable protected final JsonElement json`.
    pub json: Option<Value>,
}

impl JsonLoaderBase {
    /// Java `JsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`
    /// — resolves the `"File"` constructor reference.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        let resolved = match &json {
            Some(Value::Object(obj)) => match obj.get(KEY_FILE) {
                Some(Value::String(file)) => Some(Self::load_file_at(data_folder, file)),
                _ => json,
            },
            _ => json,
        };
        Self {
            seed,
            data_folder: data_folder.to_path_buf(),
            json: resolved,
        }
    }

    /// Java `boolean has(String name)`.
    pub fn has(&self, name: &str) -> bool {
        match &self.json {
            Some(Value::Object(obj)) => obj.contains_key(name),
            _ => false,
        }
    }

    /// Java `JsonElement get(String name)` — resolves `"File"` references on
    /// object values. Returns `None` when absent (Java `null`); a present
    /// `null` yields `Some(Value::Null)` (Java `JsonNull`).
    pub fn get(&self, name: &str) -> Option<Value> {
        let element = self.get_raw(name)?;
        match &element {
            Value::Object(obj) => match obj.get(KEY_FILE) {
                Some(Value::String(file)) => {
                    let loaded = Self::load_file_elem(self.data_folder.as_path(), file);
                    Some(loaded.unwrap_or(element))
                }
                _ => Some(element),
            },
            _ => Some(element),
        }
    }

    /// Java `JsonElement getRaw(String name)`.
    pub fn get_raw(&self, name: &str) -> Option<Value> {
        match &self.json {
            Some(Value::Object(obj)) => obj.get(name).cloned(),
            _ => None,
        }
    }

    /// Java `@Nonnull JsonElement getOrLoad(@Nonnull JsonElement element)`.
    ///
    /// Resolves a `{"File": "Some.Path"}` reference against the data folder;
    /// every other element is returned unchanged.
    pub fn get_or_load(&self, element: &Value) -> Value {
        if let Value::Object(object) = element {
            if let Some(Value::String(file)) = object.get(KEY_FILE) {
                return Self::load_file_at(self.data_folder.as_path(), file);
            }
        }
        element.clone()
    }

    /// Java `JsonElement loadFile(String filePath)`.
    pub fn load_file(&self, file_path: &str) -> Value {
        Self::load_file_at(self.data_folder.as_path(), file_path)
    }

    /// Java `protected JsonElement loadFileElem(String filePath)` — identical
    /// to [`Self::load_file`] in the base class (exposed as a static for
    /// loaders that resolve references outside `get`).
    pub fn load_file_elem(data_folder: &Path, file_path: &str) -> Option<Value> {
        Some(Self::load_file_at(data_folder, file_path))
    }

    fn load_file_at(data_folder: &Path, file_path: &str) -> Value {
        let relative = file_path.replace(JSON_FILEPATH_SEPARATOR, std::path::MAIN_SEPARATOR_STR);
        let file = data_folder.join(format!("{relative}.json"));
        // Prevent path traversal
        let normalized = file.canonical_utf8().unwrap_or_else(|_| file.clone());
        if !normalized.starts_with(
            data_folder
                .canonical_utf8()
                .as_deref()
                .unwrap_or(data_folder),
        ) {
            panic!("Invalid file reference: {file_path}");
        }
        match std::fs::read_to_string(&file) {
            Ok(text) => {
                // Java gson (lenient mode) tolerates a leading UTF-8 BOM
                // (`\uFEFF`); several of the vanilla world files carry one.
                let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
                match serde_json::from_str::<Value>(&strip_json_comments(text)) {
                    Ok(value) => {
                        // Java JSON_LOADER rejects `JsonNull` documents
                        // (`e -> !e.isJsonNull()`); a file containing `null`
                        // fails the load.
                        if value.is_null() {
                            panic!("Error while loading file reference.{}", file.display());
                        }
                        value
                    }
                    Err(e) => panic!("Error while loading file reference.{}: {e}", file.display()),
                }
            }
            Err(e) => panic!("Error while loading file reference.{}: {e}", file.display()),
        }
    }

    // ------------------------------------------------------------------
    // mustGet* family
    // ------------------------------------------------------------------

    /// Java `JsonObject mustGetObject(String key, JsonObject defaultValue)`.
    pub fn must_get_object(&self, key: &str, default_value: Option<Value>) -> Value {
        self.must_get(
            key,
            self.get(key),
            default_value,
            |e| e.is_object(),
            |e| e.clone(),
            "JsonObject",
        )
    }

    /// Java `JsonArray mustGetArray(String key, JsonArray defaultValue)`.
    pub fn must_get_array(&self, key: &str, default_value: Option<Value>) -> Value {
        self.must_get(
            key,
            self.get(key),
            default_value,
            |e| e.is_array(),
            |e| e.clone(),
            "JsonArray",
        )
    }

    /// Java `String mustGetString(String key, String defaultValue)`.
    pub fn must_get_string(&self, key: &str, default_value: Option<&str>) -> String {
        self.must_get(
            key,
            self.get(key),
            default_value.map(|s| s.to_string()),
            |e| e.is_string(),
            |e| e.as_str().unwrap_or_default().to_string(),
            "String",
        )
    }

    /// Java `Boolean mustGetBool(String key, Boolean defaultValue)`.
    pub fn must_get_bool(&self, key: &str, default_value: Option<bool>) -> bool {
        self.must_get(
            key,
            self.get(key),
            default_value,
            |e| e.is_boolean(),
            |e| e.as_bool().unwrap_or_default(),
            "Boolean",
        )
    }

    /// Java `Number mustGetNumber(String key, Number defaultValue)`.
    pub fn must_get_number(&self, key: &str, default_value: Option<f64>) -> f64 {
        self.must_get(
            key,
            self.get(key),
            default_value,
            |e| e.is_number(),
            |e| e.as_f64().unwrap_or_default(),
            "Number",
        )
    }

    /// Java `<V> V mustGet(...)` — the shared implementation.
    #[allow(clippy::too_many_arguments)]
    fn must_get<T>(
        &self,
        key: &str,
        element: Option<Value>,
        default_value: Option<T>,
        predicate: impl Fn(&Value) -> bool,
        mapper: impl Fn(&Value) -> T,
        type_name: &str,
    ) -> T {
        match element {
            None => match default_value {
                Some(default) => default,
                None => panic!("Missing property '{key}'"),
            },
            Some(element) => {
                if !predicate(&element) {
                    panic!("Property '{key}' must be of type '{type_name}'");
                }
                mapper(&element)
            }
        }
    }

    // ------------------------------------------------------------------
    // gson-style primitive accessors (static in Java)
    // ------------------------------------------------------------------

    /// Java `JsonElement.getAsString()` on a primitive.
    pub fn get_as_string(element: &Value) -> String {
        match element {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            other => panic!("Not a string primitive: {other:?}"),
        }
    }

    /// Java `JsonElement.getAsDouble()` on a primitive. gson parses numeric
    /// primitives as doubles (a string primitive like `"0.5"` is also accepted
    /// by gson's lenient path, mirrored here).
    pub fn get_as_double(element: &Value) -> f64 {
        match element {
            Value::Number(n) => n
                .as_f64()
                .unwrap_or_else(|| panic!("Not a double primitive: {n}")),
            Value::String(s) => s
                .parse::<f64>()
                .unwrap_or_else(|_| panic!("Not a double primitive: {s}")),
            other => panic!("Not a double primitive: {other:?}"),
        }
    }

    /// Java `JsonElement.getAsInt()` — gson goes through
    /// `Number.intValue()` i.e. a `(int)` cast of the double value (truncate
    /// toward zero, saturating, `NaN -> 0`), exactly like Rust's `as` cast.
    pub fn get_as_int(element: &Value) -> i32 {
        Self::get_as_double(element) as i32
    }

    /// Java `JsonElement.getAsLong()` — gson goes through
    /// `Number.longValue()` i.e. a `(long)` cast of the double value (truncate
    /// toward zero, saturating, `NaN -> 0`), exactly like Rust's `as` cast.
    pub fn get_as_long(element: &Value) -> i64 {
        Self::get_as_double(element) as i64
    }

    /// Java `JsonElement.getAsBoolean()`.
    pub fn get_as_bool(element: &Value) -> bool {
        match element {
            Value::Bool(b) => *b,
            other => panic!("Not a boolean primitive: {other:?}"),
        }
    }

    /// Java `JsonElement.isNumber()` / `JsonLoader.isNumber`.
    pub fn is_number(element: &Value) -> bool {
        element.is_number()
    }

    /// Java `JsonLoader.isString`.
    pub fn is_string(element: &Value) -> bool {
        element.is_string()
    }

    /// Java `JsonLoader.isBoolean`.
    pub fn is_boolean(element: &Value) -> bool {
        element.is_boolean()
    }
}

/// Java `JsonLoader.EMPTY_OBJECT` — a fresh empty object element.
pub fn empty_object() -> Value {
    serde_json::json!({})
}

/// Java `JsonLoader.EMPTY_ARRAY` — a fresh empty array element.
pub fn empty_array() -> Value {
    serde_json::json!([])
}

trait CanonicalUtf8 {
    fn canonical_utf8(&self) -> std::io::Result<PathBuf>;
}

impl CanonicalUtf8 for Path {
    fn canonical_utf8(&self) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_comments_are_stripped() {
        let text = "{\n  // a comment\n  \"a\": 1\n}";
        let stripped = strip_json_comments(text);
        assert!(!stripped.contains("a comment"));
        let value: Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(value["a"], serde_json::json!(1));
    }

    #[test]
    fn block_comments_are_stripped() {
        let text = "{ /* a\n multi line\n comment */ \"a\": 1 }";
        let stripped = strip_json_comments(text);
        let value: Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(value["a"], serde_json::json!(1));
    }

    #[test]
    fn hash_comments_are_stripped() {
        let text = "{\n # a comment\n \"a\": 1\n}";
        let stripped = strip_json_comments(text);
        let value: Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(value["a"], serde_json::json!(1));
    }

    #[test]
    fn comment_markers_inside_strings_are_kept() {
        let text = "{\"a\": \"http://example.com/#frag\", \"b\": \"/* not a comment */\"}";
        let stripped = strip_json_comments(text);
        let value: Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(value["a"], serde_json::json!("http://example.com/#frag"));
        assert_eq!(value["b"], serde_json::json!("/* not a comment */"));
    }

    #[test]
    fn escaped_quotes_do_not_end_the_string() {
        let text = "{\"a\": \"quote \\\" and // comment\"}";
        let stripped = strip_json_comments(text);
        let value: Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(value["a"], serde_json::json!("quote \" and // comment"));
    }

    #[test]
    fn multi_byte_characters_survive_the_stripper() {
        let text = "{\n // äöü\n \"a\": \"日本語\"\n}";
        let stripped = strip_json_comments(text);
        let value: Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(value["a"], serde_json::json!("日本語"));
    }

    #[test]
    fn the_long_accessor_truncates_like_the_java_cast() {
        assert_eq!(JsonLoaderBase::get_as_long(&serde_json::json!(100.5)), 100);
        assert_eq!(
            JsonLoaderBase::get_as_long(&serde_json::json!(-100.5)),
            -100
        );
        assert_eq!(JsonLoaderBase::get_as_long(&serde_json::json!(7)), 7);
    }
}
