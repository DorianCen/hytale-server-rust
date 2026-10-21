//! Stage 1: deterministic inventory of every generated protocol class.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::java::{bool_const, display_path, int_const, package_name, JavaSource};
use crate::{DumperError, Result};

/// Inventory document written to JSON.
#[derive(Debug, Serialize)]
pub struct Inventory {
    /// Generation metadata and counts.
    pub meta: Meta,
    /// All classes, sorted by package then name.
    pub classes: Vec<ClassEntry>,
}

/// Metadata for the inventory.
#[derive(Debug, Serialize)]
pub struct Meta {
    /// Source directory as given on the command line.
    pub source: String,
    /// Tool name and version.
    pub generator: String,
    /// Counts derived from the source tree.
    pub counts: Counts,
    /// Counts declared by `ProtocolSettings.java`.
    pub expected: Counts,
    /// Human-readable notes for every count mismatch.
    pub discrepancies: Vec<String>,
}

/// Counts by class kind.
#[derive(Debug, Default, Serialize)]
pub struct Counts {
    /// Classes implementing `Packet`.
    pub packets: usize,
    /// Plain data classes with a generated `computeSize`.
    #[serde(rename = "structs")]
    pub structs: usize,
    /// Enum declarations.
    pub enums: usize,
    /// Interfaces and constant/registry helper classes.
    pub other: usize,
    /// Total Java files.
    pub total: usize,
}

/// One class in the inventory.
#[derive(Debug, Serialize)]
pub struct ClassEntry {
    /// Simple class name.
    pub name: String,
    /// Java package.
    pub package: String,
    /// `packet`, `struct`, `enum`, `interface`, or `other`.
    pub kind: String,
    /// Path relative to the inventory source root.
    pub path: String,
    /// Packet metadata, when `kind == "packet"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub packet: Option<PacketEntry>,
}

/// Packet metadata extracted from the constants and interface list.
#[derive(Debug, Serialize)]
pub struct PacketEntry {
    /// Wire packet id (`PACKET_ID`).
    pub id: i64,
    /// `ToServer`, `ToClient`, or `Both`.
    pub direction: String,
    /// `NetworkChannel` variant.
    pub channel: String,
    /// `IS_COMPRESSED`.
    pub compressed: bool,
    /// Nullable bitfield byte count.
    pub nullable_bit_field_size: i64,
    /// Fixed block size, excluding the variable offset slots.
    pub fixed_block_size: i64,
    /// Number of variable fields.
    pub variable_field_count: i64,
    /// Offset of the variable block.
    pub variable_block_start: i64,
    /// Declared maximum encoded size.
    pub max_size: i64,
}

/// Parse the declared counts from `ProtocolSettings.java` at the root, if present.
fn expected_counts(source: &Path) -> Counts {
    let path = source.join("ProtocolSettings.java");
    let Ok(text) = std::fs::read_to_string(path) else {
        return Counts::default();
    };
    Counts {
        packets: int_const(&text, "PACKET_COUNT").unwrap_or(0) as usize,
        structs: int_const(&text, "STRUCT_COUNT").unwrap_or(0) as usize,
        enums: int_const(&text, "ENUM_COUNT").unwrap_or(0) as usize,
        other: 0,
        total: 0,
    }
}

/// Build an inventory over `source`.
pub fn build(source: &Path) -> Result<Inventory> {
    if !source.is_dir() {
        return Err(DumperError::Io {
            path: source.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "not a directory"),
        });
    }

    let mut files = Vec::new();
    walk(source, &mut files)?;
    files.sort();

    let mut classes = Vec::with_capacity(files.len());
    let mut counts = Counts::default();

    for path in files {
        let java = JavaSource::read(&path)?;
        let package = package_name(&java.text).ok_or_else(|| DumperError::Parse {
            path: java.path.clone(),
            line: 1,
            message: "missing package declaration".to_string(),
        })?;
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| DumperError::Parse {
                path: java.path.clone(),
                line: 1,
                message: "file name is not valid UTF-8".to_string(),
            })?
            .to_string();

        let kind = classify(&java.text);
        if kind == "other" && !declares_type(&java.text) {
            return java.err_at("public", "no generated type declaration found");
        }
        if kind == "other"
            && java.text.contains("public class ")
            && !java.text.contains("public final class ")
        {
            return java.err_at(
                "public class",
                "non-final helper class is not a generated wire struct",
            );
        }
        counts.total += 1;
        let packet = if kind == "packet" {
            counts.packets += 1;
            Some(parse_packet(&java)?)
        } else {
            match kind {
                "enum" => counts.enums += 1,
                "struct" => counts.structs += 1,
                _ => counts.other += 1,
            }
            None
        };

        let relative = path
            .strip_prefix(source)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        classes.push(ClassEntry {
            name,
            package: package.to_string(),
            kind: kind.to_string(),
            path: relative,
            packet,
        });
    }

    classes.sort_by(|a, b| a.package.cmp(&b.package).then_with(|| a.name.cmp(&b.name)));

    let expected = expected_counts(source);
    let mut discrepancies = Vec::new();
    note(
        &mut discrepancies,
        "packets",
        counts.packets,
        expected.packets,
    );
    note(
        &mut discrepancies,
        "structs",
        counts.structs,
        expected.structs,
    );
    note(&mut discrepancies, "enums", counts.enums, expected.enums);
    if !discrepancies.is_empty() {
        discrepancies.push(
            "Struct/enum deltas are source-shape notes, not silent skips: `other` lists the \
             interfaces and constant/registry helpers that are not wire structs."
                .to_string(),
        );
    }

    Ok(Inventory {
        meta: Meta {
            source: display_path(source),
            generator: format!("protocol-dumper {}", env!("CARGO_PKG_VERSION")),
            counts,
            expected,
            discrepancies,
        },
        classes,
    })
}

