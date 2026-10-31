//! `.lang` translation catalogs.
//!
//! Java's `I18nModule` reads `Server/Languages/<language>/**/*.lang` and sends
//! the merged catalog in `UpdateTranslations`. This module ports the file
//! parser, the `fallback.lang` language mapping and the en-US fallback merge.

use std::collections::BTreeMap;

use crate::pack::{AssetError, AssetPack};

/// Pack-relative language root.
pub const LANGUAGES_ROOT: &str = "Server/Languages";
/// Language used when the requested one has no catalog.
pub const DEFAULT_LANGUAGE: &str = "en-US";

/// Load the catalog the client should receive for `language`.
pub fn load(pack: &AssetPack, language: &str) -> Result<BTreeMap<String, String>, AssetError> {
    let files = pack.list()?;
    let languages: Vec<String> = files
        .iter()
        .filter_map(|path| {
            let rest = path.strip_prefix(LANGUAGES_ROOT)?.strip_prefix('/')?;
            let (language, _) = rest.split_once('/')?;
            (!language.is_empty()).then(|| language.to_string())
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    let fallback = files
        .iter()
        .find(|path| path.as_str() == "Server/Languages/fallback.lang")
        .and_then(|path| pack.read(path).ok())
        .map(|bytes| parse_properties(&String::from_utf8_lossy(&bytes)))
        .unwrap_or_default();

    let resolved = if languages.iter().any(|candidate| candidate == language) {
        language
    } else {
        fallback
            .get(language)
            .filter(|fallback| languages.iter().any(|candidate| candidate == *fallback))
            .map(String::as_str)
            .unwrap_or(DEFAULT_LANGUAGE)
    };

    let mut messages = if resolved == DEFAULT_LANGUAGE {
        BTreeMap::new()
    } else {
        load_language(pack, &files, DEFAULT_LANGUAGE)?
    };
    messages.extend(load_language(pack, &files, resolved)?);
    Ok(messages)
}

fn load_language(
    pack: &AssetPack,
    files: &[String],
    language: &str,
) -> Result<BTreeMap<String, String>, AssetError> {
    let prefix = format!("{LANGUAGES_ROOT}/{language}/");
    let paths: Vec<String> = files
        .iter()
        .filter(|path| path.starts_with(&prefix) && path.ends_with(".lang"))
        .cloned()
        .collect();
    let contents = pack.read_many(&paths)?;
    let mut messages = BTreeMap::new();
    for (path, bytes) in paths.iter().zip(contents) {
        let relative = path.strip_prefix(&prefix).unwrap_or(path);
        let mut parts: Vec<&str> = relative.split('/').collect();
        let file = parts.pop().unwrap_or(relative);
        let stem = file.strip_suffix(".lang").unwrap_or(file);
        parts.push(stem);
        let key_prefix = parts.join(".");
        for (key, value) in parse_lang(&String::from_utf8_lossy(&bytes)) {
            messages
                .entry(format!("{key_prefix}.{key}"))
                .or_insert(value);
        }
    }
    Ok(messages)
}

fn parse_properties(text: &str) -> BTreeMap<String, String> {
    let mut properties = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if !key.is_empty() && !value.is_empty() {
            properties
                .entry(key.to_string())
                .or_insert_with(|| value.to_string());
        }
    }
    properties
}

/// Java `LangFileParser.parse`.
fn parse_lang(text: &str) -> BTreeMap<String, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut messages = BTreeMap::new();
    let mut current_key: Option<String> = None;
    let mut current_value = String::new();

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || (current_key.is_none() && line.starts_with('#')) {
            continue;
        }
        if let Some(key) = current_key.as_mut() {
            let continued = line.ends_with('\\');
            let value = if continued {
                &line[..line.len() - 1]
            } else {
                line
            };
            current_value.push_str(value.trim());
            if !continued {
                messages
                    .entry(std::mem::take(key))
                    .or_insert_with(|| unescape(&current_value));
                current_key = None;
                current_value.clear();
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            continue;
        }
        let continued = value.ends_with('\\');
        let value = if continued {
            &value[..value.len() - 1]
        } else {
            value
        };
        if continued {
            current_key = Some(key.to_string());
            current_value.push_str(value);
        } else {
            messages
                .entry(key.to_string())
                .or_insert_with(|| unescape(literal(value)));
        }
    }

    if let Some(key) = current_key {
        messages
            .entry(key)
            .or_insert_with(|| unescape(&current_value));
    }
    messages
}

fn literal(value: &str) -> &str {
    if value.len() > 1 && value.starts_with('"') && value.ends_with('"') {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

fn unescape(value: &str) -> String {
    value.replace("\\n", "\n").replace("\\t", "\t")
}

#[cfg(test)]
mod tests {
    use super::parse_lang;

    #[test]
    fn parser_handles_comments_quotes_escapes_and_continuations() {
        let parsed = parse_lang(
            "\u{feff}# comment\nkey = value\nquoted = \" spaced \"\nescaped = a\\nb\nmulti = one\\\ntwo\n",
        );
        assert_eq!(parsed.get("key").map(String::as_str), Some("value"));
        assert_eq!(parsed.get("quoted").map(String::as_str), Some(" spaced "));
        assert_eq!(parsed.get("escaped").map(String::as_str), Some("a\nb"));
        assert_eq!(parsed.get("multi").map(String::as_str), Some("onetwo"));
    }
}
