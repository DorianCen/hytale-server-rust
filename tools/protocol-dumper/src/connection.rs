//! Stage 2: field-level IR for the `packets/connection` family.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::java::{
    bool_const, display_path, int_const, method_body, package_name, split_type_and_name, JavaSource,
};
use crate::{DumperError, Result};

/// Field-level IR for a packet family or the full protocol.
#[derive(Debug, Serialize)]
pub struct ConnectionIr {
    /// Generation metadata.
    pub meta: Meta,
    /// `ProtocolSettings` constants, present in full-protocol mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<ProtocolSettingsIr>,
    /// Enums declared in the connection package or referenced by a packet.
    pub enums: Vec<EnumIr>,
    /// Nested structs/unions needed by the packets, sorted by name.
    pub structs: Vec<StructIr>,
    /// Packets, sorted by packet id.
    pub packets: Vec<PacketIr>,
}

/// Constants declared by `ProtocolSettings.java`.
#[derive(Debug, Serialize)]
pub struct ProtocolSettingsIr {
    /// Protocol CRC expected in `Connect`.
    pub crc: i64,
    /// Protocol version.
    pub version: i64,
    /// Protocol build number.
    pub build_number: i64,
    /// Declared packet count.
    pub packet_count: i64,
    /// Declared struct count.
    pub struct_count: i64,
    /// Declared enum count.
    pub enum_count: i64,
    /// Maximum packet payload size.
    pub max_packet_size: i64,
}

/// One nested struct or union referenced by a connection packet.
#[derive(Debug, Serialize)]
pub struct StructIr {
    /// Struct name.
    pub name: String,
    /// Java package.
    pub package: String,
    /// `NULLABLE_BIT_FIELD_SIZE`.
    pub nullable_bit_field_size: i64,
    /// `FIXED_BLOCK_SIZE`.
    pub fixed_block_size: i64,
    /// `VARIABLE_FIELD_COUNT`.
    pub variable_field_count: i64,
    /// `VARIABLE_BLOCK_START`.
    pub variable_block_start: i64,
    /// `MAX_SIZE`.
    pub max_size: i64,
    /// Fields in wire order.
    pub fields: Vec<FieldIr>,
    /// Union metadata for an abstract polymorphic type.
    pub union: Option<UnionIr>,
}

/// A polymorphic union and its type-id-tagged subtypes.
#[derive(Debug, Serialize)]
pub struct UnionIr {
    /// Abstract Java type name.
    pub name: String,
    /// Subtypes in type-id order.
    pub subtypes: Vec<UnionSubtype>,
}

/// One union subtype.
#[derive(Debug, Serialize)]
pub struct UnionSubtype {
    /// Varint type id written before the subtype body.
    pub type_id: i64,
    /// Concrete struct name.
    pub name: String,
}

/// IR metadata.
#[derive(Debug, Serialize)]
pub struct Meta {
    /// Source directory.
    pub source: String,
    /// Tool name and version.
    pub generator: String,
    /// Number of packets and enums.
    pub counts: Counts,
}

/// IR counts.
#[derive(Debug, Serialize)]
pub struct Counts {
    /// Packet count.
    pub packets: usize,
    /// Enum count.
    pub enums: usize,
}

/// One generated enum.
#[derive(Debug, Serialize)]
pub struct EnumIr {
    /// Enum name.
    pub name: String,
    /// Java package.
    pub package: String,
    /// Values in declaration order.
    pub values: Vec<EnumValue>,
}

/// One enum member.
#[derive(Debug, Serialize)]
pub struct EnumValue {
    /// Member name.
    pub name: String,
    /// Explicit wire value from the generated constructor.
    pub value: i64,
}

/// One packet in the connection family.
#[derive(Debug, Serialize)]
pub struct PacketIr {
    /// Class name.
    pub name: String,
    /// Java package.
    pub package: String,
    /// `PACKET_ID`.
    pub packet_id: i64,
    /// `ToServer`, `ToClient`, or `Both`.
    pub direction: String,
    /// `NetworkChannel` variant.
    pub channel: String,
    /// `IS_COMPRESSED`.
    pub compressed: bool,
    /// `NULLABLE_BIT_FIELD_SIZE`.
    pub nullable_bit_field_size: i64,
    /// `FIXED_BLOCK_SIZE`.
    pub fixed_block_size: i64,
    /// `VARIABLE_FIELD_COUNT`.
    pub variable_field_count: i64,
    /// `VARIABLE_BLOCK_START`.
    pub variable_block_start: i64,
    /// `MAX_SIZE`.
    pub max_size: i64,
    /// Fields in wire order.
    pub fields: Vec<FieldIr>,
}

/// One packet field.
#[derive(Debug, Serialize)]
pub struct FieldIr {
    /// Field name.
    pub name: String,
    /// Declared Java type, normalised.
    pub field_type: String,
    /// Wire kind: primitive, enum, struct, string, array, uuid, or map.
    pub kind: String,
    /// Whether the field is `@Nullable`.
    pub nullable: bool,
    /// Nullable bit index, when nullable.
    pub nullable_bit: Option<u32>,
    /// Byte containing the nullable bit, when nullable.
    pub nullable_byte: Option<u32>,
    /// Offset inside the fixed block, when fixed.
    pub fixed_offset: Option<i64>,
    /// Size of a fixed field, when known.
    pub fixed_size: Option<i64>,
    /// Declaration order for variable fields.
    pub variable_order: Option<i64>,
    /// Fixed-block offset slot holding this variable field's offset.
    pub variable_offset_slot: Option<i64>,
    /// String encoding and bounds, when the field is a string.
    pub string: Option<StringInfo>,
    /// Array element type and bounds, when the field is an array.
    pub array: Option<ArrayInfo>,
    /// Map key/value types and bounds, when the field is a map.
    pub map: Option<MapInfo>,
    /// Enum type, when the field is an enum.
    pub enum_type: Option<String>,
    /// Nested struct type, when the field is a struct.
    pub nested_type: Option<String>,
    /// Packed boolean bit location, when the field is bit-packed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bool_packed: Option<BoolPacked>,
}

/// A boolean packed into a shared byte with other boolean fields.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct BoolPacked {
    /// Byte offset inside the fixed block.
    pub byte_offset: i64,
    /// Bit index inside that byte (0-7).
    pub bit: u8,
}

