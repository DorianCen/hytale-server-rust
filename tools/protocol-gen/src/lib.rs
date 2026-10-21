//! `protocol-gen` — protocol IR → Rust source.
//!
//! The connection mode is the first end-to-end slice of the generator: it emits
//! the packet types, their nested structs/unions, the connection enums, and the
//! shared `Wire`/`Packet` traits. Generated files are write-only.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use serde::Deserialize;

/// Errors returned by the generator.
#[derive(Debug, thiserror::Error)]
pub enum GenError {
    /// A filesystem operation failed.
    #[error("{path}: {source}")]
    Io {
        /// Path that failed.
        path: String,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The IR was malformed.
    #[error("invalid IR: {0}")]
    Ir(String),
    /// JSON decoding failed.
    #[error("decode IR: {0}")]
    Json(#[from] serde_json::Error),
    /// rustfmt failed.
    #[error("rustfmt: {0}")]
    Format(String),
}

type Result<T> = std::result::Result<T, GenError>;

#[derive(Debug, Deserialize)]
struct ConnectionIr {
    #[allow(dead_code)]
    enums: Vec<EnumIr>,
    #[serde(default)]
    structs: Vec<StructIr>,
    packets: Vec<PacketIr>,
}

#[derive(Debug, Deserialize)]
struct EnumIr {
    name: String,
    package: String,
    values: Vec<EnumValue>,
}

#[derive(Debug, Deserialize)]
struct EnumValue {
    name: String,
    value: i64,
}

#[derive(Debug, Deserialize)]
struct StructIr {
    name: String,
    #[allow(dead_code)]
    package: String,
    #[serde(default)]
    nullable_bit_field_size: i64,
    #[serde(default)]
    fixed_block_size: i64,
    #[serde(default)]
    #[allow(dead_code)]
    variable_field_count: i64,
    #[serde(default)]
    #[allow(dead_code)]
    variable_block_start: i64,
    #[serde(default)]
    #[allow(dead_code)]
    max_size: i64,
    #[serde(default)]
    fields: Vec<FieldIr>,
    #[serde(default)]
    union: Option<UnionIr>,
}

#[derive(Debug, Deserialize)]
struct UnionIr {
    #[allow(dead_code)]
    name: String,
    subtypes: Vec<UnionSubtype>,
}

#[derive(Debug, Deserialize)]
struct UnionSubtype {
    type_id: i64,
    name: String,
}

#[derive(Debug, Deserialize)]
struct PacketIr {
    name: String,
    packet_id: i64,
    direction: String,
    channel: String,
    compressed: bool,
    nullable_bit_field_size: i64,
    fixed_block_size: i64,
    #[allow(dead_code)]
    variable_field_count: i64,
    #[allow(dead_code)]
    variable_block_start: i64,
    max_size: i64,
    fields: Vec<FieldIr>,
}

#[derive(Debug, Deserialize, Clone)]
struct FieldIr {
    name: String,
    field_type: String,
    kind: String,
    nullable: bool,
    #[serde(default)]
    nullable_bit: Option<u32>,
    #[serde(default)]
    nullable_byte: Option<u32>,
    #[serde(default)]
    fixed_offset: Option<i64>,
    #[serde(default)]
    fixed_size: Option<i64>,
    #[serde(default)]
    variable_order: Option<i64>,
    #[serde(default)]
    variable_offset_slot: Option<i64>,
    #[serde(default)]
    string: Option<StringInfo>,
    #[serde(default)]
    array: Option<ArrayInfo>,
    #[serde(default)]
    map: Option<MapInfo>,
    #[serde(default)]
    enum_type: Option<String>,
    #[serde(default)]
    nested_type: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct StringInfo {
    encoding: String,
    #[allow(dead_code)]
    length_prefix: String,
    #[serde(default)]
    #[allow(dead_code)]
    fixed_length: Option<i64>,
    #[serde(default)]
    max_length: Option<i64>,
}

#[derive(Debug, Deserialize, Clone)]
struct ArrayInfo {
    element_type: String,
    #[serde(default)]
    max_length: Option<i64>,
}

#[derive(Debug, Deserialize, Clone)]
struct MapInfo {
    #[allow(dead_code)]
    key_type: String,
    value_type: String,
    #[serde(default)]
    max_length: Option<i64>,
}

/// Generate the connection-family sources from a checked-in IR.
pub fn generate_connection(ir: &Path) -> Result<BTreeMap<String, String>> {
    let text = std::fs::read_to_string(ir).map_err(|source| GenError::Io {
        path: ir.display().to_string(),
        source,
    })?;
    let ir: ConnectionIr = serde_json::from_str(&text)?;

    let mut enums = BTreeMap::new();
    for enumeration in &ir.enums {
        if enumeration.values.is_empty() {
            return Err(GenError::Ir(format!(
                "enum {} has no values",
                enumeration.name
            )));
        }
        enums.insert(enumeration.name.clone(), enumeration);
    }

    let mut files = BTreeMap::new();
    files.insert("mod.rs".to_string(), generate_mod());
    files.insert("wire.rs".to_string(), generate_wire());
    files.insert(
        "connection.rs".to_string(),
        generate_connection_module(&ir, &enums)?,
    );
    files.insert(
        "common.rs".to_string(),
        generate_common_module(&ir.structs, &enums)?,
    );
    for contents in files.values_mut() {
        *contents = format_source(contents)?;
    }
    Ok(files)
}

/// Format a generated source file with the toolchain's rustfmt.
fn format_source(source: &str) -> Result<String> {
    let mut child = Command::new("rustfmt")
        .args(["--emit", "stdout", "--edition", "2021"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| GenError::Format(error.to_string()))?;
    child
        .stdin
        .take()
        .ok_or_else(|| GenError::Format("rustfmt stdin unavailable".to_string()))?
        .write_all(source.as_bytes())
        .map_err(|error| GenError::Format(error.to_string()))?;
    let output = child
        .wait_with_output()
        .map_err(|error| GenError::Format(error.to_string()))?;
    if !output.status.success() {
        return Err(GenError::Format(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    String::from_utf8(output.stdout).map_err(|error| GenError::Format(error.to_string()))
}

/// Write generated files into `out`.
pub fn write_files(out: &Path, files: &BTreeMap<String, String>) -> Result<()> {
    std::fs::create_dir_all(out).map_err(|source| GenError::Io {
        path: out.display().to_string(),
        source,
    })?;
    for (name, contents) in files {
        let path = out.join(name);
        std::fs::write(&path, contents).map_err(|source| GenError::Io {
            path: path.display().to_string(),
            source,
        })?;
    }
    Ok(())
}

/// Regenerate to a temp directory and compare against `out`.
///
/// Returns the paths that differ. A missing file and a stale file both count as
/// a difference; unrelated files in `out` are ignored.
pub fn check(out: &Path, fresh: &BTreeMap<String, String>) -> Result<Vec<String>> {
    let mut differences = Vec::new();
    for (name, expected) in fresh {
        let path = out.join(name);
        match std::fs::read_to_string(&path) {
            Ok(actual) if actual == *expected => {}
            Ok(_) => differences.push(format!("{name} is stale")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                differences.push(format!("{name} is missing"));
            }
            Err(source) => {
                return Err(GenError::Io {
                    path: path.display().to_string(),
                    source,
                });
            }
        }
    }
    let entries = std::fs::read_dir(out).map_err(|source| GenError::Io {
        path: out.display().to_string(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| GenError::Io {
            path: out.display().to_string(),
            source,
        })?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if !fresh.contains_key(name) {
                differences.push(format!("{name} is not generated by protocol-gen"));
            }
        }
    }
    Ok(differences)
}

fn generate_mod() -> String {
    let mut out = String::new();
    out.push_str("// @generated by tools/protocol-gen. Do not edit.\n\n");
    out.push_str("#![allow(clippy::all)]\n\n");
    out.push_str("pub mod common;\n");
    out.push_str("pub mod connection;\n");
    out.push_str("pub mod wire;\n\n");
    out.push_str("pub use common::*;\n");
    out.push_str("pub use wire::{NetworkChannel, Packet, PacketDirection, Wire};\n");
    out
}

fn generate_wire() -> String {
    let mut out = String::new();
    out.push_str("// @generated by tools/protocol-gen. Do not edit.\n\n");
    out.push_str("#![allow(clippy::all)]\n\n");
    out.push_str("use crate::io::{ProtocolError, Reader, Result, Uuid, Writer};\n\n");
    out.push_str(
        "/// A value that owns its fixed block and variable block.\n\
         pub trait Wire: Sized {\n\
         \x20   /// Encode the value.\n\
         \x20   fn encode(&self, writer: &mut Writer) -> Result<()>;\n\
         \x20   /// Decode one value, consuming exactly its bytes.\n\
         \x20   fn decode(reader: &mut Reader<'_>) -> Result<Self>;\n\
         \x20   /// Encoded size.\n\
         \x20   fn compute_size(&self) -> Result<usize> {\n\
         \x20       let mut writer = Writer::new();\n\
         \x20       self.encode(&mut writer)?;\n\
         \x20       Ok(writer.len())\n\
         \x20   }\n\
         }\n\n",
    );
    out.push_str(
        "/// Packet direction from the generated Java marker interface.\n\
         #[derive(Debug, Clone, Copy, PartialEq, Eq)]\n\
         pub enum PacketDirection {\n\
         \x20   /// Client to server.\n\
         \x20   ToServer,\n\
         \x20   /// Server to client.\n\
         \x20   ToClient,\n\
         \x20   /// Both directions.\n\
         \x20   Both,\n\
         \x20   /// Unknown marker.\n\
         \x20   None,\n\
         }\n\n",
    );
    out.push_str(
        "/// Network channel.\n\
         #[derive(Debug, Clone, Copy, PartialEq, Eq)]\n\
         pub enum NetworkChannel {\n\
         \x20   /// Default reliable stream.\n\
         \x20   Default,\n\
         \x20   /// Chunk stream.\n\
         \x20   Chunks,\n\
         \x20   /// World-map stream.\n\
         \x20   WorldMap,\n\
         \x20   /// Voice stream.\n\
         \x20   Voice,\n\
         }\n\n",
    );
    out.push_str(
        "/// A generated packet.\n\
         pub trait Packet: Wire {\n\
         \x20   /// Packet id from `PacketRegistry`.\n\
         \x20   const ID: u32;\n\
         \x20   /// Stream channel.\n\
         \x20   const CHANNEL: NetworkChannel;\n\
         \x20   /// Direction marker.\n\
         \x20   const DIRECTION: PacketDirection;\n\
         \x20   /// Whether the frame payload is zstd-compressed.\n\
         \x20   const COMPRESSED: bool;\n\
         \x20   /// Protocol maximum payload size.\n\
         \x20   const MAX_SIZE: u32;\n\
         }\n\n",
    );
    out.push_str("/// Read a fixed field at `offset`.\n");
    out.push_str(
        "pub fn read_at<'a, T>(\n\
         \x20   fixed: &'a [u8],\n\
         \x20   offset: usize,\n\
         \x20   read: impl FnOnce(&mut Reader<'a>) -> Result<T>,\n\
         ) -> Result<T> {\n\
         \x20   let mut reader = Reader::new(fixed);\n\
         \x20   reader.seek(offset)?;\n\
         \x20   read(&mut reader)\n\
         }\n\n",
    );
    for (name, ty, size) in [
        ("u8", "u8", 1),
        ("i8", "i8", 1),
        ("u16", "u16", 2),
        ("i16", "i16", 2),
        ("u32", "u32", 4),
        ("i32", "i32", 4),
        ("u64", "u64", 8),
        ("i64", "i64", 8),
        ("f32", "f32", 4),
        ("f64", "f64", 8),
    ] {
        let _ = write!(
            out,
            "/// Write a little-endian `{ty}` at `offset`.\n\
             pub fn write_{name}_at(fixed: &mut [u8], offset: usize, value: {ty}) -> Result<()> {{\n\
             \x20   write_slice_at(fixed, offset, &value.to_le_bytes())\n\
             }}\n\n"
        );
        let _ = size;
    }
    out.push_str(
        "/// Write a boolean at `offset`.\n\
         pub fn write_bool_at(fixed: &mut [u8], offset: usize, value: bool) -> Result<()> {\n\
         \x20   write_u8_at(fixed, offset, u8::from(value))\n\
         }\n\n\
         /// Write a UUID at `offset`.\n\
         pub fn write_uuid_at(fixed: &mut [u8], offset: usize, value: Uuid) -> Result<()> {\n\
         \x20   write_slice_at(fixed, offset, &value.to_bytes())\n\
         }\n\n\
         /// Write raw bytes at `offset`.\n\
         pub fn write_slice_at(fixed: &mut [u8], offset: usize, bytes: &[u8]) -> Result<()> {\n\
         \x20   let Some(end) = offset.checked_add(bytes.len()) else {\n\
         \x20       return Err(ProtocolError::FixedBlockOverflow { offset, length: bytes.len(), size: fixed.len() });\n\
         \x20   };\n\
         \x20   if end > fixed.len() {\n\
         \x20       return Err(ProtocolError::FixedBlockOverflow { offset, length: bytes.len(), size: fixed.len() });\n\
         \x20   }\n\
         \x20   fixed[offset..end].copy_from_slice(bytes);\n\
         \x20   Ok(())\n\
         }\n\n\
         /// Write a zero-padded ASCII string at `offset`.\n\
         pub fn write_fixed_ascii_at(\n\
         \x20   fixed: &mut [u8],\n\
         \x20   offset: usize,\n\
         \x20   value: &str,\n\
         \x20   length: usize,\n\
         ) -> Result<()> {\n\
         \x20   if length > fixed.len() || offset.checked_add(length).is_none_or(|end| end > fixed.len()) {\n\
         \x20       return Err(ProtocolError::FixedBlockOverflow { offset, length, size: fixed.len() });\n\
         \x20   }\n\
         \x20   let bytes = value.as_bytes();\n\
         \x20   if bytes.len() > length {\n\
         \x20       return Err(ProtocolError::FixedStringTooLong { field: \"fixed string\", length, value_len: bytes.len() });\n\
         \x20   }\n\
         \x20   if !value.is_ascii() {\n\
         \x20       return Err(ProtocolError::InvalidAscii { field: \"fixed string\" });\n\
         \x20   }\n\
         \x20   fixed[offset..offset + length].fill(0);\n\
         \x20   fixed[offset..offset + bytes.len()].copy_from_slice(bytes);\n\
         \x20   Ok(())\n\
         }\n\n\
         /// Write a zero-padded UTF-8 string at `offset`.\n\
         pub fn write_fixed_utf8_at(\n\
         \x20   fixed: &mut [u8],\n\
         \x20   offset: usize,\n\
         \x20   value: &str,\n\
         \x20   length: usize,\n\
         ) -> Result<()> {\n\
         \x20   if length > fixed.len() || offset.checked_add(length).is_none_or(|end| end > fixed.len()) {\n\
         \x20       return Err(ProtocolError::FixedBlockOverflow { offset, length, size: fixed.len() });\n\
         \x20   }\n\
         \x20   let bytes = value.as_bytes();\n\
         \x20   if bytes.len() > length {\n\
         \x20       return Err(ProtocolError::FixedStringTooLong { field: \"fixed string\", length, value_len: bytes.len() });\n\
         \x20   }\n\
         \x20   fixed[offset..offset + length].fill(0);\n\
         \x20   fixed[offset..offset + bytes.len()].copy_from_slice(bytes);\n\
         \x20   Ok(())\n\
         }\n",
    );
    out
}

fn generate_connection_module(
    ir: &ConnectionIr,
    enums: &BTreeMap<String, &EnumIr>,
) -> Result<String> {
    let mut out = String::new();
    out.push_str("// @generated by tools/protocol-gen. Do not edit.\n\n");
    out.push_str(
        "#![allow(clippy::all)]
#![allow(unused_imports, unused_mut, unused_variables)]\n\n",
    );
    out.push_str("use std::collections::BTreeMap;\n\n");
    out.push_str("use crate::io::{NullableBitset, Reader, Result, Uuid, Writer};\n\n");
    out.push_str("use super::common::*;\n");
    out.push_str("use super::wire::*;\n\n");

    for enumeration in enums
        .values()
        .filter(|enumeration| enumeration.package.ends_with(".packets.connection"))
    {
        out.push_str(&generate_enum(enumeration));
    }

    for packet in &ir.packets {
        out.push_str(&generate_struct(
            &packet.name,
            packet.nullable_bit_field_size,
            packet.fixed_block_size,
            packet.variable_block_start,
            packet.fields.clone(),
            enums,
        )?);
        out.push_str(&generate_packet_impl(packet));
    }
    Ok(out)
}

fn generate_common_module(
    structs: &[StructIr],
    enums: &BTreeMap<String, &EnumIr>,
) -> Result<String> {
    let mut out = String::new();
    out.push_str("// @generated by tools/protocol-gen. Do not edit.\n\n");
    out.push_str(
        "#![allow(clippy::all)]
#![allow(unused_imports, unused_mut, unused_variables)]\n\n",
    );
    out.push_str("use std::collections::BTreeMap;\n\n");
    out.push_str("use crate::io::{NullableBitset, Reader, Result, Uuid, Writer};\n\n");
    out.push_str("use super::wire::*;\n\n");

    for enumeration in enums
        .values()
        .filter(|enumeration| !enumeration.package.ends_with(".packets.connection"))
    {
        out.push_str(&generate_enum(enumeration));
    }
    for structure in structs {
        if let Some(union) = &structure.union {
            out.push_str(&generate_union(union, enums)?);
        } else {
            out.push_str(&generate_struct(
                &structure.name,
                structure.nullable_bit_field_size,
                structure.fixed_block_size,
                structure.variable_block_start,
                structure.fields.clone(),
                enums,
            )?);
        }
    }
    Ok(out)
}

fn generate_enum(enumeration: &EnumIr) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "/// Java `{}`.\n#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub enum {} {{",
        enumeration.name, enumeration.name
    );
    for value in &enumeration.values {
        let _ = writeln!(out, "    /// Wire value {}.", value.value);
        let _ = writeln!(out, "    {} = {},", value.name, value.value);
    }
    out.push_str("}\n\n");

    let _ = writeln!(out, "impl {} {{", enumeration.name);
    out.push_str(
        "    /// Wire value.\n    pub const fn value(self) -> i64 {\n        match self {\n",
    );
    for value in &enumeration.values {
        let _ = writeln!(out, "            Self::{} => {},", value.name, value.value);
    }
    out.push_str("        }\n    }\n\n");
    out.push_str("    /// Decode a wire value.\n    pub fn from_value(value: i64) -> Result<Self> {\n        match value {\n");
    for value in &enumeration.values {
        let _ = writeln!(
            out,
            "            {} => Ok(Self::{}),",
            value.value, value.name
        );
    }
    let _ = writeln!(
        out,
        "            other => Err(crate::io::ProtocolError::InvalidEnum {{ type_name: \"{}\", value: other }}),",
        enumeration.name
    );
    out.push_str("        }\n    }\n\n");
    out.push_str(
        "    /// Write this value into a fixed block.\n\
         \x20   pub fn write_at(self, fixed: &mut [u8], offset: usize, size: usize) -> Result<()> {\n\
         \x20       match size {\n\
         \x20           1 => write_u8_at(fixed, offset, self.value() as u8),\n\
         \x20           2 => write_u16_at(fixed, offset, self.value() as u16),\n\
         \x20           4 => write_i32_at(fixed, offset, self.value() as i32),\n\
         \x20           8 => write_i64_at(fixed, offset, self.value()),\n\
         \x20           other => Err(crate::io::ProtocolError::LengthTooLarge { field: \"enum size\", length: other, max: 8 }),\n\
         \x20       }\n\
         \x20   }\n\n\
         \x20   /// Write this value into a variable block.\n\
         \x20   pub fn write(self, writer: &mut Writer, size: usize) -> Result<()> {\n\
         \x20       match size {\n\
         \x20           1 => writer.write_u8(self.value() as u8),\n\
         \x20           2 => writer.write_u16(self.value() as u16),\n\
         \x20           4 => writer.write_i32(self.value() as i32),\n\
         \x20           8 => writer.write_i64(self.value()),\n\
         \x20           other => return Err(crate::io::ProtocolError::LengthTooLarge { field: \"enum size\", length: other, max: 8 }),\n\
         \x20       }\n\
         \x20       Ok(())\n\
         \x20   }\n\n\
         \x20   /// Read this value from a variable block.\n\
         \x20   pub fn read(reader: &mut Reader<'_>, size: usize) -> Result<Self> {\n\
         \x20       let value = match size {\n\
         \x20           1 => reader.read_u8()? as i64,\n\
         \x20           2 => reader.read_u16()? as i64,\n\
         \x20           4 => reader.read_i32()? as i64,\n\
         \x20           8 => reader.read_i64()?,\n\
         \x20           other => return Err(crate::io::ProtocolError::LengthTooLarge { field: \"enum size\", length: other, max: 8 }),\n\
         \x20       };\n\
         \x20       Self::from_value(value)\n\
         \x20   }\n\n\
         \x20   /// Read this value from a fixed block.\n\
         \x20   pub fn read_at(fixed: &[u8], offset: usize, size: usize) -> Result<Self> {\n\
         \x20       let value = match size {\n\
         \x20           1 => read_at(fixed, offset, |reader| reader.read_u8())? as i64,\n\
         \x20           2 => read_at(fixed, offset, |reader| reader.read_u16())? as i64,\n\
         \x20           4 => read_at(fixed, offset, |reader| reader.read_i32())? as i64,\n\
         \x20           8 => read_at(fixed, offset, |reader| reader.read_i64())?,\n\
         \x20           other => return Err(crate::io::ProtocolError::LengthTooLarge { field: \"enum size\", length: other, max: 8 }),\n\
         \x20       };\n\
         \x20       Self::from_value(value)\n\
         \x20   }\n\
         }\n\n",
    );
    let first = &enumeration.values[0].name;
    let _ = writeln!(
        out,
        "impl Default for {} {{\n    fn default() -> Self {{ Self::{first} }}\n}}\n",
        enumeration.name
    );
    out
}

fn generate_union(union: &UnionIr, enums: &BTreeMap<String, &EnumIr>) -> Result<String> {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "/// Java `{}` polymorphic value.\n#[derive(Debug, Clone, PartialEq)]\npub enum {} {{",
        union.name, union.name
    );
    for subtype in &union.subtypes {
        let _ = writeln!(
            out,
            "    /// Type id {}.\n    {}({}),",
            subtype.type_id, subtype.name, subtype.name
        );
    }
    out.push_str("}\n\n");

    let _ = writeln!(out, "impl {} {{", union.name);
    out.push_str("    /// Type id written before the body.\n    pub const fn type_id(&self) -> u32 {\n        match self {\n");
    for subtype in &union.subtypes {
        let _ = writeln!(
            out,
            "            Self::{}(_) => {},",
            subtype.name, subtype.type_id
        );
    }
    out.push_str("        }\n    }\n\n");
    out.push_str("    /// Encode the type id and body.\n    pub fn encode(&self, writer: &mut Writer) -> Result<()> {\n        writer.write_varint(self.type_id())?;\n        match self {\n");
    for subtype in &union.subtypes {
        let _ = writeln!(
            out,
            "            Self::{}(value) => value.encode(writer),",
            subtype.name
        );
    }
    out.push_str("        }\n    }\n\n");
    out.push_str("    /// Decode a type id and body.\n    pub fn decode(reader: &mut Reader<'_>) -> Result<Self> {\n        match reader.read_varint()? {\n");
    let _ = enums;
    for subtype in &union.subtypes {
        let _ = writeln!(
            out,
            "            {} => Ok(Self::{}({}::decode(reader)?)),",
            subtype.type_id, subtype.name, subtype.name
        );
    }
    let _ = writeln!(
        out,
        "            other => Err(crate::io::ProtocolError::InvalidEnum {{ type_name: \"{}\", value: i64::from(other) }}),",
        union.name
    );
    out.push_str("        }\n    }\n\n");
    out.push_str(
        "    /// Encoded size.\n\
         \x20   pub fn compute_size(&self) -> Result<usize> {\n\
         \x20       let mut writer = Writer::new();\n\
         \x20       self.encode(&mut writer)?;\n\
         \x20       Ok(writer.len())\n\
         \x20   }\n\
         }\n\n",
    );
    let first = &union.subtypes[0];
    let _ = writeln!(
        out,
        "impl Default for {} {{\n    fn default() -> Self {{ Self::{}(Default::default()) }}\n}}\n",
        union.name, first.name
    );
    Ok(out)
}

fn generate_struct(
    name: &str,
    nullable_bit_field_size: i64,
    fixed_block_size: i64,
    variable_block_start: i64,
    fields: Vec<FieldIr>,
    enums: &BTreeMap<String, &EnumIr>,
) -> Result<String> {
    let mut fixed_fields = Vec::new();
    let mut variable_fields = Vec::new();
    for field in &fields {
        if field.variable_order.is_some() || field.fixed_offset.is_none() {
            variable_fields.push(field.clone());
        } else {
            fixed_fields.push(field.clone());
        }
    }
    variable_fields.sort_by_key(|field| field.variable_order.unwrap_or(0));

    let mut out = String::new();
    let _ = writeln!(
        out,
        "/// Java `{name}`.\n#[derive(Debug, Clone, PartialEq)]\npub struct {name} {{"
    );
    for field in &fields {
        let identifier = rust_ident(&to_snake(&field.name));
        let ty = rust_type(field, enums)?;
        let _ = writeln!(
            out,
            "    /// `{}`.\n    pub {identifier}: {ty},",
            field.name
        );
    }
    out.push_str("}\n\n");

    out.push_str(&format!(
        "impl Default for {name} {{\n    fn default() -> Self {{\n        Self {{\n"
    ));
    for field in &fields {
        let identifier = rust_ident(&to_snake(&field.name));
        let value = default_expr(field, enums)?;
        let _ = writeln!(out, "            {identifier}: {value},");
    }
    out.push_str("        }\n    }\n}\n\n");

    let has_variables = !variable_fields.is_empty();
    let needs_mut_fixed = !fixed_fields.is_empty()
        || nullable_bit_field_size > 0
        || variable_fields
            .iter()
            .any(|field| field.variable_offset_slot.is_some());
    let _ = writeln!(out, "impl Wire for {name} {{");
    out.push_str("    fn encode(&self, writer: &mut Writer) -> Result<()> {\n");
    let _ = writeln!(
        out,
        "        let {}fixed = vec![0u8; {}usize];",
        if needs_mut_fixed { "mut " } else { "" },
        variable_block_start.max(fixed_block_size)
    );
    if nullable_bit_field_size > 0 {
        let _ = writeln!(
            out,
            "        let mut bits = NullableBitset::new({nullable_bit_field_size}usize);"
        );
        for field in &fields {
            if field.nullable {
                let identifier = rust_ident(&to_snake(&field.name));
                let bit = field_nullable_index(field)?;
                let _ = writeln!(
                    out,
                    "        if self.{identifier}.is_some() {{ bits.set({bit}, true)?; }}"
                );
            }
        }
        out.push_str("        fixed[..bits.as_bytes().len()].copy_from_slice(bits.as_bytes());\n");
    }
    if has_variables {
        out.push_str("        let mut vars = Writer::new();\n");
    }

    for field in &fixed_fields {
        out.push_str(&encode_fixed_field(field, enums)?);
    }
    for field in &variable_fields {
        out.push_str(&encode_variable_field(field, enums)?);
    }

    out.push_str("        writer.write_slice(&fixed);\n");
    if has_variables {
        out.push_str("        writer.write_slice(vars.as_slice());\n");
    }
    out.push_str("        Ok(())\n    }\n\n");

    out.push_str("    fn decode(reader: &mut Reader<'_>) -> Result<Self> {\n");
    let _ = writeln!(
        out,
        "        let fixed = reader.read_slice({}usize)?.to_vec();",
        variable_block_start.max(fixed_block_size)
    );
    if nullable_bit_field_size > 0 {
        let _ = writeln!(
            out,
            "        let bits = NullableBitset::from_bytes(fixed[..{nullable_bit_field_size}usize].to_vec());"
        );
    } else {
        out.push_str("        let bits = NullableBitset::default();\n");
    }
    if has_variables {
        out.push_str("        let var_start = reader.position();\n");
    }
    out.push_str("        Ok(Self {\n");
    for field in &fields {
        let identifier = rust_ident(&to_snake(&field.name));
        let value = decode_field_expr(field, enums)?;
        let _ = writeln!(out, "            {identifier}: {value},");
    }
    out.push_str("        })\n    }\n}\n\n");
    Ok(out)
}

fn generate_packet_impl(packet: &PacketIr) -> String {
    let direction = match packet.direction.as_str() {
        "ToServer" => "PacketDirection::ToServer",
        "ToClient" => "PacketDirection::ToClient",
        "Both" => "PacketDirection::Both",
        _ => "PacketDirection::None",
    };
    let channel = match packet.channel.as_str() {
        "Chunks" => "NetworkChannel::Chunks",
        "WorldMap" => "NetworkChannel::WorldMap",
        "Voice" => "NetworkChannel::Voice",
        _ => "NetworkChannel::Default",
    };
    let max_size = u32::try_from(packet.max_size).unwrap_or(u32::MAX);
    format!(
        "impl Packet for {} {{\n\
         \x20   const ID: u32 = {};\n\
         \x20   const CHANNEL: NetworkChannel = {channel};\n\
         \x20   const DIRECTION: PacketDirection = {direction};\n\
         \x20   const COMPRESSED: bool = {};\n\
         \x20   const MAX_SIZE: u32 = {max_size};\n\
         }}\n\n",
        packet.name, packet.packet_id, packet.compressed
    )
}

fn encode_fixed_field(field: &FieldIr, enums: &BTreeMap<String, &EnumIr>) -> Result<String> {
    let identifier = rust_ident(&to_snake(&field.name));
    let offset = field.fixed_offset.unwrap_or(0) as usize;
    let size = field.fixed_size.unwrap_or(1) as usize;
    let mut out = String::new();

    if field.kind == "string" {
        let value = if field.nullable {
            "value.as_str().to_string()".to_string()
        } else {
            format!("self.{identifier}.as_str()")
        };
        if field.nullable {
            out.push_str(&format!(
                "        if let Some(value) = &self.{identifier} {{\n"
            ));
        }
        let encoding = field
            .string
            .as_ref()
            .map(|string| string.encoding.as_str())
            .unwrap_or("UTF-8");
        let helper = if encoding == "ASCII" {
            "write_fixed_ascii_at"
        } else {
            "write_fixed_utf8_at"
        };
        out.push_str(&format!(
            "        {helper}(&mut fixed, {offset}, {value}, {size})?;\n"
        ));
        if field.nullable {
            out.push_str("        }\n");
        }
        return Ok(out);
    }

    if field.nullable {
        out.push_str(&format!(
            "        if let Some(value) = &self.{identifier} {{\n"
        ));
    }
    let value = if field.nullable {
        "value".to_string()
    } else {
        format!("self.{identifier}")
    };
    match field.kind.as_str() {
        "primitive" => {
            let (write, _) = primitive_wire(&field.field_type)?;
            let argument = if field.nullable {
                format!("*{value}")
            } else {
                value
            };
            out.push_str(&format!(
                "        {write}_at(&mut fixed, {offset}, {argument})?;\n"
            ));
        }
        "uuid" => {
            let argument = if field.nullable {
                format!("*{value}")
            } else {
                value
            };
            out.push_str(&format!(
                "        write_uuid_at(&mut fixed, {offset}, {argument})?;\n"
            ));
        }
        "enum" => {
            out.push_str(&format!(
                "        {value}.write_at(&mut fixed, {offset}, {size})?;\n"
            ));
        }
        "struct" => {
            out.push_str(&format!(
                "        let mut nested = Writer::new();\n\
                 \x20       {value}.encode(&mut nested)?;\n\
                 \x20       write_slice_at(&mut fixed, {offset}, nested.as_slice())?;\n"
            ));
        }
        other => {
            return Err(GenError::Ir(format!(
                "fixed field {} has unsupported kind {other}",
                field.name
            )));
        }
    }
    if field.nullable {
        out.push_str("        }\n");
    }
    let _ = enums;
    Ok(out)
}

fn encode_variable_field(field: &FieldIr, enums: &BTreeMap<String, &EnumIr>) -> Result<String> {
    let identifier = rust_ident(&to_snake(&field.name));
    let slot = field.variable_offset_slot.map(|slot| slot as usize);
    let mut out = String::new();
    if field.nullable {
        out.push_str(&format!(
            "        if let Some(value) = &self.{identifier} {{\n"
        ));
        if let Some(slot) = slot {
            out.push_str(&format!(
                "            write_i32_at(&mut fixed, {slot}, vars.len() as i32)?;\n"
            ));
        }
        out.push_str(&encode_variable_value(field, "value", "vars", 12, enums)?);
        out.push_str("        } else {\n");
        if let Some(slot) = slot {
            out.push_str(&format!(
                "            write_i32_at(&mut fixed, {slot}, -1)?;\n"
            ));
        }
        out.push_str("        }\n");
    } else {
        if let Some(slot) = slot {
            out.push_str(&format!(
                "        write_i32_at(&mut fixed, {slot}, vars.len() as i32)?;\n"
            ));
        }
        out.push_str(&encode_variable_value(
            field,
            &format!("self.{identifier}"),
            "vars",
            8,
            enums,
        )?);
    }
    Ok(out)
}

fn encode_variable_value(
    field: &FieldIr,
    value: &str,
    writer: &str,
    indent: usize,
    enums: &BTreeMap<String, &EnumIr>,
) -> Result<String> {
    let pad = " ".repeat(indent);
    let name = field_name(field);
    let mut out = String::new();
    match field.kind.as_str() {
        "string" => {
            let string = field.string.as_ref().ok_or_else(|| {
                GenError::Ir(format!("string field {} has no metadata", field.name))
            })?;
            let max = string.max_length.unwrap_or(4_096_000);
            let helper = if string.encoding == "ASCII" {
                "write_var_ascii_string"
            } else {
                "write_var_string"
            };
            out.push_str(&format!(
                "{pad}{writer}.{helper}(\"{name}\", {value}.as_str(), {max}usize)?;\n"
            ));
        }
        "array" => {
            let array = field.array.as_ref().ok_or_else(|| {
                GenError::Ir(format!("array field {} has no metadata", field.name))
            })?;
            let element = array_element_expr(&array.element_type, "element", writer, enums)?;
            out.push_str(&format!(
                "{pad}{writer}.write_varint({value}.len() as u32)?;\n\
                 {pad}for element in {value}.iter() {{\n\
                 {pad}    {element}\n\
                 {pad}}}\n"
            ));
        }
        "map" => {
            let map = field
                .map
                .as_ref()
                .ok_or_else(|| GenError::Ir(format!("map field {} has no metadata", field.name)))?;
            let max = map.max_length.unwrap_or(4_096_000);
            let value_expr = map_value_expr(&map.value_type, "map_value", writer, enums)?;
            out.push_str(&format!(
                "{pad}{writer}.write_varint({value}.len() as u32)?;\n\
                 {pad}for (key, map_value) in {value}.iter() {{\n\
                 {pad}    {writer}.write_var_string(\"key\", key, {max}usize)?;\n\
                 {pad}    {value_expr}\n\
                 {pad}}}\n"
            ));
        }
        "struct" => {
            out.push_str(&format!("{pad}{value}.encode(&mut {writer})?;\n"));
        }
        "uuid" => {
            out.push_str(&format!("{pad}{writer}.write_uuid({value});\n"));
        }
        "enum" => {
            out.push_str(&format!("{pad}{value}.write({writer}, 1)?;\n"));
        }
        other => {
            return Err(GenError::Ir(format!(
                "variable field {} has unsupported kind {other}",
                field.name
            )));
        }
    }
    Ok(out)
}

fn array_element_expr(
    element_type: &str,
    value: &str,
    writer: &str,
    enums: &BTreeMap<String, &EnumIr>,
) -> Result<String> {
    if element_type == "byte" {
        return Ok(format!("{writer}.write_u8(*{value} as u8);"));
    }
    if let Some((write, deref)) = primitive_wire_optional(element_type) {
        return Ok(format!(
            "{writer}.{write}({}{value});",
            if deref { "*" } else { "" }
        ));
    }
    if enums.contains_key(element_type) {
        return Ok(format!("{value}.write({writer}, 1)?;"));
    }
    if element_type == "String" {
        return Ok(format!(
            "{writer}.write_var_string(\"element\", {value}.as_str(), 4_096_000usize)?;"
        ));
    }
    Ok(format!("{value}.encode(&mut {writer})?"))
}

fn map_value_expr(
    value_type: &str,
    value: &str,
    writer: &str,
    enums: &BTreeMap<String, &EnumIr>,
) -> Result<String> {
    if let Some((write, deref)) = primitive_wire_optional(value_type) {
        return Ok(format!(
            "{writer}.{write}({}{value});",
            if deref { "*" } else { "" }
        ));
    }
    if enums.contains_key(value_type) {
        return Ok(format!("{value}.write({writer}, 1)?;"));
    }
    if value_type == "String" {
        return Ok(format!(
            "{writer}.write_var_string(\"value\", {value}.as_str(), 4_096_000usize)?;"
        ));
    }
    Ok(format!("{value}.encode(&mut {writer})?"))
}

fn decode_field_expr(field: &FieldIr, enums: &BTreeMap<String, &EnumIr>) -> Result<String> {
    if field.variable_order.is_some() || field.fixed_offset.is_none() {
        return decode_variable_field(field, enums);
    }
    let offset = field.fixed_offset.unwrap_or(0) as usize;
    if field.nullable {
        let bit = field_nullable_index(field)?;
        let present = format!("bits.is_set({bit})?");
        let decode = decode_fixed_value(field, offset)?;
        return Ok(format!("if {present} {{ Some({decode}) }} else {{ None }}"));
    }
    decode_fixed_value(field, offset)
}

fn decode_fixed_value(field: &FieldIr, offset: usize) -> Result<String> {
    let size = field.fixed_size.unwrap_or(1) as usize;
    let name = field_name(field);
    Ok(match field.kind.as_str() {
        "primitive" => {
            let read = primitive_read_optional(&field.field_type).ok_or_else(|| {
                GenError::Ir(format!("unsupported primitive type {}", field.field_type))
            })?;
            format!("read_at(&fixed, {offset}, |reader| reader.{read}())?")
        }
        "uuid" => format!("read_at(&fixed, {offset}, |reader| reader.read_uuid())?"),
        "enum" => format!(
            "{}::read_at(&fixed, {offset}, {size})?",
            field
                .enum_type
                .as_deref()
                .ok_or_else(|| GenError::Ir(format!("enum field {} has no type", field.name)))?
        ),
        "struct" => format!(
            "read_at(&fixed, {offset}, |reader| {}::decode(reader))?",
            field
                .nested_type
                .as_deref()
                .ok_or_else(|| GenError::Ir(format!("struct field {} has no type", field.name)))?
        ),
        "string" => {
            let encoding = field
                .string
                .as_ref()
                .map(|string| string.encoding.as_str())
                .unwrap_or("UTF-8");
            if encoding == "ASCII" {
                format!("read_at(&fixed, {offset}, |reader| reader.read_fixed_ascii_string(\"{name}\", {size}))?")
            } else {
                format!("read_at(&fixed, {offset}, |reader| reader.read_fixed_string({size}))?")
            }
        }
        other => {
            return Err(GenError::Ir(format!(
                "fixed field {} has unsupported kind {other}",
                field.name
            )));
        }
    })
}

fn decode_variable_field(field: &FieldIr, enums: &BTreeMap<String, &EnumIr>) -> Result<String> {
    let name = field_name(field);
    let slot = field.variable_offset_slot.map(|slot| slot as usize);
    let decode = decode_variable_value(field, "reader", enums)?;
    let mut out = String::new();
    out.push_str("{\n");
    if field.nullable {
        let bit = field_nullable_index(field)?;
        out.push_str(&format!("            if bits.is_set({bit})? {{\n"));
        out.push_str(&check_slot(slot, true, &name));
        out.push_str(&format!("                let value = {decode};\n"));
        out.push_str("                Some(value)\n");
        out.push_str("            } else {\n");
        out.push_str(&check_slot(slot, false, &name));
        out.push_str("                None\n");
        out.push_str("            }\n");
    } else {
        out.push_str(&check_slot(slot, true, &name));
        out.push_str(&format!("            {decode}\n"));
    }
    out.push_str("        }");
    Ok(out)
}

fn check_slot(slot: Option<usize>, present: bool, name: &str) -> String {
    let Some(slot) = slot else {
        return String::new();
    };
    if present {
        format!(
            "                let expected = (reader.position() - var_start) as i32;\n\
             \x20               let found = read_at(&fixed, {slot}, |reader| reader.read_i32())?;\n\
             \x20               if found != expected {{ return Err(crate::io::ProtocolError::NonCanonicalOffset {{ field: \"{name}\", found, expected }}); }}\n"
        )
    } else {
        format!(
            "                let found = read_at(&fixed, {slot}, |reader| reader.read_i32())?;\n\
             \x20               if found != -1 {{ return Err(crate::io::ProtocolError::NonCanonicalOffset {{ field: \"{name}\", found, expected: -1 }}); }}\n"
        )
    }
}

fn decode_variable_value(
    field: &FieldIr,
    reader: &str,
    enums: &BTreeMap<String, &EnumIr>,
) -> Result<String> {
    let name = field_name(field);
    Ok(match field.kind.as_str() {
        "string" => {
            let string = field.string.as_ref().ok_or_else(|| {
                GenError::Ir(format!("string field {} has no metadata", field.name))
            })?;
            let max = string.max_length.unwrap_or(4_096_000);
            if string.encoding == "ASCII" {
                format!("{reader}.read_var_ascii_string(\"{name}\", {max}usize)?")
            } else {
                format!("{reader}.read_var_string(\"{name}\", {max}usize)?")
            }
        }
        "array" => {
            let array = field.array.as_ref().ok_or_else(|| {
                GenError::Ir(format!("array field {} has no metadata", field.name))
            })?;
            let max = array.max_length.unwrap_or(4_096_000);
            let element = decode_element_expr(&array.element_type, enums)?;
            format!("{reader}.read_array(\"{name}\", {max}usize, |reader| {element})?")
        }
        "map" => {
            let map = field
                .map
                .as_ref()
                .ok_or_else(|| GenError::Ir(format!("map field {} has no metadata", field.name)))?;
            let max = map.max_length.unwrap_or(4_096_000);
            let value = decode_map_value_expr(&map.value_type, enums)?;
            format!(
                "{{\n\
                 \x20               let count = {reader}.read_length(\"{name}\", {max}usize)?;\n\
                 \x20               let mut mapped = BTreeMap::new();\n\
                 \x20               for _ in 0..count {{\n\
                 \x20                   let key = {reader}.read_var_string(\"key\", 4_096_000usize)?;\n\
                 \x20                   let map_value = {value};\n\
                 \x20                   if mapped.insert(key.clone(), map_value).is_some() {{ return Err(crate::io::ProtocolError::DuplicateMapKey {{ field: \"{name}\", key }}); }}\n\
                 \x20               }}\n\
                 \x20               mapped\n\
                 \x20           }}"
            )
        }
        "struct" => {
            let nested = field
                .nested_type
                .as_deref()
                .ok_or_else(|| GenError::Ir(format!("struct field {} has no type", field.name)))?;
            format!("{nested}::decode({reader})?")
        }
        "uuid" => format!("{reader}.read_uuid()?"),
        "enum" => {
            let enumeration = field
                .enum_type
                .as_deref()
                .ok_or_else(|| GenError::Ir(format!("enum field {} has no type", field.name)))?;
            format!("{enumeration}::read({reader}, 1)?")
        }
        other => {
            return Err(GenError::Ir(format!(
                "variable field {} has unsupported kind {other}",
                field.name
            )));
        }
    })
}

fn decode_element_expr(element_type: &str, enums: &BTreeMap<String, &EnumIr>) -> Result<String> {
    if element_type == "byte" {
        return Ok("reader.read_i8()".to_string());
    }
    if let Some(read) = primitive_read_optional(element_type) {
        return Ok(format!("reader.{read}()"));
    }
    if enums.contains_key(element_type) {
        return Ok(format!("{element_type}::read(reader, 1)?"));
    }
    if element_type == "String" {
        return Ok("reader.read_var_string(\"element\", 4_096_000usize)?".to_string());
    }
    Ok(format!("{element_type}::decode(reader)"))
}

fn decode_map_value_expr(value_type: &str, enums: &BTreeMap<String, &EnumIr>) -> Result<String> {
    if let Some(read) = primitive_read_optional(value_type) {
        return Ok(format!("reader.{read}()?"));
    }
    if enums.contains_key(value_type) {
        return Ok(format!("{value_type}::read(reader, 1)?"));
    }
    if value_type == "String" {
        return Ok("reader.read_var_string(\"value\", 4_096_000usize)?".to_string());
    }
    Ok(format!("{value_type}::decode(reader)?"))
}

fn rust_type(field: &FieldIr, enums: &BTreeMap<String, &EnumIr>) -> Result<String> {
    let base = match field.kind.as_str() {
        "primitive" => primitive_type(&field.field_type)?,
        "uuid" => "Uuid".to_string(),
        "string" => "String".to_string(),
        "enum" => field
            .enum_type
            .clone()
            .ok_or_else(|| GenError::Ir(format!("enum field {} has no type", field.name)))?,
        "struct" => field
            .nested_type
            .clone()
            .ok_or_else(|| GenError::Ir(format!("struct field {} has no type", field.name)))?,
        "array" => {
            let array = field.array.as_ref().ok_or_else(|| {
                GenError::Ir(format!("array field {} has no metadata", field.name))
            })?;
            format!("Vec<{}>", element_rust_type(&array.element_type, enums)?)
        }
        "map" => {
            let map = field
                .map
                .as_ref()
                .ok_or_else(|| GenError::Ir(format!("map field {} has no metadata", field.name)))?;
            format!(
                "BTreeMap<String, {}>",
                element_rust_type(&map.value_type, enums)?
            )
        }
        other => {
            return Err(GenError::Ir(format!(
                "field {} has unsupported kind {other}",
                field.name
            )));
        }
    };
    Ok(if field.nullable {
        format!("Option<{base}>")
    } else {
        base
    })
}

fn element_rust_type(name: &str, enums: &BTreeMap<String, &EnumIr>) -> Result<String> {
    if let Some(ty) = primitive_type_optional(name) {
        return Ok(ty.to_string());
    }
    if enums.contains_key(name)
        || name == "String"
        || name.chars().next().is_some_and(char::is_uppercase)
    {
        return Ok(name.to_string());
    }
    Err(GenError::Ir(format!("unsupported element type {name}")))
}

fn default_expr(field: &FieldIr, enums: &BTreeMap<String, &EnumIr>) -> Result<String> {
    if field.nullable {
        return Ok("None".to_string());
    }
    let _ = enums;
    Ok(match field.kind.as_str() {
        "primitive" => match field.field_type.as_str() {
            "boolean" | "Boolean" => "false".to_string(),
            "float" => "0.0".to_string(),
            "double" => "0.0".to_string(),
            _ => "0".to_string(),
        },
        "uuid" => "Uuid::default()".to_string(),
        "string" => "String::new()".to_string(),
        "enum" | "struct" => "Default::default()".to_string(),
        "array" => "Vec::new()".to_string(),
        "map" => "BTreeMap::new()".to_string(),
        other => {
            return Err(GenError::Ir(format!(
                "field {} has unsupported default kind {other}",
                field.name
            )));
        }
    })
}

fn primitive_type(field_type: &str) -> Result<String> {
    primitive_type_optional(field_type)
        .map(str::to_string)
        .ok_or_else(|| GenError::Ir(format!("unsupported primitive type {field_type}")))
}

fn primitive_type_optional(field_type: &str) -> Option<&'static str> {
    Some(match field_type {
        "byte" => "i8",
        "short" => "i16",
        "int" => "i32",
        "long" => "i64",
        "float" => "f32",
        "double" => "f64",
        "boolean" | "Boolean" => "bool",
        _ => return None,
    })
}

fn primitive_wire(field_type: &str) -> Result<(&'static str, bool)> {
    primitive_wire_optional(field_type)
        .ok_or_else(|| GenError::Ir(format!("unsupported primitive type {field_type}")))
}

fn primitive_wire_optional(field_type: &str) -> Option<(&'static str, bool)> {
    Some(match field_type {
        "byte" => ("write_i8", true),
        "short" => ("write_i16", true),
        "int" => ("write_i32", true),
        "long" => ("write_i64", true),
        "float" => ("write_f32", true),
        "double" => ("write_f64", true),
        "boolean" | "Boolean" => ("write_bool", true),
        _ => return None,
    })
}

fn primitive_read_optional(field_type: &str) -> Option<&'static str> {
    Some(match field_type {
        "byte" => "read_i8",
        "short" => "read_i16",
        "int" => "read_i32",
        "long" => "read_i64",
        "float" => "read_f32",
        "double" => "read_f64",
        "boolean" | "Boolean" => "read_bool",
        _ => return None,
    })
}

fn field_nullable_index(field: &FieldIr) -> Result<usize> {
    let bit = field
        .nullable_bit
        .ok_or_else(|| GenError::Ir(format!("nullable field {} has no bit", field.name)))?;
    Ok(usize::try_from(field.nullable_byte.unwrap_or(0) * 8 + bit).unwrap_or(usize::MAX))
}

fn field_name(field: &FieldIr) -> String {
    field.name.clone()
}

fn to_snake(name: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = name.chars().collect();
    for (index, character) in chars.iter().enumerate() {
        if character.is_uppercase() {
            let previous = index.checked_sub(1).and_then(|i| chars.get(i));
            let next = chars.get(index + 1);
            let previous_is_lower_or_digit = previous
                .map(|value| value.is_lowercase() || value.is_ascii_digit())
                .unwrap_or(false);
            if index > 0 && (previous_is_lower_or_digit || upper_run_end(previous, next)) {
                out.push('_');
            }
        }
        out.push(character.to_ascii_lowercase());
    }
    out
}

fn upper_run_end(previous: Option<&char>, next: Option<&char>) -> bool {
    previous.map(|value| value.is_uppercase()).unwrap_or(false)
        && next.map(|value| value.is_lowercase()).unwrap_or(false)
}

fn rust_ident(name: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
        "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
        "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
        "use", "where", "while", "async", "await", "dyn", "abstract", "become", "box", "do",
        "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
    ];
    if KEYWORDS.contains(&name) {
        format!("r#{name}")
    } else {
        name.to_string()
    }
}
