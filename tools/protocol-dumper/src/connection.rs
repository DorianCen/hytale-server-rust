//! Stage 2: field-level IR for the `packets/connection` family.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::java::{
    bool_const, display_path, int_const, method_body, package_name, split_type_and_name, JavaSource,
};
use crate::{DumperError, Result};

/// Field-level IR for the connection packet family.
#[derive(Debug, Serialize)]
pub struct ConnectionIr {
    /// Generation metadata.
    pub meta: Meta,
    /// Enums declared in the connection package.
    pub enums: Vec<EnumIr>,
    /// Packets, sorted by packet id.
    pub packets: Vec<PacketIr>,
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
    /// Enum type, when the field is an enum.
    pub enum_type: Option<String>,
    /// Nested struct type, when the field is a struct.
    pub nested_type: Option<String>,
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

    Ok(ConnectionIr {
        meta: Meta {
            source: display_path(source),
            generator: format!("protocol-dumper {}", env!("CARGO_PKG_VERSION")),
            counts: Counts {
                packets: packets.len(),
                enums: enums.len(),
            },
        },
        enums,
        packets,
    })
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

    let mut fields = Vec::with_capacity(raw_fields.len());
    for raw in raw_fields {
        let info = wire.get(&raw.name).cloned().unwrap_or_default();
        let (kind, enum_type, nested_type) = classify_type(&raw.field_type, enum_names, &info);
        let string = info.string.clone();
        let array = raw
            .field_type
            .strip_suffix("[]")
            .map(|element_type| ArrayInfo {
                element_type: element_type.to_string(),
                max_length: info.array_max,
            });
        let (nullable_bit, nullable_byte) = match nullable_bits.get(&raw.name) {
            Some((byte, bit)) => (Some(*bit), Some(*byte)),
            None => (None, None),
        };
        if raw.nullable && nullable_bit.is_none() {
            return Err(DumperError::Parse {
                path: java.path.clone(),
                line: raw.line,
                message: format!(
                    "nullable field {} has no generated has{}() method",
                    raw.name, raw.name
                ),
            });
        }
        fields.push(FieldIr {
            name: raw.name,
            field_type: raw.field_type,
            kind,
            nullable: raw.nullable,
            nullable_bit,
            nullable_byte,
            fixed_offset: info.fixed_offset,
            fixed_size: info.fixed_size,
            variable_order: info.variable_order,
            variable_offset_slot: info.variable_offset_slot,
            string,
            array,
            enum_type,
            nested_type,
        });
    }

    // Compute fixed sizes from the following fixed offset or the end of the
    // fixed block. This covers nested structs whose size is not repeated in
    // the packet class.
    let mut offsets: Vec<(i64, usize)> = fields
        .iter()
        .enumerate()
        .filter_map(|(index, field)| field.fixed_offset.map(|offset| (offset, index)))
        .collect();
    offsets.sort_unstable();
    for (position, (offset, index)) in offsets.iter().enumerate() {
        if fields[*index].fixed_size.is_some() {
            continue;
        }
        let end = offsets
            .get(position + 1)
            .map(|(next, _)| *next)
            .unwrap_or(fixed_block_size);
        fields[*index].fixed_size = Some(end - offset);
    }

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
    if fields.is_empty() && !java.text.contains("VARIABLE_FIELD_COUNT = 0") {
        return java.err_at("public class", "packet declares no fields");
    }
    Ok(fields)
}

fn normalise_type(field_type: &str) -> String {
    field_type
        .replace("java.util.UUID", "UUID")
        .replace("java.lang.String", "String")
}

fn parse_nullable_bits(
    java: &JavaSource,
    fields: &[RawField],
) -> Result<BTreeMap<String, (u32, u32)>> {
    let mut bits = BTreeMap::new();
    for field in fields.iter().filter(|field| field.nullable) {
        let header = format!("public static boolean has{}(", upper_first(&field.name));
        let Some(start) = java.text.find(&header) else {
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

    // Array limits are declared in a length guard, not in the write call.
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
            || line.contains(".serialize(mem, varOffset");
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
        } else if line.contains(".serialize(mem, varOffset") {
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

    Ok(wire)
}

fn upper_first(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn field_reference<'a>(line: &str, names: &[&'a str]) -> Option<&'a str> {
    for name in names {
        if line.contains(&format!("this.{name}"))
            || line.contains(&format!("this.{name}.length"))
            || line.contains(&format!("{name}.length"))
        {
            return Some(name);
        }
    }
    // Array writes use the bare field in a guard but `this.field.length` in the
    // write line; the loop above covers both generated forms.
    None
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
        "byte" | "short" | "int" | "long" | "float" | "double" | "boolean"
    ) {
        return ("primitive".to_string(), None, None);
    }
    if enum_names.iter().any(|name| name == field_type) {
        return ("enum".to_string(), Some(field_type.to_string()), None);
    }
    if wire.string.is_some() {
        return ("string".to_string(), None, None);
    }
    if field_type.starts_with("Map<") || field_type.starts_with("java.util.Map<") {
        return ("map".to_string(), None, None);
    }
    ("struct".to_string(), None, Some(field_type.to_string()))
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