/// String wire metadata.
#[derive(Debug, Clone, Serialize)]
pub struct StringInfo {
    /// `ASCII` or `UTF-8`.
    pub encoding: String,
    /// `fixed` or `varint`.
    pub length_prefix: String,
    /// Fixed length, for fixed strings.
    pub fixed_length: Option<i64>,
    /// Maximum byte length, for variable strings.
    pub max_length: Option<i64>,
}

/// Array wire metadata.
#[derive(Debug, Serialize)]
pub struct ArrayInfo {
    /// Declared element type.
    pub element_type: String,
    /// Maximum element count.
    pub max_length: Option<i64>,
}

/// Map wire metadata.
#[derive(Debug, Serialize)]
pub struct MapInfo {
    /// Declared key type (always `String` in the generated protocol).
    pub key_type: String,
    /// Declared value type.
    pub value_type: String,
    /// Maximum entry count.
    pub max_length: Option<i64>,
}

#[derive(Debug, Clone)]
struct RawField {
    name: String,
    field_type: String,
    nullable: bool,
    line: usize,
}

#[derive(Debug, Default, Clone)]
struct WireInfo {
    fixed_offset: Option<i64>,
    fixed_size: Option<i64>,
    variable_order: Option<i64>,
    variable_offset_slot: Option<i64>,
    string: Option<StringInfo>,
    array_max: Option<i64>,
    bool_packed: Option<BoolPacked>,
}

/// Build the connection IR over `source`.
pub fn build(source: &Path) -> Result<ConnectionIr> {
    if !source.is_dir() {
        return Err(DumperError::Io {
            path: source.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "not a directory"),
        });
    }

    let mut files = Vec::new();
    walk(source, &mut files)?;
    files.sort();

    let mut sources = Vec::new();
    let mut enum_names = Vec::new();
    for path in &files {
        let java = JavaSource::read(path)?;
        if java.text.contains("public enum ") {
            enum_names.push(
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or_default()
                    .to_string(),
            );
        }
        sources.push(java);
    }

    let mut enums = Vec::new();
    let mut packets = Vec::new();
    for java in &sources {
        if java.text.contains("public enum ") {
            enums.push(parse_enum(java)?);
        } else if java.text.contains("implements Packet") {
            packets.push(parse_packet(java, &enum_names)?);
        } else {
            return java.err_at("public", "file is neither a packet nor an enum");
        }
    }

    enums.sort_by(|a, b| a.name.cmp(&b.name));
    packets.sort_by_key(|packet| packet.packet_id);

    validate(&sources, &packets)?;

    let structs = discover_structs(source, &packets, &mut enums)?;
    enums.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(ConnectionIr {
        meta: Meta {
            source: display_path(source),
            generator: format!("protocol-dumper {}", env!("CARGO_PKG_VERSION")),
            counts: Counts {
                packets: packets.len(),
                enums: enums.len(),
            },
        },
        settings: None,
        enums,
        structs,
        packets,
    })
}

/// Build the field-level IR for every generated protocol class.
///
/// Unlike [`build`], which parses one packet family, this walks the full
/// `com/hypixel/hytale/protocol` tree and emits every packet, struct, union and
/// enum. The generated Java sources are classified by their declaration shape;
/// helper classes (`PacketRegistry`, `ProtocolSettings`, flag holders) are not
/// wire types and are skipped.
pub fn build_all(source: &Path) -> Result<ConnectionIr> {
    if !source.is_dir() {
        return Err(DumperError::Io {
            path: source.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "not a directory"),
        });
    }

    let mut files = Vec::new();
    walk(source, &mut files)?;
    files.sort();

    let mut sources = Vec::with_capacity(files.len());
    for path in &files {
        sources.push(JavaSource::read(path)?);
    }

    let index: BTreeMap<String, PathBuf> = files
        .iter()
        .filter_map(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(|stem| (stem.to_string(), path.clone()))
        })
        .collect();

    let enum_names: Vec<String> = sources
        .iter()
        .filter(|java| is_enum_source(&java.text))
        .filter_map(stem)
        .collect();

    let mut enums = Vec::new();
    let mut structs = Vec::new();
    let mut packets = Vec::new();
    let mut skipped = Vec::new();

    for java in &sources {
        if is_enum_source(&java.text) {
            enums.push(parse_enum(java)?);
            continue;
        }
        let Some(name) = stem(java) else {
            continue;
        };
        if has_packet_interface(&java.text) {
            packets.push(parse_packet(java, &enum_names)?);
        } else if java.text.contains("public abstract class ") {
            let mut pending = std::collections::BTreeSet::new();
            structs.push(parse_union(java, &index, &mut pending)?);
        } else if java.text.contains("public int computeSize()") {
            structs.push(parse_struct(java, &enum_names, &index)?);
        } else {
            skipped.push(name);
        }
    }

    enums.sort_by(|a, b| a.name.cmp(&b.name));
    structs.sort_by(|a, b| a.name.cmp(&b.name));

    // A packet class can also appear as a nested wire type (for example
    // `InteractionEffects.cameraShake` references the `CameraShakeEffect`
    // packet). Resolve those references to their generated field layout.
    resolve_missing_structs(&mut structs, &enums, &index)?;

    packets.sort_by_key(|packet| packet.packet_id);
    structs.sort_by(|a, b| a.name.cmp(&b.name));

    validate_full(&sources, &packets, &enums, &structs)?;

    let settings = parse_settings(source);
    Ok(ConnectionIr {
        meta: Meta {
            source: display_path(source),
            generator: format!("protocol-dumper {}", env!("CARGO_PKG_VERSION")),
            counts: Counts {
                packets: packets.len(),
                enums: enums.len(),
            },
        },
        settings,
        enums,
        structs,
        packets,
    })
}

fn stem(java: &JavaSource) -> Option<String> {
    java.path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_string)
}

fn is_enum_source(text: &str) -> bool {
    top_level_declaration_kind(text) == Some("enum")
}

/// First top-level type declaration keyword.
fn top_level_declaration_kind(text: &str) -> Option<&'static str> {
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

