// Ported from: `.hytale-source/.../CoreServer/.../prefab/selection/mask/BlockPattern.java`
//! Java `com.hypixel.hytale.server.core.prefab.selection.mask.BlockPattern` —
//! a weighted list of block names (`"50%Rock_Stone,Rock_Sandstone"`).

use hytale_math::fast_random::FastRandom;
use worldgen_model::block_key::BlockEntry;
use worldgen_model::registry::Registries;

use crate::prefab::mask::block_filter::parse_block;
use crate::weighted_map::{IWeightedMap, WeightedMap};

/// Java `public class BlockPattern`.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockPattern {
    /// Java `private final IWeightedMap<String> weightedMap` — the block
    /// names and their chances.
    pub weighted_map: WeightedMap<String>,
    /// Java `private IWeightedMap<Integer> resolvedWeightedMap` — the
    /// resolved block ids (lazily built by [`BlockPattern::resolve`]).
    pub resolved_weighted_map: Option<WeightedMap<i32>>,
    /// Java `private IWeightedMap<BlockEntry> resolvedWeightedMapBtk`.
    pub resolved_weighted_map_btk: Option<WeightedMap<BlockEntry>>,
    /// Java `private boolean hasInvalidBlocks`.
    pub has_invalid_blocks: bool,
}

impl BlockPattern {
    /// Java `public static final String CHANCE_SUFFIX` = `"%"`.
    pub const CHANCE_SUFFIX: char = '%';
    /// Java `private static final double DEFAULT_CHANCE` = `100`.
    pub const DEFAULT_CHANCE: f64 = 100.0;
    /// Java `private static final String BLOCK_SEPARATOR` = `","`.
    pub const BLOCK_SEPARATOR: char = ',';
    /// Java `private static final String ALT_BLOCK_SEPARATOR` = `";"`.
    pub const ALT_BLOCK_SEPARATOR: char = ';';

    /// Java `public BlockPattern(final IWeightedMap<String> weightedMap)`.
    pub fn new(weighted_map: WeightedMap<String>) -> Self {
        Self {
            weighted_map,
            resolved_weighted_map: None,
            resolved_weighted_map_btk: None,
            has_invalid_blocks: false,
        }
    }

    /// Java `public static BlockPattern parse(@Nonnull String str)`.
    pub fn parse(text: &str) -> Self {
        if text.is_empty() || text == "Empty" {
            return Self::new(Self::parse_block_pattern(&["Empty"]));
        }
        let mut text = text.to_string();
        if text.to_lowercase().contains("filler=") {
            // Java: the filler triplet uses commas, so it is temporarily
            // replaced by semicolons before the pattern is split.
            text = replace_filler_triplet(&text);
        }
        let text = text.replace(
            Self::ALT_BLOCK_SEPARATOR,
            &Self::BLOCK_SEPARATOR.to_string(),
        );
        let parts: Vec<&str> = text.split(Self::BLOCK_SEPARATOR).collect();
        Self::new(Self::parse_block_pattern(&parts))
    }

    /// Java `private static IWeightedMap<String> parseBlockPattern(@Nonnull
    /// final String... blocksArgs)`.
    fn parse_block_pattern(blocks_args: &[&str]) -> WeightedMap<String> {
        let mut builder = WeightedMap::<String>::builder();
        for block_arg in blocks_args {
            if block_arg.is_empty() {
                continue;
            }
            let mut chance = Self::DEFAULT_CHANCE;
            let mut block_arg = *block_arg;
            let parts: Vec<&str> = block_arg.split(Self::CHANCE_SUFFIX).collect();
            if parts.len() > 1 {
                chance = match parts[0].parse::<f64>() {
                    Ok(value) => value,
                    Err(_) => panic!("Invalid Chance Value: {}", parts[0]),
                };
                block_arg = parts[1];
            }
            builder.put(block_arg.to_string(), chance);
        }
        builder.build()
    }

    /// Java `public boolean isEmpty()`.
    pub fn is_empty(&self) -> bool {
        self.weighted_map.size() == 0
    }

    /// Java `public void resolve()` — resolves the block names against the
    /// registries.
    ///
    /// Deviation: the Java code expands the *reference blocks* (the blocks
    /// that stand for a list of other blocks); the golden registry dump
    /// carries no such data, so the expansion is not ported.
    pub fn resolve(&mut self, registries: &Registries) {
        if self.resolved_weighted_map.is_some() {
            return;
        }
        let mut map_builder = WeightedMap::<i32>::builder();
        let mut key_builder = WeightedMap::<BlockEntry>::builder();
        let mut has_invalid = false;
        self.weighted_map
            .for_each_entry(&mut |block_name: &String, weight: f64| {
                let block_id = parse_block(registries, block_name);
                if block_id == 0 {
                    let base_name = match block_name.find('|') {
                        Some(index) => &block_name[..index],
                        None => block_name.as_str(),
                    };
                    if !base_name.eq_ignore_ascii_case("Empty") {
                        has_invalid = true;
                    }
                }
                map_builder.put(block_id, weight);
                if let Some(key) = try_parse_block_type_key(block_name) {
                    key_builder.put(key, weight);
                }
            });
        self.has_invalid_blocks = has_invalid;
        self.resolved_weighted_map = Some(map_builder.build());
        self.resolved_weighted_map_btk = Some(key_builder.build());
    }

