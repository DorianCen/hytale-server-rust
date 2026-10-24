//! `UpdateTagPatterns` (id 84) encoder.
//!
//! Mirrors Java `TagPatternPacketGenerator.generateInitPacket` and the
//! `toPacket()` implementations in `EqualsTagOp`, `AndPatternOp`,
//! `OrPatternOp` and `NotPatternOp`. Pattern keys are emitted by the
//! `TagPattern` store id; `Equals` resolves its `tagIndex` through the same
//! [`TagRegistry`] that produces `ServerTags`.

use std::collections::BTreeMap;

use hytale_protocol::generated::assets::UpdateTagPatterns;
use hytale_protocol::generated::common::{TagPattern, TagPatternType, UpdateType};

use crate::packets::{invalid, next_id, AssetPacket, AssetPacketError, Result, TagRegistry};
use crate::registry::{AssetRegistry, TagPatternNode};

/// Encode the `UpdateTagPatterns` init packet from the registry snapshot.
pub(crate) fn tag_pattern_packet(
    registry: &AssetRegistry,
    tags: &TagRegistry,
) -> Result<AssetPacket> {
    let mut patterns = BTreeMap::new();
    for record in registry.tag_patterns.iter() {
        let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
            store: "TagPattern",
            key: record.key.clone(),
        })?;
        patterns.insert(id, encode_node(&record.value.pattern, tags)?);
    }

    let packet = UpdateTagPatterns {
        r#type: UpdateType::Init,
        max_id: next_id(&registry.tag_patterns, "TagPattern")?,
        patterns: Some(patterns),
    };
    AssetPacket::encode("TagPattern", &packet)
}

fn encode_node(node: &TagPatternNode, tags: &TagRegistry) -> Result<TagPattern> {
    Ok(match node {
        TagPatternNode::Equals { tag } => TagPattern {
            r#type: TagPatternType::Equals,
            tag_index: tags
                .index(tag)
                .ok_or_else(|| invalid("TagPattern", tag, "tag is missing from ServerTags"))?,
            operands: None,
            not: None,
        },
        TagPatternNode::And { operands } => TagPattern {
            r#type: TagPatternType::And,
            tag_index: 0,
            operands: Some(encode_operands(operands, tags)?),
            not: None,
        },
        TagPatternNode::Or { operands } => TagPattern {
            r#type: TagPatternType::Or,
            tag_index: 0,
            operands: Some(encode_operands(operands, tags)?),
            not: None,
        },
        TagPatternNode::Not { pattern } => TagPattern {
            r#type: TagPatternType::Not,
            tag_index: 0,
            operands: None,
            not: Some(Box::new(encode_node(pattern, tags)?)),
        },
    })
}

fn encode_operands(operands: &[TagPatternNode], tags: &TagRegistry) -> Result<Vec<TagPattern>> {
    operands
        .iter()
        .map(|operand| encode_node(operand, tags))
        .collect()
}