fn parse_settings(source: &Path) -> Option<ProtocolSettingsIr> {
    let text = std::fs::read_to_string(source.join("ProtocolSettings.java")).ok()?;
    Some(ProtocolSettingsIr {
        crc: int_const(&text, "PROTOCOL_CRC")?,
        version: int_const(&text, "PROTOCOL_VERSION")?,
        build_number: int_const(&text, "PROTOCOL_BUILD_NUMBER")?,
        packet_count: int_const(&text, "PACKET_COUNT")?,
        struct_count: int_const(&text, "STRUCT_COUNT")?,
        enum_count: int_const(&text, "ENUM_COUNT")?,
        max_packet_size: int_const(&text, "MAX_PACKET_SIZE")?,
    })
}

fn resolve_missing_structs(
    structs: &mut Vec<StructIr>,
    enums: &[EnumIr],
    index: &BTreeMap<String, PathBuf>,
) -> Result<()> {
    loop {
        let mut known: std::collections::BTreeSet<String> =
            structs.iter().map(|value| value.name.clone()).collect();
        known.extend(enums.iter().map(|value| value.name.clone()));
        let mut missing = std::collections::BTreeSet::new();
        for structure in structs.iter() {
            for field in &structure.fields {
                if field.kind != "struct" {
                    continue;
                }
                if let Some(nested) = &field.nested_type {
                    if !known.contains(nested) && is_user_type(nested) {
                        missing.insert(nested.clone());
                    }
                }
            }
        }
        if missing.is_empty() {
            return Ok(());
        }
        for name in missing {
            let Some(path) = index.get(&name) else {
                return Err(DumperError::Parse {
                    path: PathBuf::new(),
                    line: 0,
                    message: format!("referenced type {name} has no generated source file"),
                });
            };
            let java = JavaSource::read(path)?;
            let enum_names: Vec<String> = enums.iter().map(|value| value.name.clone()).collect();
            let parsed = if java.text.contains("public abstract class ") {
                let mut pending = std::collections::BTreeSet::new();
                parse_union(&java, index, &mut pending)?
            } else {
                parse_struct(&java, &enum_names, index)?
            };
            structs.push(parsed);
        }
    }
}