    /// Java `public boolean hasInvalidBlocks()`.
    pub fn has_invalid_blocks(&self) -> bool {
        self.has_invalid_blocks
    }

    /// Java `public int nextBlock(final Random random)`.
    pub fn next_block(&self, random: &mut FastRandom) -> Option<i32> {
        self.resolved_weighted_map
            .as_ref()
            .and_then(|map| map.get_random(random).copied())
    }

    /// Java `@Nullable public BlockEntry nextBlockTypeKey(final Random
    /// random)`.
    pub fn next_block_type_key(&self, random: &mut FastRandom) -> Option<BlockEntry> {
        self.resolved_weighted_map_btk
            .as_ref()
            .and_then(|map| map.get_random(random).cloned())
    }

    /// Java `@Deprecated public int firstBlock()`.
    pub fn first_block(&self) -> i32 {
        self.resolved_weighted_map
            .as_ref()
            .and_then(|map| map.internal_keys().first().copied())
            .unwrap_or(0)
    }
}

/// Java `public static BlockEntry tryParseBlockTypeKey(String blockText)`.
fn try_parse_block_type_key(block_text: &str) -> Option<BlockEntry> {
    let text = block_text.replace(';', ",");
    // Java catches every exception of the decode and returns `null`.
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| BlockEntry::decode(&text)));
    result.ok()
}

/// Java `FILLER_TEMP_REMOVER_PATTERN` — the regex
/// `(Filler=-?\d+),(-?\d+),(-?\d+)`, replaced with `$1;$2;$3`.
fn replace_filler_triplet(text: &str) -> String {
    let bytes: Vec<char> = text.chars().collect();
    let mut result = String::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if let Some((end, x, y, z)) = match_filler_triplet(&bytes, index) {
            result.push_str(&format!("Filler={x};{y};{z}"));
            index = end;
            continue;
        }
        result.push(bytes[index]);
        index += 1;
    }
    result
}

/// Matches the Java regex `(Filler=-?\d+),(-?\d+),(-?\d+)` at `start`.
fn match_filler_triplet(chars: &[char], start: usize) -> Option<(usize, i32, i32, i32)> {
    const PREFIX: &str = "Filler=";
    if start + PREFIX.len() > chars.len() {
        return None;
    }
    let prefix: String = chars[start..start + PREFIX.len()].iter().collect();
    if prefix != PREFIX {
        return None;
    }
    let mut index = start + PREFIX.len();
    let x = read_int(chars, &mut index, true)?;
    if index >= chars.len() || chars[index] != ',' {
        return None;
    }
    index += 1;
    let y = read_int(chars, &mut index, true)?;
    if index >= chars.len() || chars[index] != ',' {
        return None;
    }
    index += 1;
    let z = read_int(chars, &mut index, true)?;
    Some((index, x, y, z))
}

/// Reads a (possibly signed) integer from `chars` at `index`.
fn read_int(chars: &[char], index: &mut usize, signed: bool) -> Option<i32> {
    let start = *index;
    if signed && *index < chars.len() && (chars[*index] == '-' || chars[*index] == '+') {
        *index += 1;
    }
    let digits_start = *index;
    while *index < chars.len() && chars[*index].is_ascii_digit() {
        *index += 1;
    }
    if *index == digits_start {
        return None;
    }
    let text: String = chars[start..*index].iter().collect();
    text.parse::<i32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_block_pattern_keeps_its_name() {
        let pattern = BlockPattern::parse("Rock_Stone");
        assert_eq!(pattern.weighted_map.size(), 1);
        assert_eq!(pattern.weighted_map.internal_keys(), &["Rock_Stone"]);
    }

    #[test]
    fn the_chance_is_parsed() {
        let pattern = BlockPattern::parse("50%Rock_Stone,Rock_Sandstone");
        assert_eq!(pattern.weighted_map.size(), 2);
        let mut entries = Vec::new();
        pattern
            .weighted_map
            .for_each_entry(&mut |name: &String, weight: f64| {
                entries.push((name.clone(), weight));
            });
        assert_eq!(
            entries,
            vec![
                ("Rock_Stone".to_string(), 50.0),
                ("Rock_Sandstone".to_string(), 100.0)
            ]
        );
    }

    #[test]
    fn the_empty_pattern_is_the_empty_block() {
        let pattern = BlockPattern::parse("");
        assert_eq!(pattern.weighted_map.size(), 1);
        assert_eq!(pattern.weighted_map.internal_keys(), &["Empty"]);
    }

    #[test]
    fn the_filler_triplet_is_split_by_the_java_normalisation() {
        // Java quirk: the "filler fix" turns the commas of the filler
        // triplet into semicolons, but the normalisation that follows turns
        // them back into commas — so the triplet *is* split. (Verified
        // against the Java regex behaviour.)
        let pattern = BlockPattern::parse("Rock_Stone|Yaw=90|Filler=1,2,3");
        assert_eq!(pattern.weighted_map.size(), 3);
        assert_eq!(
            pattern.weighted_map.internal_keys(),
            &["Rock_Stone|Yaw=90|Filler=1", "2", "3"]
        );
    }
}
