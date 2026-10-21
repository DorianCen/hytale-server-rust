//! Environment section: one environment id per block, stored as vertical runs.
//!
//! Ported from `CoreServer/.../world/chunk/section/EnvironmentSection.java`.
//!
//! The storage format is a palette plus uniform/column/dedup modes. Java writes
//! palette keys as asset-name strings; until the environment registry lands this
//! port writes numeric runtime ids as varints and reads the same form. The
//! structural tags, run records and packed dedup refs match Java.

use std::collections::BTreeSet;

use hytale_protocol::io::{Reader, Writer};

use super::{Result, SectionError};

const SIZE: usize = 32;
const COLUMNS: usize = SIZE * SIZE;
const BLOCKS: usize = COLUMNS * SIZE;
const STORAGE_UNIFORM: u8 = 0;
const STORAGE_COLUMNS: u8 = 1;
const STORAGE_DEDUP: u8 = 2;

/// Wire mode from `SectionEnvironmentsMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SectionEnvironmentsMode {
    /// One environment for the whole section.
    Uniform = 0,
    /// One run list per column.
    Columns = 1,
}

/// Vertical environment runs for one 32³ section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentSection {
    uniform: Option<i32>,
    values: Option<Vec<i32>>,
}

impl Default for EnvironmentSection {
    fn default() -> Self {
        Self::new()
    }
}

impl EnvironmentSection {
    /// Java `EnvironmentSection()` — uniform unknown environment.
    pub fn new() -> Self {
        Self::uniform(0)
    }

    /// Java `EnvironmentSection(int environmentId)`.
    pub fn uniform(environment_id: i32) -> Self {
        Self {
            uniform: Some(environment_id),
            values: None,
        }
    }

    /// True when the section is a single uniform value.
    pub fn is_uniform(&self) -> bool {
        self.uniform.is_some()
    }

    /// Uniform value, when uniform.
    pub fn uniform_value(&self) -> Option<i32> {
        self.uniform
    }

    /// Java `get(int x, int y, int z)` (only the low five y bits are used).
    pub fn get(&self, x: i32, y: i32, z: i32) -> i32 {
        match (&self.uniform, &self.values) {
            (Some(value), _) => *value,
            (None, Some(values)) => values[index_column(x, z) * SIZE + (y as usize & 31)],
            _ => 0,
        }
    }

    /// Java `set(int x, int y, int z, int environmentId)`.
    pub fn set(&mut self, x: i32, y: i32, z: i32, environment_id: i32) -> bool {
        self.replace_range(
            index_column(x, z),
            y as usize & 31,
            y as usize & 31,
            environment_id,
        )
    }

    /// Java `setRange(int x, int z, int minY, int maxY, int environmentId)`.
    pub fn set_range(
        &mut self,
        x: i32,
        z: i32,
        min_y: i32,
        max_y: i32,
        environment_id: i32,
    ) -> Result<bool> {
        if min_y > max_y || (min_y >> 5) != (max_y >> 5) {
            return Err(SectionError::InvalidData(
                "environment range crosses a section boundary",
            ));
        }
        Ok(self.replace_range(
            index_column(x, z),
            min_y as usize & 31,
            max_y as usize & 31,
            environment_id,
        ))
    }

    /// Java `setColumn(int x, int z, int environmentId)`.
    pub fn set_column(&mut self, x: i32, z: i32, environment_id: i32) -> bool {
        self.replace_range(index_column(x, z), 0, SIZE - 1, environment_id)
    }

    /// Java `setUniform(int environmentId)`.
    pub fn set_uniform(&mut self, environment_id: i32) -> bool {
        if self.uniform == Some(environment_id) {
            return false;
        }
        self.uniform = Some(environment_id);
        self.values = None;
        true
    }

    /// True when any block uses `environment_id`.
    pub fn contains(&self, environment_id: i32) -> bool {
        if let Some(value) = self.uniform {
            return value == environment_id;
        }
        self.values
            .as_ref()
            .is_some_and(|values| values.contains(&environment_id))
    }

    /// Runs for one column, bottom to top, as `(inclusive max y, id)`.
    pub fn column_runs(&self, x: i32, z: i32) -> Vec<(u8, i32)> {
        if let Some(value) = self.uniform {
            return vec![((SIZE - 1) as u8, value)];
        }
        let column = index_column(x, z);
        let start = column * SIZE;
        let values = &self
            .values
            .as_ref()
            .expect("non-uniform section has values")[start..start + SIZE];
        let mut runs = Vec::new();
        let mut current = values[0];
        for (y, value) in values.iter().enumerate().skip(1) {
            if *value != current {
                runs.push(((y - 1) as u8, current));
                current = *value;
            }
        }
        runs.push(((SIZE - 1) as u8, current));
        runs
    }