fn validate_full(
    sources: &[JavaSource],
    packets: &[PacketIr],
    enums: &[EnumIr],
    structs: &[StructIr],
) -> Result<()> {
    let mut errors = Vec::new();

    let mut ids = std::collections::BTreeSet::new();
    for packet in packets {
        if !ids.insert(packet.packet_id) {
            errors.push(format!(
                "duplicate packet id {} ({})",
                packet.packet_id, packet.name
            ));
        }
        if packet.direction == "None" {
            errors.push(format!("packet {} has no direction marker", packet.name));
        }
    }

    let mut names = std::collections::BTreeSet::new();
    for structure in structs {
        if !names.insert(structure.name.as_str()) {
            errors.push(format!("duplicate struct name {}", structure.name));
        }
        if structure.union.is_some() {
            continue;
        }
        for field in &structure.fields {
            if field.fixed_offset.is_none() && field.variable_order.is_none() {
                errors.push(format!(
                    "{}.{} has no fixed offset or variable order",
                    structure.name, field.name
                ));
            }
        }
    }
    for enumeration in enums {
        if !names.insert(enumeration.name.as_str()) {
            errors.push(format!("duplicate type name {}", enumeration.name));
        }
    }
    for packet in packets {
        for field in &packet.fields {
            if field.fixed_offset.is_none() && field.variable_order.is_none() {
                errors.push(format!(
                    "{}.{} has no fixed offset or variable order",
                    packet.name, field.name
                ));
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(DumperError::Parse {
            path: sources
                .first()
                .map(|source| source.path.clone())
                .unwrap_or_default(),
            line: 0,
            message: format!(
                "full protocol validation failed ({} errors): {}",
                errors.len(),
                errors.join("; ")
            ),
        })
    }
}

/// Resolve the struct/union/enum types reachable from the packets.
///
/// The packet IR only names nested types; the generator needs their field-level
/// layout. Starting at the packet fields and walking the references keeps the
/// output bounded to the connection closure instead of the whole protocol.
fn discover_structs(
    source: &Path,
    packets: &[PacketIr],
    enums: &mut Vec<EnumIr>,
) -> Result<Vec<StructIr>> {
    let root = source
        .parent()
        .and_then(Path::parent)
        .unwrap_or(source)
        .to_path_buf();
    let mut files = Vec::new();
    walk(&root, &mut files)?;
    files.sort();
    let index: BTreeMap<String, PathBuf> = files
        .iter()
        .filter_map(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(|stem| (stem.to_string(), path.clone()))
        })
        .collect();

    let enum_names: Vec<String> = enums.iter().map(|value| value.name.clone()).collect();
    let mut pending: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for packet in packets {
        collect_references(&packet.fields, &mut pending);
    }

    let mut structs = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    while let Some(name) = pending.pop_first() {
        if !seen.insert(name.clone()) {
            continue;
        }
        if enum_names.iter().any(|existing| existing == &name) {
            continue;
        }
        let Some(path) = index.get(&name) else {
            return Err(DumperError::Parse {
                path: root.clone(),
                line: 0,
                message: format!("referenced type {name} has no generated source file"),
            });
        };
        let java = JavaSource::read(path)?;
        if java.text.contains("public enum ") {
            enums.push(parse_enum(&java)?);
            continue;
        }
        let parsed = if java.text.contains("public abstract class ") {
            parse_union(&java, &index, &mut pending)?
        } else {
            parse_struct(&java, &enum_names, &index)?
        };
        collect_references(&parsed.fields, &mut pending);
        structs.push(parsed);
    }

    structs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(structs)
}

fn collect_references(fields: &[FieldIr], pending: &mut std::collections::BTreeSet<String>) {
    for field in fields {
        if let Some(nested) = &field.nested_type {
            if is_user_type(nested) {
                pending.insert(nested.clone());
            }
        }
        if let Some(array) = &field.array {
            if is_user_type(&array.element_type) {
                pending.insert(array.element_type.clone());
            }
        }
        if let Some(map) = &field.map {
            if is_user_type(&map.value_type) {
                pending.insert(map.value_type.clone());
            }
        }
    }
}

fn is_user_type(name: &str) -> bool {
    !matches!(
        name,
        "byte"
            | "short"
            | "int"
            | "long"
            | "float"
            | "double"
            | "boolean"
            | "Boolean"
            | "Integer"
            | "Float"
            | "String"
            | "UUID"
    ) && vector_kind(name).is_none()
        && !name.ends_with("[]")
        && !name.starts_with("Map<")
        && !name.starts_with("java.util.Map<")
}

fn collect_fields(java: &JavaSource, index: &BTreeMap<String, PathBuf>) -> Result<Vec<RawField>> {
    let mut chain = Vec::new();
    let mut current = java.clone();
    let mut seen = std::collections::BTreeSet::new();
    loop {
        let name = current
            .path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_string();
        if !seen.insert(name.clone()) {
            return Err(parse_error(
                &current,
                1,
                format!("inheritance cycle through {name}"),
            ));
        }
        let parent = parent_name(&current.text);
        chain.push(current);
        let Some(parent) = parent else {
            break;
        };
        let Some(path) = index.get(&parent) else {
            return Err(parse_error(
                java,
                1,
                format!("{name} extends {parent}, which has no generated source file"),
            ));
        };
        current = JavaSource::read(path)?;
    }
    chain.reverse();

    let mut fields = Vec::new();
    for source in chain {
        fields.extend(parse_fields(&source)?);
    }
    Ok(fields)
}

/// Top-level `extends` target, if any.
fn parent_name(text: &str) -> Option<String> {
    let class = [
        "public class ",
        "public final class ",
        "public abstract class ",
    ]
    .iter()
    .filter_map(|needle| text.find(needle))
    .min()?;
    let rest = &text[class..];
    let end = rest.find('{').unwrap_or(rest.len());
    let declaration = &rest[..end];
    let start = declaration.find(" extends ")? + " extends ".len();
    let name: String = declaration[start..]
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

fn parse_struct(
    java: &JavaSource,
    enum_names: &[String],
    index: &BTreeMap<String, PathBuf>,
) -> Result<StructIr> {
    let package = package_name(&java.text)
        .ok_or_else(|| parse_error(java, 1, "missing package declaration"))?
        .to_string();
    let name = java
        .path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_string();
    let required_int = |constant: &str| -> Result<i64> {
        int_const(&java.text, constant)
            .ok_or_else(|| parse_error(java, 1, format!("missing {constant} constant")))
    };
    let fixed_block_size = required_int("FIXED_BLOCK_SIZE")?;
    let raw_fields = collect_fields(java, index)?;
    let nullable_bits = parse_nullable_bits(java, &raw_fields)?;
    let serialize = method_body(&java.text, "public int serialize(")
        .ok_or_else(|| parse_error(java, 1, "missing serialize() method"))?;
    let wire = parse_wire(java, serialize, &raw_fields)?;
    let fields = build_fields(
        java,
        raw_fields,
        &nullable_bits,
        &wire,
        enum_names,
        fixed_block_size,
    )?;

    Ok(StructIr {
        name,
        package,
        nullable_bit_field_size: required_int("NULLABLE_BIT_FIELD_SIZE")?,
        fixed_block_size,
        variable_field_count: required_int("VARIABLE_FIELD_COUNT")?,
        variable_block_start: required_int("VARIABLE_BLOCK_START")?,
        max_size: required_int("MAX_SIZE")?,
        fields,
        union: None,
    })
}

fn parse_union(
    java: &JavaSource,
    index: &BTreeMap<String, PathBuf>,
    pending: &mut std::collections::BTreeSet<String>,
) -> Result<StructIr> {
    let package = package_name(&java.text)
        .ok_or_else(|| parse_error(java, 1, "missing package declaration"))?
        .to_string();
    let name = java
        .path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_string();
    let max_size = int_const(&java.text, "MAX_SIZE").unwrap_or(0);

    let mut subtypes = Vec::new();
    let mut cursor = 0usize;
    while let Some(found) = java.text[cursor..].find("case ") {
        let start = cursor + found + "case ".len();
        let digits: String = java.text[start..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if digits.is_empty() {
            cursor = start;
            continue;
        }
        let after = &java.text[start + digits.len()..];
        let Some(after_arrow) = after.strip_prefix(" -> ") else {
            cursor = start;
            continue;
        };
        let type_name: String = after_arrow
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !type_name.is_empty() {
            let type_id = digits.parse::<i64>().map_err(|_| {
                parse_error(
                    java,
                    1,
                    format!("union subtype id is not an integer: {digits}"),
                )
            })?;
            if !subtypes
                .iter()
                .any(|existing: &UnionSubtype| existing.name == type_name)
            {
                if index.contains_key(&type_name) {
                    pending.insert(type_name.clone());
                }
                subtypes.push(UnionSubtype {
                    type_id,
                    name: type_name,
                });
            }
        }
        cursor = start + digits.len();
    }
    if subtypes.is_empty() {
        return Err(parse_error(
            java,
            1,
            "abstract class has no case <id> -> <Subtype> dispatch",
        ));
    }
    subtypes.sort_by_key(|subtype| subtype.type_id);

    Ok(StructIr {
        name: name.clone(),
        package,
        nullable_bit_field_size: 0,
        fixed_block_size: 0,
        variable_field_count: 0,
        variable_block_start: 0,
        max_size,
        fields: Vec::new(),
        union: Some(UnionIr { name, subtypes }),
    })
}

fn build_fields(
    java: &JavaSource,
    raw_fields: Vec<RawField>,
    nullable_bits: &BTreeMap<String, (u32, u32)>,
    wire: &BTreeMap<String, WireInfo>,
    enum_names: &[String],
    fixed_block_size: i64,
) -> Result<Vec<FieldIr>> {
    let mut fields = Vec::with_capacity(raw_fields.len());
    for raw in raw_fields {
        let info = wire.get(&raw.name).cloned().unwrap_or_default();
        let (kind, enum_type, nested_type) = classify_type(&raw.field_type, enum_names, &info);
        let string = info.string.clone();
        let array = raw
            .field_type
            .strip_suffix("[]")
            .map(|element_type| ArrayInfo {
                element_type: normalise_type(element_type),
                max_length: info.array_max,
            });
        let map = map_types(&raw.field_type).map(|(key_type, value_type)| MapInfo {
            key_type: normalise_type(&key_type),
            value_type: normalise_type(&value_type),
            max_length: info.array_max.or_else(|| dictionary_limit(java, &raw.name)),
        });
        let (nullable_bit, nullable_byte) = match nullable_bits.get(&raw.name) {
            Some((byte, bit)) => (Some(*bit), Some(*byte)),
            None => (None, None),
        };
        if raw.nullable && nullable_bit.is_none() {
            return Err(parse_error(
                java,
                raw.line,
                format!(
                    "nullable field {} has no generated has{}() method",
                    raw.name,
                    upper_first(&raw.name)
                ),
            ));
        }
        fields.push(FieldIr {
            name: raw.name,
            field_type: raw.field_type,
            kind,
            nullable: raw.nullable,
            nullable_bit,
            nullable_byte,
            fixed_offset: if info.bool_packed.is_some() {
                info.bool_packed.map(|packed| packed.byte_offset)
            } else {
                info.fixed_offset
            },
            fixed_size: if info.bool_packed.is_some() {
                Some(1)
            } else {
                info.fixed_size
            },
            variable_order: info.variable_order,
            variable_offset_slot: info.variable_offset_slot,
            string,
            array,
            map,
            enum_type,
            nested_type,
            bool_packed: info.bool_packed,
        });
    }

    let mut offsets: Vec<(i64, usize)> = fields
        .iter()
        .enumerate()
        .filter_map(|(index, field)| field.fixed_offset.map(|offset| (offset, index)))
        .collect();
    offsets.sort_unstable();
    for (position, (offset, index)) in offsets.iter().enumerate() {
        if fields[*index].fixed_size.is_some() || fields[*index].bool_packed.is_some() {
            continue;
        }
        let end = offsets
            .get(position + 1)
            .map(|(next, _)| *next)
            .unwrap_or(fixed_block_size);
        fields[*index].fixed_size = Some(end - offset);
    }
    Ok(fields)
}

/// Read the entry bound from a generated `dictionaryTooLong("Field", len, MAX)`
/// guard. Map bounds appear in `toObject`, not in `serialize`.
fn dictionary_limit(java: &JavaSource, field: &str) -> Option<i64> {
    let marker = format!("dictionaryTooLarge(\"{}\"", upper_first(field));
    let start = java.text.find(&marker)?;
    let rest = &java.text[start + marker.len()..];
    let end = rest.find(')')?;
    rest[..end]
        .split(',')
        .next_back()
        .and_then(|tail| tail.trim().parse().ok())
}

fn parse_error(java: &JavaSource, line: usize, message: impl Into<String>) -> DumperError {
    DumperError::Parse {
        path: java.path.clone(),
        line,
        message: message.into(),
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

fn parse_enum(java: &JavaSource) -> Result<EnumIr> {
    let text = &java.text;
    let package = package_name(text).ok_or_else(|| DumperError::Parse {
        path: java.path.clone(),
        line: 1,
        message: "missing package declaration".to_string(),
    })?;
    let keyword = "public enum ";
    let start = text.find(keyword).ok_or_else(|| DumperError::Parse {
        path: java.path.clone(),
        line: 1,
        message: "missing enum declaration".to_string(),
    })? + keyword.len();
    let rest = &text[start..];
    let name_end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    let name = rest[..name_end].to_string();
    let body_start = rest.find('{').ok_or_else(|| DumperError::Parse {
        path: java.path.clone(),
        line: java.line_of(start),
        message: "enum without body".to_string(),
    })?;
    let body = &rest[body_start + 1..];
    let body_end = body.find(';').ok_or_else(|| DumperError::Parse {
        path: java.path.clone(),
        line: java.line_of(start),
        message: "enum values are not terminated by ';'".to_string(),
    })?;

    let mut values = Vec::new();
    for entry in body[..body_end].split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let open = entry.find('(').ok_or_else(|| DumperError::Parse {
            path: java.path.clone(),
            line: java.line_of(start),
            message: format!("enum value without explicit number: {entry}"),
        })?;
        let close = entry.find(')').ok_or_else(|| DumperError::Parse {
            path: java.path.clone(),
            line: java.line_of(start),
            message: format!("enum value without closing parenthesis: {entry}"),
        })?;
        let value =
            entry[open + 1..close]
                .trim()
                .parse::<i64>()
                .map_err(|_| DumperError::Parse {
                    path: java.path.clone(),
                    line: java.line_of(start),
                    message: format!("enum value is not an integer: {entry}"),
                })?;
        values.push(EnumValue {
            name: entry[..open].trim().to_string(),
            value,
        });
    }

    Ok(EnumIr {
        name,
        package: package.to_string(),
        values,
    })
}

fn parse_packet(java: &JavaSource, enum_names: &[String]) -> Result<PacketIr> {
    let text = &java.text;
    let package = package_name(text).ok_or_else(|| DumperError::Parse {
        path: java.path.clone(),
        line: 1,
        message: "missing package declaration".to_string(),
    })?;
    let name = java
        .path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_string();

    let required_int = |constant: &str| -> Result<i64> {
        int_const(text, constant).ok_or_else(|| DumperError::Parse {
            path: java.path.clone(),
            line: 1,
            message: format!("missing {constant} constant"),
        })
    };

    let packet_id = required_int("PACKET_ID")?;
    let fixed_block_size = required_int("FIXED_BLOCK_SIZE")?;
    let variable_field_count = required_int("VARIABLE_FIELD_COUNT")?;

    let raw_fields = parse_fields(java)?;
    let nullable_bits = parse_nullable_bits(java, &raw_fields)?;
    let serialize =
        method_body(text, "public int serialize(").ok_or_else(|| DumperError::Parse {
            path: java.path.clone(),
            line: 1,
            message: "missing serialize() method".to_string(),
        })?;
    let wire = parse_wire(java, serialize, &raw_fields)?;
    let fields = build_fields(
        java,
        raw_fields,
        &nullable_bits,
        &wire,
        enum_names,
        fixed_block_size,
    )?;

    Ok(PacketIr {
        name,
        package: package.to_string(),
        packet_id,
        direction: direction(text),
        channel: channel(text).ok_or_else(|| DumperError::Parse {
            path: java.path.clone(),
            line: 1,
            message: "missing getChannel() NetworkChannel value".to_string(),
        })?,
        compressed: bool_const(text, "IS_COMPRESSED").ok_or_else(|| DumperError::Parse {
            path: java.path.clone(),
            line: 1,
            message: "missing IS_COMPRESSED".to_string(),
        })?,
        nullable_bit_field_size: required_int("NULLABLE_BIT_FIELD_SIZE")?,
        fixed_block_size,
        variable_field_count,
        variable_block_start: required_int("VARIABLE_BLOCK_START")?,
        max_size: required_int("MAX_SIZE")?,
        fields,
    })
}

fn parse_fields(java: &JavaSource) -> Result<Vec<RawField>> {
    let mut fields = Vec::new();
    for (index, line) in java.text.lines().enumerate() {
        let trimmed = line.trim();
        if !trimmed.ends_with(';')
            || !(trimmed.starts_with("public ")
                || trimmed.starts_with("@Nonnull public ")
                || trimmed.starts_with("@Nullable public "))
            || trimmed.contains(" static ")
            || trimmed.contains(" class ")
            || trimmed.contains(" interface ")
            || trimmed.contains(" enum ")
        {
            continue;
        }
        let nullable = trimmed.starts_with("@Nullable");
        let declaration = trimmed
            .trim_start_matches("@Nonnull ")
            .trim_start_matches("@Nullable ")
            .trim_start_matches("public ")
            .trim_end_matches(';');
        let declaration = declaration.split(" = ").next().unwrap_or(declaration);
        // Abstract/static method declarations also end in `;`; a real field has
        // no parenthesis before its initializer.
        if declaration.contains('(') {
            continue;
        }
        let (field_type, name) =
            split_type_and_name(declaration).ok_or_else(|| DumperError::Parse {
                path: java.path.clone(),
                line: index + 1,
                message: format!("cannot parse field declaration: {trimmed}"),
            })?;
        fields.push(RawField {
            name: name.to_string(),
            field_type: normalise_type(field_type),
            nullable,
            line: index + 1,
        });
    }
    if fields.is_empty()
        && !java.text.contains("VARIABLE_FIELD_COUNT = 0")
        && !java.text.contains(" extends ")
        && !java.text.contains("public abstract class ")
    {
        return java.err_at("public class", "packet declares no fields");
    }
    Ok(fields)
}

fn normalise_type(field_type: &str) -> String {
    field_type
        .replace("java.util.UUID", "UUID")
        .replace("java.lang.String", "String")
        .replace("java.lang.Boolean", "Boolean")
        .replace("java.lang.Integer", "Integer")
        .replace("java.lang.Float", "Float")
}

/// Split `Map<K, V>` into its key and value types.
fn map_types(field_type: &str) -> Option<(String, String)> {
    let inner = field_type
        .trim()
        .strip_prefix("java.util.Map<")
        .or_else(|| field_type.trim().strip_prefix("Map<"))?
        .strip_suffix('>')?;
    let mut depth = 0usize;
    for (index, character) in inner.char_indices() {
        match character {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                return Some((
                    inner[..index].trim().to_string(),
                    inner[index + 1..].trim().to_string(),
                ));
            }
            _ => {}
        }
    }
    None
}

fn parse_nullable_bits(
    java: &JavaSource,
    fields: &[RawField],
) -> Result<BTreeMap<String, (u32, u32)>> {
    let mut bits = BTreeMap::new();
    for field in fields.iter().filter(|field| field.nullable) {
        let Some((header, start)) = nullable_header(java, &field.name) else {
            continue;
        };
        let rest = &java.text[start..];
        let end = rest.find('}').unwrap_or(rest.len());
        let body = &rest[..end];
        let Some(byte_offset) = body.find("offset + ") else {
            return java.err_at(&header, format!("has{} has no byte offset", field.name));
        };
        let after = &body[byte_offset + "offset + ".len()..];
        let byte: u32 = after
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .map_err(|_| DumperError::Parse {
                path: java.path.clone(),
                line: java.line_of(start),
                message: format!("has{} byte offset is not an integer", field.name),
            })?;
        let Some(mask_offset) = body.find("b & 0x") else {
            return java.err_at(&header, format!("has{} has no bit mask", field.name));
        };
        let mask_hex: String = body[mask_offset + "b & 0x".len()..]
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect();
        let mask = u32::from_str_radix(&mask_hex, 16).map_err(|_| DumperError::Parse {
            path: java.path.clone(),
            line: java.line_of(start),
            message: format!("has{} bit mask is not hexadecimal", field.name),
        })?;
        let bit = mask.trailing_zeros();
        bits.insert(field.name.clone(), (byte, bit));
    }
    Ok(bits)
}

fn parse_wire(
    java: &JavaSource,
    serialize: &str,
    fields: &[RawField],
) -> Result<BTreeMap<String, WireInfo>> {
    let names: Vec<&str> = fields.iter().map(|field| field.name.as_str()).collect();
    let mut wire: BTreeMap<String, WireInfo> = BTreeMap::new();
    let mut slots = Vec::new();
    let mut variable_order = 0i64;
    let mut array_limits = BTreeMap::new();

    // Length limits are declared in a guard, not in the write call.
    for line in serialize.lines() {
        if line.contains("arrayTooLong(") {
            if let Some(field) = names
                .iter()
                .find(|name| line.contains(&format!("{name}.length")))
            {
                if let Some(limit) = line.split(',').next_back().and_then(|tail| {
                    tail.chars()
                        .filter(|c| c.is_ascii_digit())
                        .collect::<String>()
                        .parse::<i64>()
                        .ok()
                }) {
                    array_limits.insert((*field).to_string(), limit);
                }
            }
        } else if line.contains("dictionaryTooLarge(") {
            if let Some(field) = names.iter().find(|name| {
                line.contains(&format!("this.{name}.size()"))
                    || line.contains(&format!("{name}.size()"))
            }) {
                if let Some(limit) = line.split(',').next_back().and_then(|tail| {
                    tail.chars()
                        .filter(|c| c.is_ascii_digit())
                        .collect::<String>()
                        .parse::<i64>()
                        .ok()
                }) {
                    array_limits.insert((*field).to_string(), limit);
                }
            }
        }
    }

    for line in serialize.lines() {
        let line = line.trim();
        if line.contains("varOffset - offset") {
            if let Some(slot) = number_after(line, "offset + ") {
                slots.push(slot);
            }
            continue;
        }
        let variable_write = line.contains("writeVarString(")
            || line.contains("writeVarAsciiString(")
            || line.contains("VarInt.set(mem, varOffset")
            || line.contains(".serialize(mem, varOffset")
            || line.contains(".serializeWithTypeId(mem, varOffset");
        if !variable_write && !line.contains("offset + ") {
            continue;
        }

        let Some(field) = field_reference(line, &names) else {
            continue;
        };
        let fixed_offset = if variable_write {
            None
        } else {
            number_after(line, "offset + ")
        };

        if line.contains("writeVarString(") || line.contains("writeVarAsciiString(") {
            let encoding = if line.contains("writeVarAsciiString(") {
                "ASCII"
            } else {
                "UTF-8"
            };
            let max_length = line.split(',').next_back().and_then(|tail| {
                tail.chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse::<i64>()
                    .ok()
            });
            let entry = wire.entry(field.to_string()).or_default();
            entry.string = Some(StringInfo {
                encoding: encoding.to_string(),
                length_prefix: "varint".to_string(),
                fixed_length: None,
                max_length,
            });
            if entry.variable_order.is_none() {
                entry.variable_order = Some(variable_order);
                variable_order += 1;
            }
        } else if line.contains("VarInt.set(") {
            let entry = wire.entry(field.to_string()).or_default();
            entry.array_max = array_limits.get(field).copied();
            if entry.variable_order.is_none() {
                entry.variable_order = Some(variable_order);
                variable_order += 1;
            }
        } else if line.contains("writeFixedAsciiString(") {
            let length = line.split(',').next_back().and_then(|tail| {
                tail.chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse::<i64>()
                    .ok()
            });
            let entry = wire.entry(field.to_string()).or_default();
            entry.fixed_offset = fixed_offset;
            entry.string = Some(StringInfo {
                encoding: "ASCII".to_string(),
                length_prefix: "fixed".to_string(),
                fixed_length: length,
                max_length: None,
            });
        } else if line.contains("writeFixedString(") {
            let length = line.split(',').next_back().and_then(|tail| {
                tail.chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse::<i64>()
                    .ok()
            });
            let entry = wire.entry(field.to_string()).or_default();
            entry.fixed_offset = fixed_offset;
            entry.string = Some(StringInfo {
                encoding: "UTF-8".to_string(),
                length_prefix: "fixed".to_string(),
                fixed_length: length,
                max_length: None,
            });
        } else if line.contains(".serialize(mem, varOffset")
            || line.contains(".serializeWithTypeId(mem, varOffset")
        {
            let entry = wire.entry(field.to_string()).or_default();
            if entry.variable_order.is_none() {
                entry.variable_order = Some(variable_order);
                variable_order += 1;
            }
        } else {
            let entry = wire.entry(field.to_string()).or_default();
            entry.fixed_offset = fixed_offset;
        }
    }

    // Variable offset slots are emitted in field order. A single variable field
    // has no random-access slot and is written directly at VARIABLE_BLOCK_START.
    let variable_fields: Vec<&str> = fields
        .iter()
        .filter(|field| {
            wire.get(&field.name)
                .is_some_and(|info| info.variable_order.is_some())
        })
        .map(|field| field.name.as_str())
        .collect();
    if slots.len() == variable_fields.len() {
        for (field, slot) in variable_fields.iter().zip(slots) {
            wire.entry((*field).to_string())
                .or_default()
                .variable_offset_slot = Some(slot);
        }
    } else if !slots.is_empty() {
        return java.err_at(
            "mem.set(PacketIO.PROTO_INT",
            format!(
                "variable offset slot count {} does not match variable field count {}",
                slots.len(),
                variable_fields.len()
            ),
        );
    }

    // Bit-packed booleans: a `boolBits<X>_<Y>` byte collects up to eight
    // `if (this.field) boolBitsX_Y |= 0xNN;` lines and is written once.
    let bool_offsets = bool_group_offsets(serialize);
    for line in serialize.lines() {
        let line = line.trim();
        if !line.contains("|= 0x") || !line.contains("boolBits") {
            continue;
        }
        let Some(field) = field_reference(line, &names) else {
            continue;
        };
        let Some(group) = bool_group(line) else {
            continue;
        };
        let Some(offset) = bool_offsets.get(&group) else {
            continue;
        };
        let Some(mask_start) = line.find("0x") else {
            continue;
        };
        let mask_hex: String = line[mask_start + 2..]
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect();
        let Ok(mask) = u32::from_str_radix(&mask_hex, 16) else {
            continue;
        };
        wire.entry(field.to_string()).or_default().bool_packed = Some(BoolPacked {
            byte_offset: *offset,
            bit: mask.trailing_zeros() as u8,
        });
    }

    Ok(wire)
}

fn bool_group(line: &str) -> Option<String> {
    let start = line.find("boolBits")? + "boolBits".len();
    let group: String = line[start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    (!group.is_empty()).then_some(group)
}

/// Map each `boolBitsX_Y` group to the fixed-block byte it is written to.
fn bool_group_offsets(serialize: &str) -> BTreeMap<String, i64> {
    let mut offsets = BTreeMap::new();
    for line in serialize.lines() {
        let line = line.trim();
        if !line.contains("mem.set(PacketIO.PROTO_BYTE, offset + ") || !line.contains("boolBits") {
            continue;
        }
        let Some(group) = bool_group(line) else {
            continue;
        };
        let Some(after) = line.split("offset + ").nth(1) else {
            continue;
        };
        let expression = after.split(',').next().unwrap_or(after);
        let mut offset = 0i64;
        let mut valid = true;
        for part in expression.split('+') {
            match part.trim().parse::<i64>() {
                Ok(value) => offset += value,
                Err(_) => {
                    valid = false;
                    break;
                }
            }
        }
        if valid {
            offsets.insert(group, offset);
        }
    }
    offsets
}

fn upper_first(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Locate the generated `has<Field>` method.
///
/// The Java generator drops a trailing `Value` for the four `defaultValue`
/// fields (`hasDefault`, not `hasDefaultValue`), so try both forms.
fn nullable_header(java: &JavaSource, field: &str) -> Option<(String, usize)> {
    let mut candidates = vec![upper_first(field)];
    if let Some(stripped) = field.strip_suffix("Value") {
        candidates.push(upper_first(stripped));
    }
    for candidate in candidates {
        let header = format!("public static boolean has{candidate}(");
        if let Some(start) = java.text.find(&header) {
            return Some((header, start));
        }
    }
    None
}

fn field_reference<'a>(line: &str, names: &[&'a str]) -> Option<&'a str> {
    // Longest first so `faceAccessory` never resolves to `face` (or `undertop`
    // to `under`); the identifier-boundary check below is the second guard.
    let mut ordered: Vec<&&str> = names.iter().collect();
    ordered.sort_by_key(|name| std::cmp::Reverse(name.len()));
    for name in ordered {
        if contains_identifier(line, &format!("this.{name}"))
            || contains_identifier(line, &format!("{name}.length"))
            || contains_identifier(line, &format!("{name}.size"))
        {
            return Some(name);
        }
    }
    None
}

/// Match `needle` only at identifier boundaries.
fn contains_identifier(haystack: &str, needle: &str) -> bool {
    let mut start = 0usize;
    while let Some(found) = haystack[start..].find(needle) {
        let position = start + found;
        let before = haystack[..position].chars().next_back();
        let after = haystack[position + needle.len()..].chars().next();
        let boundary = |character: Option<char>| {
            character.is_none_or(|value| !value.is_alphanumeric() && value != '_')
        };
        if boundary(before) && boundary(after) {
            return true;
        }
        start = position + 1;
    }
    false
}

fn number_after(text: &str, marker: &str) -> Option<i64> {
    let start = text.find(marker)? + marker.len();
    let digits: String = text[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

fn classify_type(
    field_type: &str,
    enum_names: &[String],
    wire: &WireInfo,
) -> (String, Option<String>, Option<String>) {
    if field_type.ends_with("[]") {
        return ("array".to_string(), None, None);
    }
    if field_type == "String" {
        return ("string".to_string(), None, None);
    }
    if field_type == "UUID" {
        return ("uuid".to_string(), None, None);
    }
    if matches!(
        field_type,
        "byte"
            | "short"
            | "int"
            | "long"
            | "float"
            | "double"
            | "boolean"
            | "Boolean"
            | "Integer"
            | "Float"
    ) {
        return ("primitive".to_string(), None, None);
    }
    if let Some(kind) = vector_kind(field_type) {
        return (kind.to_string(), None, None);
    }
    if enum_names.iter().any(|name| name == field_type) {
        return ("enum".to_string(), Some(field_type.to_string()), None);
    }
    if map_types(field_type).is_some() {
        return ("map".to_string(), None, None);
    }
    if wire.string.is_some() {
        return ("string".to_string(), None, None);
    }
    ("struct".to_string(), None, Some(field_type.to_string()))
}

/// JOML vector interfaces used by the generated sources.
///
/// The plain `Vector3i`/`Vector3d`/`Vector2i` protocol classes are generated
/// structs; only the JOML interfaces need synthetic Rust types.
fn vector_kind(field_type: &str) -> Option<&'static str> {
    Some(match field_type {
        "Vector2fc" | "Vector2f" => "vector2f",
        "Vector3fc" | "Vector3f" => "vector3f",
        "Vector4fc" | "Vector4f" => "vector4f",
        "Quaternionfc" | "Quaternionf" => "quaternionf",
        _ => return None,
    })
}

fn direction(text: &str) -> String {
    let clause = implements_clause(text);
    let to_server = clause
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|part| part == "ToServerPacket");
    let to_client = clause
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|part| part == "ToClientPacket");
    match (to_server, to_client) {
        (true, true) => "Both",
        (true, false) => "ToServer",
        (false, true) => "ToClient",
        (false, false) => "None",
    }
    .to_string()
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

fn implements_clause(text: &str) -> &str {
    let Some(start) = text.find("implements ") else {
        return "";
    };
    let rest = &text[start + "implements ".len()..];
    let end = rest.find('{').unwrap_or(rest.len());
    &rest[..end]
}

fn validate(sources: &[JavaSource], packets: &[PacketIr]) -> Result<()> {
    let connect = packets
        .iter()
        .find(|packet| packet.name == "Connect")
        .ok_or_else(|| DumperError::Parse {
            path: sources
                .first()
                .map(|source| source.path.clone())
                .unwrap_or_default(),
            line: 0,
            message: "Connect.java was not parsed".to_string(),
        })?;
    let mut errors = Vec::new();
    if connect.packet_id != 0 {
        errors.push(format!("Connect.PACKET_ID is {}", connect.packet_id));
    }
    if connect.fixed_block_size != 30 {
        errors.push(format!(
            "Connect.FIXED_BLOCK_SIZE is {}",
            connect.fixed_block_size
        ));
    }
    if connect.variable_block_start != 46 {
        errors.push(format!(
            "Connect.VARIABLE_BLOCK_START is {}",
            connect.variable_block_start
        ));
    }
    if connect.variable_field_count != 4 {
        errors.push(format!(
            "Connect.VARIABLE_FIELD_COUNT is {}",
            connect.variable_field_count
        ));
    }
    let expected = [
        "protocolCrc",
        "protocolBuildNumber",
        "clientVersion",
        "clientType",
        "identityToken",
        "language",
        "referralData",
        "referralSource",
    ];
    let actual: Vec<&str> = connect
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect();
    if actual != expected {
        errors.push(format!(
            "Connect field order is {}, expected {}",
            actual.join(","),
            expected.join(",")
        ));
    }
    if let Some(field) = connect
        .fields
        .iter()
        .find(|field| field.name == "identityToken")
    {
        if field.nullable_bit != Some(0) {
            errors.push(format!(
                "Connect.identityToken nullable bit is {:?}, expected 0",
                field.nullable_bit
            ));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(DumperError::Parse {
            path: sources
                .first()
                .map(|source| source.path.clone())
                .unwrap_or_default(),
            line: 0,
            message: format!("Connect validation failed: {}", errors.join("; ")),
        })
    }
}