fn note(discrepancies: &mut Vec<String>, name: &str, actual: usize, expected: usize) {
    if actual != expected && expected != 0 {
        discrepancies.push(format!(
            "{name}: parsed {actual}, ProtocolSettings declares {expected}"
        ));
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(dir).map_err(|source| DumperError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| DumperError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("java") {
            out.push(path);
        }
    }
    Ok(())
}

fn declares_type(text: &str) -> bool {
    top_level_declaration(text).is_some()
}

/// Return the first top-level type declaration keyword.
///
/// A naive `text.contains("public enum ")` misclassifies `PacketRegistry.java`,
/// which declares a nested `PacketDirection` enum inside a final class.
fn top_level_declaration(text: &str) -> Option<&'static str> {
    let declarations = [
        ("public enum ", "enum"),
        ("public interface ", "interface"),
        ("public final class ", "class"),
        ("public abstract class ", "class"),
        ("public class ", "class"),
    ];
    declarations
        .iter()
        .filter_map(|(needle, kind)| text.find(needle).map(|position| (position, *kind)))
        .min_by_key(|(position, _)| *position)
        .map(|(_, kind)| kind)
}

fn classify(text: &str) -> &'static str {
    match top_level_declaration(text) {
        Some("enum") => "enum",
        Some("interface") => "interface",
        Some("class") if has_packet_interface(text) => "packet",
        Some("class") if text.contains("public int computeSize()") => "struct",
        Some("class") => "other",
        _ => "other",
    }
}

fn has_packet_interface(text: &str) -> bool {
    let Some(start) = text.find("implements ") else {
        return false;
    };
    let clause = &text[start + "implements ".len()..];
    let end = clause.find('{').unwrap_or(clause.len());
    clause[..end]
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|part| part == "Packet")
}

fn parse_packet(java: &JavaSource) -> Result<PacketEntry> {
    let text = &java.text;
    let id = int_const(text, "PACKET_ID")
        .ok_or_else(|| parse_error(java, "missing PACKET_ID constant"))?;
    let direction = direction(text);
    let channel = channel(text)
        .ok_or_else(|| parse_error(java, "missing getChannel() NetworkChannel value"))?;

    Ok(PacketEntry {
        id,
        direction,
        channel,
        compressed: bool_const(text, "IS_COMPRESSED")
            .ok_or_else(|| parse_error(java, "missing IS_COMPRESSED constant"))?,
        nullable_bit_field_size: int_const(text, "NULLABLE_BIT_FIELD_SIZE")
            .ok_or_else(|| parse_error(java, "missing NULLABLE_BIT_FIELD_SIZE"))?,
        fixed_block_size: int_const(text, "FIXED_BLOCK_SIZE")
            .ok_or_else(|| parse_error(java, "missing FIXED_BLOCK_SIZE"))?,
        variable_field_count: int_const(text, "VARIABLE_FIELD_COUNT")
            .ok_or_else(|| parse_error(java, "missing VARIABLE_FIELD_COUNT"))?,
        variable_block_start: int_const(text, "VARIABLE_BLOCK_START")
            .ok_or_else(|| parse_error(java, "missing VARIABLE_BLOCK_START"))?,
        max_size: int_const(text, "MAX_SIZE")
            .ok_or_else(|| parse_error(java, "missing MAX_SIZE"))?,
    })
}

fn parse_error(java: &JavaSource, message: &str) -> DumperError {
    DumperError::Parse {
        path: java.path.clone(),
        line: 1,
        message: message.to_string(),
    }
}

fn direction(text: &str) -> String {
    let start = text.find("implements ").map(|i| i + "implements ".len());
    let clause = start
        .map(|start| {
            let rest = &text[start..];
            let end = rest.find('{').unwrap_or(rest.len());
            &rest[..end]
        })
        .unwrap_or("");
    let to_server = clause
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|part| part == "ToServerPacket");
    let to_client = clause
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|part| part == "ToClientPacket");
    match (to_server, to_client) {
        (true, true) => "Both".to_string(),
        (true, false) => "ToServer".to_string(),
        (false, true) => "ToClient".to_string(),
        (false, false) => "None".to_string(),
    }
}

fn channel(text: &str) -> Option<String> {
    let start = text.find("getChannel()")?;
    let rest = &text[start..];
    let marker = "return NetworkChannel.";
    let value = rest.find(marker)? + marker.len();
    let rest = &rest[value..];
    let end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    Some(rest[..end].to_string())
}