    fn replace_range(
        &mut self,
        column: usize,
        min_y: usize,
        max_y: usize,
        environment_id: i32,
    ) -> bool {
        if let Some(value) = self.uniform {
            if value == environment_id {
                return false;
            }
            let mut values = vec![value; BLOCKS];
            values[column * SIZE + min_y..=column * SIZE + max_y].fill(environment_id);
            self.values = Some(values);
            self.uniform = None;
            return true;
        }
        let values = self
            .values
            .as_mut()
            .expect("non-uniform section has values");
        let slice = &mut values[column * SIZE + min_y..=column * SIZE + max_y];
        if slice.iter().all(|value| *value == environment_id) {
            return false;
        }
        slice.fill(environment_id);
        true
    }

    /// Storage bytes (palette + uniform/column/dedup mode).
    pub fn serialize(&self) -> Result<Vec<u8>> {
        let mut writer = Writer::new();
        let Some(values) = &self.values else {
            let environment = self.uniform.unwrap_or(0);
            writer.write_varint(1)?;
            write_palette_entry(&mut writer, environment);
            writer.write_u8(STORAGE_UNIFORM);
            writer.write_varint(0)?;
            return Ok(writer.into_inner());
        };

        let palette: Vec<i32> = values
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut remap = std::collections::BTreeMap::new();
        for (index, environment) in palette.iter().enumerate() {
            remap.insert(*environment, index as u32);
        }
        writer.write_varint(palette.len() as u32)?;
        for environment in &palette {
            write_palette_entry(&mut writer, *environment);
        }
        writer.write_u8(STORAGE_COLUMNS);
        for column in 0..COLUMNS {
            let runs = runs_for(values, column);
            writer.write_u8((runs.len() - 1) as u8);
            for (max_y, _) in &runs[..runs.len() - 1] {
                writer.write_u8(*max_y);
            }
            for (_, environment) in runs {
                writer.write_varint(remap[&environment])?;
            }
        }
        Ok(writer.into_inner())
    }

    /// Java `writeProtocol` — mode byte plus values, without the string palette.
    pub fn serialize_for_packet(&self) -> Result<Vec<u8>> {
        let mut writer = Writer::new();
        if let Some(value) = self.uniform {
            writer.write_u8(SectionEnvironmentsMode::Uniform as u8);
            writer.write_varint(value as u32)?;
            return Ok(writer.into_inner());
        }
        let values = self
            .values
            .as_ref()
            .expect("non-uniform section has values");
        writer.write_u8(SectionEnvironmentsMode::Columns as u8);
        for column in 0..COLUMNS {
            let runs = runs_for(values, column);
            writer.write_u8((runs.len() - 1) as u8);
            for (max_y, _) in &runs[..runs.len() - 1] {
                writer.write_u8(*max_y);
            }
            for (_, environment) in runs {
                writer.write_varint(environment as u32)?;
            }
        }
        Ok(writer.into_inner())
    }

    /// Decode the storage layout.
    pub fn deserialize(bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        let palette_count = reader.read_varint()? as usize;
        if palette_count == 0 || palette_count > BLOCKS {
            return Err(SectionError::InvalidData(
                "environment palette size out of range",
            ));
        }
        let mut palette = Vec::with_capacity(palette_count);
        for _ in 0..palette_count {
            palette.push(read_palette_entry(&mut reader)?);
        }
        let mode = reader.read_u8()?;
        let section = match mode {
            STORAGE_UNIFORM => {
                let index = reader.read_varint()? as usize;
                let value = *palette.get(index).ok_or(SectionError::InvalidData(
                    "environment palette index out of range",
                ))?;
                Self::uniform(value)
            }
            STORAGE_COLUMNS => {
                let mut values = vec![0i32; BLOCKS];
                for column in 0..COLUMNS {
                    read_column_record(&mut reader, &palette, &mut values, column)?;
                }
                Self {
                    uniform: None,
                    values: Some(values),
                }
            }
            STORAGE_DEDUP => {
                let unique_count = reader.read_varint()? as usize;
                if unique_count == 0 || unique_count > COLUMNS {
                    return Err(SectionError::InvalidData(
                        "environment unique column count out of range",
                    ));
                }
                let mut unique = vec![vec![0i32; SIZE]; unique_count];
                for record in &mut unique {
                    read_column_record_into(&mut reader, &palette, record)?;
                }
                let refs = read_packed_refs(&mut reader, unique_count)?;
                let mut values = vec![0i32; BLOCKS];
                for (column, reference) in refs.iter().enumerate() {
                    let source = unique.get(*reference).ok_or(SectionError::InvalidData(
                        "environment column ref out of range",
                    ))?;
                    values[column * SIZE..column * SIZE + SIZE].copy_from_slice(source);
                }
                Self {
                    uniform: None,
                    values: Some(values),
                }
            }
            _ => {
                return Err(SectionError::InvalidData(
                    "unknown environment section mode",
                ))
            }
        };
        if !reader.is_empty() {
            return Err(SectionError::InvalidData(
                "trailing environment section bytes",
            ));
        }
        Ok(section)
    }

    /// Decode the packet layout.
    pub fn deserialize_for_packet(bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        let mode = reader.read_u8()?;
        match mode {
            0 => Ok(Self::uniform(reader.read_varint()? as i32)),
            1 => {
                let mut values = vec![0i32; BLOCKS];
                for column in 0..COLUMNS {
                    let runs = reader.read_u8()? as usize + 1;
                    let mut max_ys = Vec::with_capacity(runs.saturating_sub(1));
                    for _ in 0..runs - 1 {
                        max_ys.push(reader.read_u8()?);
                    }
                    let mut start_y = 0usize;
                    for run in 0..runs {
                        let max_y = max_ys.get(run).copied().unwrap_or((SIZE - 1) as u8) as usize;
                        let value = reader.read_varint()? as i32;
                        values[column * SIZE + start_y..=column * SIZE + max_y].fill(value);
                        start_y = max_y + 1;
                    }
                }
                if !reader.is_empty() {
                    return Err(SectionError::InvalidData(
                        "trailing environment packet bytes",
                    ));
                }
                Ok(Self {
                    uniform: None,
                    values: Some(values),
                })
            }
            other => Err(SectionError::InvalidData(if other == 2 {
                "dedup mode is storage-only"
            } else {
                "unknown environment packet mode"
            })),
        }
    }
}

fn write_palette_entry(writer: &mut Writer, environment: i32) {
    let _ = writer.write_varint(environment as u32);
}

fn read_palette_entry(reader: &mut Reader<'_>) -> Result<i32> {
    Ok(reader.read_varint()? as i32)
}

fn runs_for(values: &[i32], column: usize) -> Vec<(u8, i32)> {
    let start = column * SIZE;
    let mut runs = Vec::new();
    let mut current = values[start];
    for offset in 1..SIZE {
        let value = values[start + offset];
        if value != current {
            runs.push(((offset - 1) as u8, current));
            current = value;
        }
    }
    runs.push(((SIZE - 1) as u8, current));
    runs
}

fn read_column_record(
    reader: &mut Reader<'_>,
    palette: &[i32],
    values: &mut [i32],
    column: usize,
) -> Result<()> {
    let mut record = vec![0i32; SIZE];
    read_column_record_into(reader, palette, &mut record)?;
    values[column * SIZE..column * SIZE + SIZE].copy_from_slice(&record);
    Ok(())
}

fn read_column_record_into(
    reader: &mut Reader<'_>,
    palette: &[i32],
    record: &mut [i32],
) -> Result<()> {
    let runs = reader.read_u8()? as usize + 1;
    let mut max_ys = Vec::with_capacity(runs.saturating_sub(1));
    for _ in 0..runs - 1 {
        max_ys.push(reader.read_u8()?);
    }
    let mut start_y = 0usize;
    for run in 0..runs {
        let max_y = max_ys.get(run).copied().unwrap_or((SIZE - 1) as u8) as usize;
        if max_y >= SIZE || start_y > max_y {
            return Err(SectionError::InvalidData(
                "environment run boundary out of range",
            ));
        }
        let index = reader.read_varint()? as usize;
        let value = *palette.get(index).ok_or(SectionError::InvalidData(
            "environment palette index out of range",
        ))?;
        record[start_y..=max_y].fill(value);
        start_y = max_y + 1;
    }
    Ok(())
}

fn bits_for(unique_count: usize) -> usize {
    if unique_count <= 1 {
        0
    } else {
        (usize::BITS - (unique_count - 1).leading_zeros()) as usize
    }
}

fn read_packed_refs(reader: &mut Reader<'_>, unique_count: usize) -> Result<Vec<usize>> {
    let bits = bits_for(unique_count);
    let mut refs = vec![0usize; COLUMNS];
    if bits == 0 {
        return Ok(refs);
    }
    let mut accumulator = 0usize;
    let mut accumulated = 0usize;
    for slot in &mut refs {
        while accumulated < bits {
            accumulator = (accumulator << 8) | usize::from(reader.read_u8()?);
            accumulated += 8;
        }
        accumulated -= bits;
        let value = (accumulator >> accumulated) & ((1 << bits) - 1);
        if value >= unique_count {
            return Err(SectionError::InvalidData(
                "environment column ref out of range",
            ));
        }
        *slot = value;
    }
    Ok(refs)
}

#[inline]
fn index_column(x: i32, z: i32) -> usize {
    hytale_math::chunk_util::index_column(x, z) as usize
}
