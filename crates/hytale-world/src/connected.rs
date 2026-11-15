//! Server-side connected-block state evaluation.
//!
//! Java's `ConnectedBlockRuleSet` turns a "smart" block (pipes, fences, stairs,
//! roofs) into the concrete shape variant that matches its neighbours. This
//! module ports the patterned rule-set path used by the real `Patterned` pipe
//! assets: templated shapes, face-tag rules and the `And`/`Or`/`Not`/`Shape`
//! rule tree. Other Java rule-set kinds remain explicit no-ops until their
//! evaluator is ported.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use hytale_assets::AssetRegistry;
use hytale_math::fast_random::FastRandom;
use hytale_math::hash_util::hash3;
use serde_json::Value;

use crate::placement::{FlipType, MirrorAxis};
use crate::world::World;

const MAX_UPDATE_DEPTH: usize = 3;

/// Java `ConnectedBlockUpdateMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateMode {
    /// Only evaluate the block that was placed; neighbours are not refreshed.
    PlaceUpdate,
    /// Refresh neighbours before and after evaluating the placed block.
    UpdatePlaceUpdate,
    /// Evaluate the placed block once and never update neighbours.
    IgnoreUpdates,
}

/// One resolved connected-block change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectedChange {
    /// Block coordinate.
    pub x: i32,
    /// Block coordinate.
    pub y: i32,
    /// Block coordinate.
    pub z: i32,
    /// Block id before the update.
    pub before: i32,
    /// Block id after the update.
    pub after: i32,
    /// Rotation tuple index after the update.
    pub rotation: i32,
}

#[derive(Debug, Clone)]
struct Evaluation {
    block_id: i32,
    rotation: i32,
    additional: Vec<([i32; 3], i32, i32)>,
}

impl Evaluation {
    fn single(block_id: i32, rotation: i32) -> Self {
        Self {
            block_id,
            rotation,
            additional: Vec::new(),
        }
    }
}

/// Parsed server-side connected-block rule sets.
#[derive(Debug, Clone, Default)]
pub struct ConnectedBlockCatalog {
    rulesets: BTreeMap<String, RuleSetAsset>,
    templates: BTreeMap<String, CustomTemplateData>,
}

#[derive(Debug, Clone)]
struct RuleSetAsset {
    update_mode: UpdateMode,
    shapes: BTreeMap<String, Shape>,
    patterns: Vec<Pattern>,
}

#[derive(Debug, Clone, Default)]
struct Shape {
    face_tags: FaceTags,
    relative_rotation: i32,
}

#[derive(Debug, Clone)]
struct Pattern {
    rule: Option<Rule>,
    rotations: Vec<i32>,
    transform_with_orientation: bool,
    output_shape_id: String,
}

#[derive(Debug, Clone)]
enum Rule {
    FaceTag {
        position_offset: [i32; 3],
        face_tags: FaceTags,
        all: bool,
    },
    And(Vec<Rule>),
    Or(Vec<Rule>),
    Not(Box<Rule>),
    Shape {
        position_offset: [i32; 3],
        shape_id: String,
        allowed_rotations: Option<Vec<i32>>,
    },
}

#[derive(Debug, Clone, Default)]
struct FaceTags {
    faces: BTreeMap<[i32; 3], Vec<String>>,
}

impl FaceTags {
    fn contains(&self, direction: [i32; 3], tag: &str) -> bool {
        self.faces
            .get(&direction)
            .is_some_and(|tags| tags.iter().any(|value| value == tag))
    }

    fn directions(&self) -> impl Iterator<Item = ([i32; 3], &Vec<String>)> {
        self.faces
            .iter()
            .map(|(direction, tags)| (*direction, tags))
    }
}

#[derive(Debug, Clone)]
struct BlockBinding {
    kind: BindingKind,
    shape_by_block: BTreeMap<i32, String>,
    /// Custom-template output patterns keyed by shape name.
    shape_patterns: BTreeMap<String, Vec<BlockPatternEntry>>,
}

#[derive(Debug, Clone)]
enum BindingKind {
    Patterned { rule_set_id: String },
    CustomTemplate { template_id: String },
    Stair { data: StairLikeData },
    Roof { data: RoofData },
}

#[derive(Debug, Clone, Default)]
struct StairLikeData {
    straight: Option<i32>,
    corner_left: Option<i32>,
    corner_right: Option<i32>,
    inverted_corner_left: Option<i32>,
    inverted_corner_right: Option<i32>,
    material_name: Option<String>,
}

impl StairLikeData {
    fn output(&self, stair: StairType) -> Option<i32> {
        match stair {
            StairType::Straight => self.straight,
            StairType::CornerLeft => self.corner_left,
            StairType::CornerRight => self.corner_right,
            StairType::InvertedCornerLeft => self.inverted_corner_left,
            StairType::InvertedCornerRight => self.inverted_corner_right,
        }
    }

    fn shape_name(stair: StairType) -> &'static str {
        match stair {
            StairType::Straight => "Straight",
            StairType::CornerLeft => "Corner_Left",
            StairType::CornerRight => "Corner_Right",
            StairType::InvertedCornerLeft => "Inverted_Corner_Left",
            StairType::InvertedCornerRight => "Inverted_Corner_Right",
        }
    }
}

#[derive(Debug, Clone, Default)]
struct RoofData {
    regular: StairLikeData,
    hollow: Option<StairLikeData>,
    topper: Option<i32>,
    width: i32,
    material_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StairType {
    Straight,
    CornerLeft,
    CornerRight,
    InvertedCornerLeft,
    InvertedCornerRight,
}

impl StairType {
    fn corner(right: bool) -> Self {
        if right {
            Self::CornerRight
        } else {
            Self::CornerLeft
        }
    }

    fn inverted_corner(right: bool) -> Self {
        if right {
            Self::InvertedCornerRight
        } else {
            Self::InvertedCornerLeft
        }
    }
}

#[derive(Debug, Clone, Default)]
struct CustomTemplateData {
    default_shape: Option<String>,
    dont_update_after_initial_placement: bool,
    shapes: Vec<(String, CustomTemplateShape)>,
}

#[derive(Debug, Clone, Default)]
struct CustomTemplateShape {
    face_tags: FaceTags,
    patterns: Vec<CustomTemplatePattern>,
}

#[derive(Debug, Clone, Default)]
struct CustomTemplatePattern {
    rules: Vec<CustomTemplateRule>,
    transform_orientation: bool,
    transforms: Vec<PatternTransform>,
    only_on_placement: bool,
    only_on_update: bool,
    yaw_offset: i32,
}

/// One Java `PatternRotationDefinition` entry (pattern yaw + mirror axis).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct PatternTransform {
    yaw: i32,
    mirror: MirrorAxis,
}

/// One resolved `BlockPattern` entry (Java `BlockPattern.BlockEntry` + weight).
#[derive(Debug, Clone, PartialEq)]
struct BlockPatternEntry {
    block_id: i32,
    rotation: i32,
    weight: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuleMatch {
    Matched(bool),
    /// Java returns `Optional.empty()` for a missing chunk/section, aborting
    /// the whole pattern evaluation.
    Missing,
}

#[derive(Debug, Clone, Default)]
struct CustomTemplateRule {
    position: [i32; 3],
    include: bool,
    placement_normals: Option<Vec<[i32; 3]>>,
    face_tags: FaceTags,
    shapes: Vec<CustomShapeEntry>,
    block_types: Vec<String>,
}

/// Java `BlockPattern.BlockEntry` used by custom-template shape rules.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CustomShapeEntry {
    name: String,
    rotation: i32,
    filler: i32,
}

struct Context<'a> {
    catalog: &'a ConnectedBlockCatalog,
    assets: &'a AssetRegistry,
    world: &'a World,
    origin: [i32; 3],
    rotation: i32,
}

impl ConnectedBlockCatalog {
    /// Parse the patterned rule sets from the loaded asset registry.
    pub fn from_assets(assets: &AssetRegistry) -> Self {
        let rulesets = assets
            .connected_block_rule_sets
            .iter()
            .filter_map(|record| {
                Some((
                    record.key.to_ascii_lowercase(),
                    parse_rule_set(&record.value)?,
                ))
            })
            .collect();
        let templates = assets
            .custom_connected_block_templates
            .iter()
            .map(|record| {
                (
                    record.key.to_ascii_lowercase(),
                    parse_custom_template(&record.value),
                )
            })
            .collect();
        Self {
            rulesets,
            templates,
        }
    }

    /// Number of parsed rule sets.
    pub fn len(&self) -> usize {
        self.rulesets.len()
    }

    /// Whether no rule sets were parsed.
    pub fn is_empty(&self) -> bool {
        self.rulesets.is_empty()
    }

    /// Resolve one block's current shape id, if it uses a patterned rule set.
    pub fn shape_for_block(&self, assets: &AssetRegistry, block_id: i32) -> Option<String> {
        let binding = block_binding(self, assets, block_id)?;
        binding.shape_by_block.get(&block_id).cloned()
    }

    /// Resolve the block variant that represents `shape_id` for another block
    /// type (used by quick-retype to preserve a pipe/fence shape).
    pub fn shape_variant(
        &self,
        assets: &AssetRegistry,
        block_id: i32,
        shape_id: &str,
    ) -> Option<i32> {
        let binding = block_binding(self, assets, block_id)?;
        binding
            .shape_by_block
            .iter()
            .find_map(|(id, shape)| (shape == shape_id).then_some(*id))
    }

    /// Evaluate and apply the placement/update rules for one changed block and
    /// its bounded neighbourhood.
    #[allow(clippy::too_many_arguments)]
    pub fn apply(
        &self,
        world: &mut World,
        assets: &AssetRegistry,
        x: i32,
        y: i32,
        z: i32,
        rotation: i32,
        placement_normal: [i32; 3],
        is_placement: bool,
    ) -> Vec<ConnectedChange> {
        let mut changes = Vec::new();
        let before = world.get_block(x, y, z);
        let Some(binding) = block_binding(self, assets, before) else {
            self.update_neighbors(world, assets, [x, y, z], placement_normal, &mut changes);
            return changes;
        };
        let update_mode = match &binding.kind {
            BindingKind::Patterned { rule_set_id } => self
                .rulesets
                .get(rule_set_id)
                .map(|rule_set| rule_set.update_mode)
                .unwrap_or(UpdateMode::PlaceUpdate),
            BindingKind::CustomTemplate { .. }
            | BindingKind::Stair { .. }
            | BindingKind::Roof { .. } => UpdateMode::PlaceUpdate,
        };

        if is_placement && update_mode == UpdateMode::UpdatePlaceUpdate {
            self.update_neighbors(world, assets, [x, y, z], placement_normal, &mut changes);
        }

        let current = world.get_block(x, y, z);
        let current_rotation = if is_placement {
            rotation.rem_euclid(64)
        } else {
            world.get_rotation(x, y, z)
        };
        if let Some(evaluation) = self.evaluate(
            assets,
            world,
            [x, y, z],
            current,
            current_rotation,
            placement_normal,
            is_placement,
        ) {
            if (evaluation.block_id != current || evaluation.rotation != current_rotation)
                && world
                    .set_block_full_settings(
                        x,
                        y,
                        z,
                        evaluation.block_id,
                        evaluation.rotation,
                        0,
                        false,
                    )
                    .unwrap_or(false)
            {
                changes.push(ConnectedChange {
                    x,
                    y,
                    z,
                    before: current,
                    after: evaluation.block_id,
                    rotation: evaluation.rotation,
                });
            }
            for (offset, block_id, rotation) in evaluation.additional {
                let coordinate = add([x, y, z], offset);
                let before = world.get_block(coordinate[0], coordinate[1], coordinate[2]);
                if world
                    .set_block_full_settings(
                        coordinate[0],
                        coordinate[1],
                        coordinate[2],
                        block_id,
                        rotation,
                        0,
                        false,
                    )
                    .unwrap_or(false)
                {
                    changes.push(ConnectedChange {
                        x: coordinate[0],
                        y: coordinate[1],
                        z: coordinate[2],
                        before,
                        after: block_id,
                        rotation,
                    });
                }
            }
        }

        if update_mode != UpdateMode::IgnoreUpdates {
            self.update_neighbors(world, assets, [x, y, z], placement_normal, &mut changes);
        }
        changes
    }

    /// Refresh only the neighbours around a removed/changed block (used after
    /// breaking a block, where the origin itself is now air).
    pub fn update_around(
        &self,
        world: &mut World,
        assets: &AssetRegistry,
        x: i32,
        y: i32,
        z: i32,
        placement_normal: [i32; 3],
    ) -> Vec<ConnectedChange> {
        let mut changes = Vec::new();
        self.update_neighbors(world, assets, [x, y, z], placement_normal, &mut changes);
        changes
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate(
        &self,
        assets: &AssetRegistry,
        world: &World,
        coordinate: [i32; 3],
        block_id: i32,
        rotation: i32,
        placement_normal: [i32; 3],
        is_placement: bool,
    ) -> Option<Evaluation> {
        let binding = block_binding(self, assets, block_id)?;
        match &binding.kind {
            BindingKind::Patterned { rule_set_id } => self
                .evaluate_patterned(
                    assets,
                    world,
                    coordinate,
                    block_id,
                    rotation,
                    rule_set_id,
                    &binding,
                )
                .map(|(block_id, rotation)| Evaluation::single(block_id, rotation)),
            BindingKind::CustomTemplate { template_id } => self
                .evaluate_custom_template(
                    assets,
                    world,
                    coordinate,
                    block_id,
                    rotation,
                    template_id,
                    &binding,
                    placement_normal,
                    is_placement,
                )
                .map(|(block_id, rotation)| Evaluation::single(block_id, rotation)),
            BindingKind::Stair { data } => {
                let stair =
                    self.connected_stair_type(assets, world, coordinate, rotation, data, 1)?;
                let shape = StairLikeData::shape_name(stair);
                binding.shape_by_block.iter().find_map(|(id, shape_name)| {
                    (shape_name == shape).then_some(Evaluation::single(*id, rotation))
                })
            }
            BindingKind::Roof { data } => {
                self.evaluate_roof(assets, world, coordinate, rotation, data)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_patterned(
        &self,
        assets: &AssetRegistry,
        world: &World,
        coordinate: [i32; 3],
        block_id: i32,
        rotation: i32,
        rule_set_id: &str,
        binding: &BlockBinding,
    ) -> Option<(i32, i32)> {
        let rule_set = self.rulesets.get(rule_set_id)?;
        let shape_id = binding.shape_by_block.get(&block_id)?;
        rule_set.shapes.get(shape_id)?;

        for pattern in &rule_set.patterns {
            let output_shape = rule_set.shapes.get(&pattern.output_shape_id)?;
            let output_block = binding
                .shape_by_block
                .iter()
                .find_map(|(id, shape)| (shape == &pattern.output_shape_id).then_some(*id))?;

            if let Some(rule) = &pattern.rule {
                for pattern_rotation in &pattern.rotations {
                    let composed = if pattern.transform_with_orientation {
                        compose(rotation, *pattern_rotation)
                    } else {
                        *pattern_rotation
                    };
                    let mut candidate = Context {
                        catalog: self,
                        assets,
                        world,
                        origin: coordinate,
                        rotation: composed,
                    };
                    if rule.check(&mut candidate) {
                        return Some((
                            output_block,
                            compose(composed, output_shape.relative_rotation),
                        ));
                    }
                }
            } else {
                return Some((
                    output_block,
                    compose(rotation, output_shape.relative_rotation),
                ));
            }
        }
        None
    }

    fn update_neighbors(
        &self,
        world: &mut World,
        assets: &AssetRegistry,
        start: [i32; 3],
        placement_normal: [i32; 3],
        changes: &mut Vec<ConnectedChange>,
    ) {
        let mut queue = VecDeque::from([(start, 0usize)]);
        let mut visited = BTreeSet::from([start]);
        while let Some((origin, depth)) = queue.pop_front() {
            let mut desired = Vec::new();
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        if dx == 0 && dy == 0 && dz == 0 {
                            continue;
                        }
                        let coordinate = [origin[0] + dx, origin[1] + dy, origin[2] + dz];
                        if !world
                            .chunk(coordinate[0] >> 5, coordinate[2] >> 5)
                            .is_some()
                        {
                            continue;
                        }
                        if world.get_filler(coordinate[0], coordinate[1], coordinate[2]) != 0 {
                            continue;
                        }
                        let block_id = world.get_block(coordinate[0], coordinate[1], coordinate[2]);
                        let Some(binding) = block_binding(self, assets, block_id) else {
                            continue;
                        };
                        if let BindingKind::CustomTemplate { template_id } = &binding.kind {
                            if self.templates.get(template_id).is_some_and(|template| {
                                template.dont_update_after_initial_placement
                            }) {
                                continue;
                            }
                        }
                        if !binding.shape_by_block.contains_key(&block_id) {
                            continue;
                        }
                        let rotation =
                            world.get_rotation(coordinate[0], coordinate[1], coordinate[2]);
                        if let Some(evaluation) = self.evaluate(
                            assets,
                            world,
                            coordinate,
                            block_id,
                            rotation,
                            placement_normal,
                            false,
                        ) {
                            if evaluation.block_id != block_id
                                || evaluation.rotation != rotation
                                || !evaluation.additional.is_empty()
                            {
                                desired.push((coordinate, block_id, evaluation));
                            }
                        }
                    }
                }
            }

            for (coordinate, before, evaluation) in desired {
                if !visited.insert(coordinate) {
                    continue;
                }
                if world
                    .set_block_full_settings(
                        coordinate[0],
                        coordinate[1],
                        coordinate[2],
                        evaluation.block_id,
                        evaluation.rotation,
                        0,
                        false,
                    )
                    .unwrap_or(false)
                {
                    changes.push(ConnectedChange {
                        x: coordinate[0],
                        y: coordinate[1],
                        z: coordinate[2],
                        before,
                        after: evaluation.block_id,
                        rotation: evaluation.rotation,
                    });
                    if depth + 1 < MAX_UPDATE_DEPTH {
                        queue.push_back((coordinate, depth + 1));
                    }
                }
                for (offset, block_id, rotation) in evaluation.additional {
                    let target = add(coordinate, offset);
                    let before = world.get_block(target[0], target[1], target[2]);
                    if world
                        .set_block_full_settings(
                            target[0], target[1], target[2], block_id, rotation, 0, false,
                        )
                        .unwrap_or(false)
                    {
                        changes.push(ConnectedChange {
                            x: target[0],
                            y: target[1],
                            z: target[2],
                            before,
                            after: block_id,
                            rotation,
                        });
                        if depth + 1 < MAX_UPDATE_DEPTH {
                            queue.push_back((target, depth + 1));
                        }
                    }
                }
            }
        }
    }
}

impl Rule {
    fn check(&self, context: &mut Context<'_>) -> bool {
        match self {
            Self::FaceTag {
                position_offset,
                face_tags,
                all,
            } => face_tag_rule(context, *position_offset, face_tags, *all),
            Self::And(rules) => rules.iter().all(|rule| rule.check(context)),
            Self::Or(rules) => rules.iter().any(|rule| rule.check(context)),
            Self::Not(rule) => !rule.check(context),
            Self::Shape {
                position_offset,
                shape_id,
                allowed_rotations,
            } => context.shape_rule(*position_offset, shape_id, allowed_rotations.as_deref()),
        }
    }
}

impl Context<'_> {
    fn local_block(&self, position: [i32; 3]) -> Option<i32> {
        let coordinate = add(self.origin, apply_rotation(self.rotation, position));
        self.world.chunk(coordinate[0] >> 5, coordinate[2] >> 5)?;
        if self
            .world
            .get_filler(coordinate[0], coordinate[1], coordinate[2])
            != 0
        {
            return None;
        }
        Some(
            self.world
                .get_block(coordinate[0], coordinate[1], coordinate[2]),
        )
    }

    fn local_rotation(&self, position: [i32; 3]) -> Option<i32> {
        let coordinate = add(self.origin, apply_rotation(self.rotation, position));
        self.world.chunk(coordinate[0] >> 5, coordinate[2] >> 5)?;
        if self
            .world
            .get_filler(coordinate[0], coordinate[1], coordinate[2])
            != 0
        {
            return None;
        }
        Some(compose_inverse(
            self.rotation,
            self.world
                .get_rotation(coordinate[0], coordinate[1], coordinate[2]),
        ))
    }

    fn shape_rule(
        &self,
        position_offset: [i32; 3],
        shape_id: &str,
        allowed_rotations: Option<&[i32]>,
    ) -> bool {
        let Some(neighbor) = self.local_block(position_offset) else {
            return false;
        };
        let Some(neighbor_shape) = self.catalog.shape_for_block(self.assets, neighbor) else {
            return false;
        };
        if neighbor_shape != shape_id {
            return false;
        }
        let Some(allowed) = allowed_rotations else {
            return true;
        };
        let Some(rotation) = self.local_rotation(position_offset) else {
            return false;
        };
        let shape_relative = self
            .catalog
            .shape_for_block(self.assets, neighbor)
            .and_then(|shape| {
                self.catalog
                    .shape_relative_rotation(self.assets, neighbor, &shape)
            })
            .unwrap_or(0);
        allowed.contains(&compose(rotation, shape_relative))
    }
}

impl ConnectedBlockCatalog {
    fn shape_relative_rotation(
        &self,
        assets: &AssetRegistry,
        block_id: i32,
        shape_id: &str,
    ) -> Option<i32> {
        let binding = block_binding(self, assets, block_id)?;
        match &binding.kind {
            BindingKind::Patterned { rule_set_id } => Some(
                self.rulesets
                    .get(rule_set_id)?
                    .shapes
                    .get(shape_id)?
                    .relative_rotation,
            ),
            BindingKind::CustomTemplate { .. }
            | BindingKind::Stair { .. }
            | BindingKind::Roof { .. } => Some(0),
        }
    }
}

fn face_tag_rule(
    context: &Context<'_>,
    position_offset: [i32; 3],
    tags: &FaceTags,
    all: bool,
) -> bool {
    let mut all_directions_match = true;
    for (direction, expected) in tags.directions() {
        let lookup = add(position_offset, direction);
        let Some(neighbor) = context.local_block(lookup) else {
            all_directions_match = false;
            continue;
        };
        let Some(shape) = context.catalog.shape_for_block(context.assets, neighbor) else {
            all_directions_match = false;
            continue;
        };
        let Some(relative) =
            context
                .catalog
                .shape_relative_rotation(context.assets, neighbor, &shape)
        else {
            all_directions_match = false;
            continue;
        };
        let Some(face_tags) = context
            .catalog
            .shape_face_tags(context.assets, neighbor, &shape)
        else {
            all_directions_match = false;
            continue;
        };
        let Some(rotation) = context.local_rotation(lookup) else {
            all_directions_match = false;
            continue;
        };
        let mut check_direction = [-direction[0], -direction[1], -direction[2]];
        check_direction = undo_rotation(rotation, check_direction);
        check_direction = undo_rotation(relative, check_direction);
        let matches = !expected.is_empty()
            && expected
                .iter()
                .all(|tag| face_tags.contains(check_direction, tag));
        if matches && !all {
            return true;
        }
        if !matches {
            all_directions_match = false;
        }
    }
    all && all_directions_match
}

impl ConnectedBlockCatalog {
    fn shape_face_tags(
        &self,
        assets: &AssetRegistry,
        block_id: i32,
        shape_id: &str,
    ) -> Option<FaceTags> {
        let binding = block_binding(self, assets, block_id)?;
        match &binding.kind {
            BindingKind::Patterned { rule_set_id } => Some(
                self.rulesets
                    .get(rule_set_id)?
                    .shapes
                    .get(shape_id)?
                    .face_tags
                    .clone(),
            ),
            BindingKind::CustomTemplate { template_id } => Some(
                self.templates
                    .get(template_id)?
                    .shapes
                    .iter()
                    .find(|(name, _)| name == shape_id)?
                    .1
                    .face_tags
                    .clone(),
            ),
            BindingKind::Stair { .. } | BindingKind::Roof { .. } => Some(FaceTags::default()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_custom_template(
        &self,
        assets: &AssetRegistry,
        world: &World,
        coordinate: [i32; 3],
        block_id: i32,
        rotation: i32,
        template_id: &str,
        binding: &BlockBinding,
        placement_normal: [i32; 3],
        is_placement: bool,
    ) -> Option<(i32, i32)> {
        let template = self.templates.get(template_id)?;
        let current_shape = binding.shape_by_block.get(&block_id)?;
        if !template
            .shapes
            .iter()
            .any(|(name, _)| name == current_shape)
        {
            return None;
        }

        for (shape_name, shape) in &template.shapes {
            for pattern in &shape.patterns {
                if (is_placement && pattern.only_on_update)
                    || (!is_placement && pattern.only_on_placement)
                {
                    continue;
                }
                for transform in &pattern.transforms {
                    let total_rotation = if pattern.transform_orientation {
                        add_rotations(rotation, transform.yaw)
                    } else {
                        transform.yaw.rem_euclid(64)
                    };
                    let mut aborted = false;
                    let matched = pattern.rules.iter().all(|rule| {
                        match self.custom_template_rule_matches(
                            assets,
                            world,
                            coordinate,
                            rule,
                            total_rotation,
                            transform.mirror,
                            placement_normal,
                        ) {
                            RuleMatch::Matched(matched) => matched,
                            RuleMatch::Missing => {
                                aborted = true;
                                false
                            }
                        }
                    });
                    if aborted {
                        // Java returns Optional.empty() for an unloaded
                        // neighbour, aborting every remaining transform.
                        return None;
                    }
                    if !matched {
                        continue;
                    }
                    let entries = binding.shape_patterns.get(shape_name)?;
                    let entry = select_block_pattern(entries, coordinate)?;
                    let flip_type = flip_type_for(assets, entry.block_id);
                    let mut result_rotation =
                        add_yaw(entry.rotation, pattern.yaw_offset.rem_euclid(4));
                    if transform.mirror != MirrorAxis::None {
                        result_rotation = with_yaw(
                            result_rotation,
                            flip_type.flip_yaw(result_rotation % 4, transform.mirror),
                        );
                    }
                    result_rotation = add_rotations(result_rotation, total_rotation);
                    // Java's orthogonal upside-down workaround.
                    if (result_rotation / 4) % 4 == 2 && flip_type == FlipType::Orthogonal {
                        result_rotation = add_yaw(result_rotation, -1);
                    }
                    return Some((entry.block_id, result_rotation));
                }
            }
        }

        if let Some(default) = &template.default_shape {
            if let Some(entries) = binding.shape_patterns.get(default) {
                if let Some(entry) = select_block_pattern(entries, coordinate) {
                    return Some((entry.block_id, add_rotations(entry.rotation, rotation)));
                }
            }
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn custom_template_rule_matches(
        &self,
        assets: &AssetRegistry,
        world: &World,
        origin: [i32; 3],
        rule: &CustomTemplateRule,
        total_rotation: i32,
        mirror: MirrorAxis,
        placement_normal: [i32; 3],
    ) -> RuleMatch {
        if let Some(normals) = &rule.placement_normals {
            if !normals.contains(&placement_normal) {
                return RuleMatch::Missing;
            }
        }
        // Java flips the pattern coordinate before rotating it to world space.
        let mut position = rule.position;
        match mirror {
            MirrorAxis::X => position[0] = -position[0],
            MirrorAxis::Z => position[2] = -position[2],
            MirrorAxis::None => {}
        }
        let coordinate = add(origin, apply_rotation(total_rotation, position));
        let Some((neighbor, neighbor_filler)) = local_block_with_filler(world, coordinate) else {
            return RuleMatch::Missing;
        };
        let neighbor_rotation = world.get_rotation(coordinate[0], coordinate[1], coordinate[2]);
        // Java subtracts the total rotation component-wise, un-rotates the
        // filler, then un-mirrors the neighbour's yaw through its FlipType.
        let mut local_rotation = subtract_rotations(neighbor_rotation, total_rotation);
        let mut local_filler = unrotate_filler(neighbor_filler, total_rotation);
        if mirror != MirrorAxis::None {
            let yaw = flip_type_for(assets, neighbor).flip_yaw(local_rotation % 4, mirror);
            local_filler = rotate_filler_yaw(local_filler, yaw - local_rotation % 4);
            local_rotation = with_yaw(local_rotation, yaw);
        }
        let neighbor_shape = self.shape_for_block(assets, neighbor);
        let mut matched = false;

        if !rule.face_tags.faces.is_empty() {
            let Some(shape) = &neighbor_shape else {
                return RuleMatch::Matched(!rule.include);
            };
            let Some(face_tags) = self.shape_face_tags(assets, neighbor, shape) else {
                return RuleMatch::Matched(!rule.include);
            };
            let yaw_only = local_rotation % 4;
            for (direction, tags) in rule.face_tags.directions() {
                let adjusted = undo_rotation(yaw_only, direction);
                if tags.iter().any(|tag| face_tags.contains(adjusted, tag)) {
                    matched = true;
                    break;
                }
            }
        }

        if !rule.shapes.is_empty() {
            if let Some(shape) = neighbor_shape {
                matched |= rule.shapes.iter().any(|entry| {
                    entry.name == *shape
                        && entry.rotation == local_rotation
                        && entry.filler == local_filler
                });
            }
        }

        if !rule.block_types.is_empty() {
            if let Some(record) = assets.block_types.get_by_id(neighbor) {
                matched |= rule.block_types.iter().any(|key| key == &record.key);
            }
        }

        RuleMatch::Matched(if matched { rule.include } else { !rule.include })
    }

    fn connected_stair_type(
        &self,
        assets: &AssetRegistry,
        world: &World,
        coordinate: [i32; 3],
        rotation: i32,
        data: &StairLikeData,
        width: i32,
    ) -> Option<StairType> {
        let current_rotation = rotation.rem_euclid(64);
        let mut current_yaw = current_rotation % 4;
        let upside_down = (current_rotation / 4) % 4 != 0;
        if upside_down {
            current_yaw = (current_yaw + 2) % 4;
        }

        let mut result = StairType::Straight;
        if let Some(connection) = self.inverted_corner_connection(
            assets,
            world,
            coordinate,
            current_yaw,
            upside_down,
            data,
        ) {
            result = connection.get_stair_type(true);
        }
        if let Some(connection) = self.corner_connection(
            assets,
            world,
            coordinate,
            current_rotation,
            current_yaw,
            upside_down,
            data,
            width,
        ) {
            result = connection.get_stair_type(false);
        }

        if upside_down {
            result = match result {
                StairType::CornerLeft => StairType::CornerRight,
                StairType::CornerRight => StairType::CornerLeft,
                StairType::InvertedCornerLeft => StairType::InvertedCornerRight,
                StairType::InvertedCornerRight => StairType::InvertedCornerLeft,
                StairType::Straight => StairType::Straight,
            };
        }
        Some(result)
    }

    fn evaluate_roof(
        &self,
        assets: &AssetRegistry,
        world: &World,
        coordinate: [i32; 3],
        rotation: i32,
        data: &RoofData,
    ) -> Option<Evaluation> {
        let block_id = world.get_block(coordinate[0], coordinate[1], coordinate[2]);
        let width = data.width.max(1);
        let stair = self.roof_stair_type(assets, world, coordinate, block_id, rotation, data)?;

        if let Some(topper) = data.topper {
            if stair == StairType::Straight {
                let below = [coordinate[0], coordinate[1] - 1, coordinate[2]];
                let current = rotation & !3;
                if self.can_be_topper(assets, world, below, current, data) {
                    return Some(Evaluation::single(topper, rotation));
                }
            }
        }

        let hollow = !block_below_supports_up(assets, world, coordinate);
        if hollow {
            if let Some(hollow_data) = &data.hollow {
                if let Some(output) = hollow_data.output(stair) {
                    return Some(Evaluation::single(output, rotation));
                }
            }
        }

        let regular = data.regular.output(stair)?;
        let mut evaluation = Evaluation::single(regular, rotation);
        if let Some(binding) = block_binding(self, assets, block_id) {
            let existing = binding
                .shape_by_block
                .get(&block_id)
                .and_then(|shape| stair_type_from_shape(shape));
            if let Some(existing) = existing {
                if existing != StairType::Straight {
                    let previous_width = if stair_is_left(existing) {
                        -(width - 1)
                    } else if stair_is_right(existing) {
                        width - 1
                    } else {
                        0
                    };
                    let new_width = if stair_is_left(stair) {
                        -(width - 1)
                    } else if stair_is_right(stair) {
                        width - 1
                    } else {
                        0
                    };
                    if new_width != previous_width {
                        let offset = apply_rotation(rotation % 4, [previous_width, 0, 0]);
                        evaluation.additional.push((offset, regular, rotation));
                    }
                }
            }
        }
        Some(evaluation)
    }

    fn roof_stair_type(
        &self,
        assets: &AssetRegistry,
        world: &World,
        coordinate: [i32; 3],
        block_id: i32,
        rotation: i32,
        data: &RoofData,
    ) -> Option<StairType> {
        let current_rotation = rotation.rem_euclid(64);
        let mut current_yaw = current_rotation % 4;
        let upside_down = (current_rotation / 4) % 4 != 0;
        if upside_down {
            current_yaw = (current_yaw + 2) % 4;
        }
        let width = data.width.max(1);

        let mut result = StairType::Straight;
        if let Some(connection) = self.inverted_corner_connection(
            assets,
            world,
            coordinate,
            current_yaw,
            upside_down,
            &data.regular,
        ) {
            if self.is_width_fulfilled(
                assets,
                world,
                coordinate,
                connection,
                current_yaw,
                block_id,
                rotation,
                width,
            ) {
                result = connection.get_stair_type(true);
            }
        }
        if let Some(connection) = self.corner_connection(
            assets,
            world,
            coordinate,
            current_rotation,
            current_yaw,
            upside_down,
            &data.regular,
            width,
        ) {
            if self.is_width_fulfilled(
                assets,
                world,
                coordinate,
                connection,
                current_yaw,
                block_id,
                rotation,
                width,
            ) {
                result = connection.get_stair_type(false);
            }
        }

        if result == StairType::Straight {
            let above = [coordinate[0], coordinate[1] + 1, coordinate[2]];
            if let Some(connection) = self.valley_connection(
                assets,
                world,
                coordinate,
                above,
                &data.regular,
                rotation,
                false,
                block_id,
                width,
            ) {
                result = connection.get_stair_type(true);
            }
        }
        if result == StairType::Straight {
            let below = [coordinate[0], coordinate[1] - 1, coordinate[2]];
            if let Some(connection) = self.valley_connection(
                assets,
                world,
                coordinate,
                below,
                &data.regular,
                rotation,
                true,
                block_id,
                width,
            ) {
                result = connection.get_stair_type(false);
            }
        }

        if upside_down {
            result = match result {
                StairType::CornerLeft => StairType::CornerRight,
                StairType::CornerRight => StairType::CornerLeft,
                StairType::InvertedCornerLeft => StairType::InvertedCornerRight,
                StairType::InvertedCornerRight => StairType::InvertedCornerLeft,
                StairType::Straight => StairType::Straight,
            };
        }
        Some(result)
    }

    #[allow(clippy::too_many_arguments)]
    fn valley_connection(
        &self,
        assets: &AssetRegistry,
        world: &World,
        placement: [i32; 3],
        check: [i32; 3],
        data: &StairLikeData,
        rotation: i32,
        reverse: bool,
        block_id: i32,
        width: i32,
    ) -> Option<StairConnection> {
        let yaw = rotation % 4;
        let back_dir = if reverse { [0, 0, 1] } else { [0, 0, -1] };
        let back_pos = add(check, apply_rotation(yaw, mul(back_dir, width)));
        let back = self.stair_data(assets, world, back_pos, data.material_name.as_deref());
        let inverted = reverse;
        if !valley_connection_compatible(rotation, back, 0, inverted) {
            return None;
        }

        let left_dir = if reverse { [1, 0, 0] } else { [-1, 0, 0] };
        let right_dir = if reverse { [-1, 0, 0] } else { [1, 0, 0] };
        let left = self.stair_data(
            assets,
            world,
            add(check, apply_rotation(yaw, mul(left_dir, width))),
            data.material_name.as_deref(),
        );
        let right = self.stair_data(
            assets,
            world,
            add(check, apply_rotation(yaw, mul(right_dir, width))),
            data.material_name.as_deref(),
        );
        let left_connection = valley_connection_compatible(rotation, left, 1, inverted);
        let right_connection = valley_connection_compatible(rotation, right, 3, inverted);
        if left_connection == right_connection {
            return None;
        }
        let connection = if left_connection {
            StairConnection::Left
        } else {
            StairConnection::Right
        };
        if !self.is_width_fulfilled(
            assets, world, placement, connection, yaw, block_id, rotation, width,
        ) {
            return None;
        }
        Some(connection)
    }

    fn can_be_topper(
        &self,
        assets: &AssetRegistry,
        world: &World,
        below: [i32; 3],
        rotation: i32,
        data: &RoofData,
    ) -> bool {
        let yaw = rotation % 4;
        for (direction, offset) in [
            ([0, 0, -1], 2),
            ([0, 0, 1], 0),
            ([1, 0, 0], 1),
            ([-1, 0, 0], 3),
        ] {
            let coordinate = add(below, apply_rotation(yaw, direction));
            let stair = self.stair_data(assets, world, coordinate, data.material_name.as_deref());
            if !valley_connection_compatible(rotation, stair, offset, true) {
                return false;
            }
        }
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn is_width_fulfilled(
        &self,
        _assets: &AssetRegistry,
        world: &World,
        coordinate: [i32; 3],
        connection: StairConnection,
        yaw: i32,
        block_id: i32,
        rotation: i32,
        width: i32,
    ) -> bool {
        if width <= 1 {
            return true;
        }
        for step in 1..width {
            let direction = if matches!(connection, StairConnection::Left) {
                [-1, 0, 0]
            } else {
                [1, 0, 0]
            };
            let offset = apply_rotation(yaw, mul(direction, step));
            let Some(other) = local_block_at(world, add(coordinate, offset)) else {
                continue;
            };
            let other_rotation = world.get_rotation(
                coordinate[0] + offset[0],
                coordinate[1] + offset[1],
                coordinate[2] + offset[2],
            );
            let other_filler = world.get_filler(
                coordinate[0] + offset[0],
                coordinate[1] + offset[1],
                coordinate[2] + offset[2],
            );
            let required_filler = pack_filler(offset[0], offset[1], offset[2]);
            if !((other_filler == 0 || other_filler == required_filler)
                && other == block_id
                && other_rotation == rotation)
            {
                return false;
            }
        }
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn corner_connection(
        &self,
        assets: &AssetRegistry,
        world: &World,
        coordinate: [i32; 3],
        rotation: i32,
        current_yaw: i32,
        upside_down: bool,
        data: &StairLikeData,
        width: i32,
    ) -> Option<StairConnection> {
        let mut offset = apply_rotation(current_yaw, [0, 0, -width]);
        offset = add(coordinate, offset);
        let mut back = self.stair_data(assets, world, offset, data.material_name.as_deref());
        if back.is_none() && width > 1 {
            let offset = add(
                coordinate,
                apply_rotation(current_yaw, [0, 0, -(width + 1)]),
            );
            back = self.stair_data(assets, world, offset, data.material_name.as_deref());
            if back.is_some_and(|(stair, _)| stair == StairType::Straight) {
                back = None;
            }
        }
        let (other_type, other_rotation) = back?;
        let mut other_yaw = other_rotation % 4;
        let other_upside_down = (other_rotation / 4) % 4 != 0;
        if other_upside_down {
            other_yaw = (other_yaw + 2) % 4;
        }
        if !can_connect_to(current_yaw, other_yaw, upside_down, other_upside_down) {
            return None;
        }
        let sideways = add(coordinate, apply_rotation(other_yaw, [0, 0, 1]));
        if let Some((_, sideways_rotation)) =
            self.stair_data(assets, world, sideways, data.material_name.as_deref())
        {
            if sideways_rotation == rotation {
                return None;
            }
        }
        stair_connection(current_yaw, other_yaw, other_type, false, upside_down)
    }

    fn inverted_corner_connection(
        &self,
        assets: &AssetRegistry,
        world: &World,
        coordinate: [i32; 3],
        current_yaw: i32,
        upside_down: bool,
        data: &StairLikeData,
    ) -> Option<StairConnection> {
        let front = add(coordinate, apply_rotation(current_yaw, [0, 0, 1]));
        let (other_type, other_rotation) =
            self.stair_data(assets, world, front, data.material_name.as_deref())?;
        let mut other_yaw = other_rotation % 4;
        let other_upside_down = (other_rotation / 4) % 4 != 0;
        if other_upside_down {
            other_yaw = (other_yaw + 2) % 4;
        }
        if !can_connect_to(current_yaw, other_yaw, upside_down, other_upside_down) {
            return None;
        }
        stair_connection(current_yaw, other_yaw, other_type, true, upside_down)
    }

    fn stair_data(
        &self,
        assets: &AssetRegistry,
        world: &World,
        coordinate: [i32; 3],
        required_material: Option<&str>,
    ) -> Option<(StairType, i32)> {
        let block_id = local_block_at(world, coordinate)?;
        let binding = block_binding(self, assets, block_id)?;
        let (shape, material) = match &binding.kind {
            BindingKind::Stair { data } => (
                binding.shape_by_block.get(&block_id)?,
                data.material_name.as_deref(),
            ),
            BindingKind::Roof { data } => (
                binding.shape_by_block.get(&block_id)?,
                data.material_name.as_deref(),
            ),
            _ => return None,
        };
        if let (Some(required), Some(other)) = (required_material, material) {
            if required != other {
                return None;
            }
        }
        let stair = stair_type_from_shape(shape)?;
        Some((
            stair,
            world.get_rotation(coordinate[0], coordinate[1], coordinate[2]),
        ))
    }
}

#[derive(Debug, Clone, Copy)]
enum StairConnection {
    Left,
    Right,
}

impl StairConnection {
    fn get_stair_type(self, inverted: bool) -> StairType {
        match (self, inverted) {
            (Self::Left, false) => StairType::CornerLeft,
            (Self::Left, true) => StairType::InvertedCornerLeft,
            (Self::Right, false) => StairType::CornerRight,
            (Self::Right, true) => StairType::InvertedCornerRight,
        }
    }
}

fn stair_type_from_shape(shape: &str) -> Option<StairType> {
    Some(match shape {
        "Straight" => StairType::Straight,
        "Corner_Left" => StairType::CornerLeft,
        "Corner_Right" => StairType::CornerRight,
        "Inverted_Corner_Left" => StairType::InvertedCornerLeft,
        "Inverted_Corner_Right" => StairType::InvertedCornerRight,
        _ => return None,
    })
}

fn can_connect_to(current: i32, other: i32, upside_down: bool, other_upside_down: bool) -> bool {
    other_upside_down == upside_down && other != current && (other + 2) % 4 != current
}

fn stair_connection(
    current_yaw: i32,
    other_yaw: i32,
    other_type: StairType,
    inverted: bool,
    upside_down: bool,
) -> Option<StairConnection> {
    if other_yaw == (current_yaw + 1) % 4
        && other_type != StairType::inverted_corner(upside_down ^ inverted)
        && other_type != StairType::corner(upside_down ^ !inverted)
    {
        Some(StairConnection::Left)
    } else if other_yaw == (current_yaw + 3) % 4
        && other_type != StairType::inverted_corner(upside_down ^ !inverted)
        && other_type != StairType::corner(upside_down ^ inverted)
    {
        Some(StairConnection::Right)
    } else {
        None
    }
}

fn stair_is_left(stair: StairType) -> bool {
    matches!(stair, StairType::CornerLeft | StairType::InvertedCornerLeft)
}

fn stair_is_right(stair: StairType) -> bool {
    matches!(
        stair,
        StairType::CornerRight | StairType::InvertedCornerRight
    )
}

fn stair_is_corner(stair: StairType) -> bool {
    matches!(stair, StairType::CornerLeft | StairType::CornerRight)
}

fn stair_is_inverted(stair: StairType) -> bool {
    matches!(
        stair,
        StairType::InvertedCornerLeft | StairType::InvertedCornerRight
    )
}

fn valley_connection_compatible(
    rotation: i32,
    other: Option<(StairType, i32)>,
    yaw_offset: i32,
    inverted: bool,
) -> bool {
    let Some((other_type, other_rotation)) = other else {
        return false;
    };
    let current_yaw = rotation % 4;
    let current_pitch = (rotation / 4) % 4;
    let other_yaw = other_rotation % 4;
    let other_pitch = (other_rotation / 4) % 4;
    if other_pitch != current_pitch {
        return false;
    }
    if inverted && stair_is_corner(other_type) {
        return false;
    }
    if !inverted && stair_is_inverted(other_type) {
        return false;
    }
    let target = (current_yaw + yaw_offset) % 4;
    let right = if inverted {
        StairType::InvertedCornerRight
    } else {
        StairType::CornerRight
    };
    let left = if inverted {
        StairType::InvertedCornerLeft
    } else {
        StairType::CornerLeft
    };
    other_yaw == target
        || (other_type == right && other_yaw == (target + 1) % 4)
        || (other_type == left && other_yaw == (target + 3) % 4)
}

fn mul(vector: [i32; 3], scale: i32) -> [i32; 3] {
    [vector[0] * scale, vector[1] * scale, vector[2] * scale]
}

fn pack_filler(x: i32, y: i32, z: i32) -> i32 {
    const BITS: i32 = 5;
    const MASK: i32 = (1 << BITS) - 1;
    (x & MASK) | ((z & MASK) << BITS) | ((y & MASK) << (BITS * 2))
}

/// Java `FillerBlockUtil.unpackX/Y/Z` (5-bit sign-extended axes).
fn unpack_filler(value: i32) -> [i32; 3] {
    const BITS: i32 = 5;
    const MASK: i32 = (1 << BITS) - 1;
    let sign_extend = |axis: i32| {
        let value = axis & MASK;
        if value & (1 << (BITS - 1)) != 0 {
            value | (!0 << BITS)
        } else {
            value
        }
    };
    [
        sign_extend(value),
        sign_extend(value >> (BITS * 2)),
        sign_extend(value >> BITS),
    ]
}

/// Java `Rotation.rotateX/Y/Z` on a packed filler.
fn rotate_filler_axis(filler: i32, axis: u8, steps: i32) -> i32 {
    let [x, y, z] = unpack_filler(filler);
    let (x, y, z) = match (axis, steps.rem_euclid(4)) {
        (1, 1) => (x, -z, y),
        (1, 2) => (x, -y, -z),
        (1, 3) => (x, z, -y),
        (2, 1) => (z, y, -x),
        (2, 2) => (-x, y, -z),
        (2, 3) => (-z, y, x),
        (3, 1) => (-y, x, z),
        (3, 2) => (-x, -y, z),
        (3, 3) => (y, -x, z),
        _ => (x, y, z),
    };
    pack_filler(x, y, z)
}

/// Java's filler un-rotation (`tempRotation - neighbourRotation` per axis).
fn unrotate_filler(filler: i32, total_rotation: i32) -> i32 {
    let pitch = -((total_rotation / 4) % 4);
    let yaw = -(total_rotation % 4);
    let roll = -((total_rotation / 16) % 4);
    let filler = rotate_filler_axis(filler, 1, pitch);
    let filler = rotate_filler_axis(filler, 2, yaw);
    rotate_filler_axis(filler, 3, roll)
}

fn rotate_filler_yaw(filler: i32, steps: i32) -> i32 {
    rotate_filler_axis(filler, 2, steps)
}

fn local_block_at(world: &World, coordinate: [i32; 3]) -> Option<i32> {
    world.chunk(coordinate[0] >> 5, coordinate[2] >> 5)?;
    if world.get_filler(coordinate[0], coordinate[1], coordinate[2]) != 0 {
        return None;
    }
    Some(world.get_block(coordinate[0], coordinate[1], coordinate[2]))
}

/// Custom-template rules need the filler cell's block id and packed filler;
/// unlike the patterned/stair paths they do not skip filler cells.
fn local_block_with_filler(world: &World, coordinate: [i32; 3]) -> Option<(i32, i32)> {
    world.chunk(coordinate[0] >> 5, coordinate[2] >> 5)?;
    Some((
        world.get_block(coordinate[0], coordinate[1], coordinate[2]),
        world.get_filler(coordinate[0], coordinate[1], coordinate[2]),
    ))
}

fn block_below_supports_up(assets: &AssetRegistry, world: &World, coordinate: [i32; 3]) -> bool {
    let below = [coordinate[0], coordinate[1] - 1, coordinate[2]];
    let Some(block_id) = local_block_at(world, below) else {
        return false;
    };
    let Some(record) = assets.block_types.get_by_id(block_id) else {
        return false;
    };
    record
        .value
        .json
        .get("Supporting")
        .and_then(|supporting| supporting.get("Up"))
        .and_then(Value::as_array)
        .is_some_and(|entries| !entries.is_empty())
}

fn block_binding(
    catalog: &ConnectedBlockCatalog,
    assets: &AssetRegistry,
    block_id: i32,
) -> Option<BlockBinding> {
    let record = assets.block_types.get_by_id(block_id)?;
    let json = record
        .value
        .json
        .get("ConnectedBlockRuleSet")?
        .as_object()?;
    match json.get("Type").and_then(Value::as_str)? {
        "Patterned" => {
            let rule_set_id = json
                .get("RuleSetId")
                .and_then(Value::as_str)?
                .to_ascii_lowercase();
            catalog.rulesets.get(&rule_set_id)?;
            let mut shape_by_block = BTreeMap::new();
            if let Some(templates) = json.get("TemplatedShapes").and_then(Value::as_object) {
                for (shape_id, output) in templates {
                    if let Some(id) = resolve_output(assets, record, output) {
                        shape_by_block.insert(id, shape_id.clone());
                    }
                }
            }
            Some(BlockBinding {
                kind: BindingKind::Patterned { rule_set_id },
                shape_by_block,
                shape_patterns: BTreeMap::new(),
            })
        }
        "CustomTemplate" => {
            let template_id = json
                .get("TemplateShapeAssetId")
                .and_then(Value::as_str)?
                .to_ascii_lowercase();
            catalog.templates.get(&template_id)?;
            let mut shape_by_block = BTreeMap::new();
            let mut shape_patterns = BTreeMap::new();
            if let Some(outputs) = json
                .get("TemplateShapeBlockPatterns")
                .and_then(Value::as_object)
            {
                for (shape, pattern) in outputs {
                    let Some(pattern) = pattern.as_str() else {
                        continue;
                    };
                    let entries = parse_block_pattern(assets, pattern);
                    if entries.is_empty() {
                        continue;
                    }
                    for entry in &entries {
                        shape_by_block
                            .entry(entry.block_id)
                            .or_insert_with(|| shape.clone());
                    }
                    shape_patterns.insert(shape.clone(), entries);
                }
            }
            Some(BlockBinding {
                kind: BindingKind::CustomTemplate { template_id },
                shape_by_block,
                shape_patterns,
            })
        }
        "Stair" => {
            let data = parse_stair_like(json, record, assets);
            let shape_by_block = stair_shape_map(&data);
            Some(BlockBinding {
                kind: BindingKind::Stair { data },
                shape_by_block,
                shape_patterns: BTreeMap::new(),
            })
        }
        "Roof" => {
            let material_name = json
                .get("MaterialName")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let mut regular = json
                .get("Regular")
                .and_then(Value::as_object)
                .map(|regular| parse_stair_like(regular, record, assets))
                .unwrap_or_default();
            regular.material_name = material_name.clone();
            let mut hollow = json
                .get("Hollow")
                .and_then(Value::as_object)
                .map(|hollow| parse_stair_like(hollow, record, assets));
            if let Some(hollow) = &mut hollow {
                hollow.material_name = material_name.clone();
            }
            let topper = json
                .get("Topper")
                .and_then(|topper| resolve_output(assets, record, topper));
            let mut shape_by_block = stair_shape_map(&regular);
            if let Some(hollow) = &hollow {
                shape_by_block.extend(stair_shape_map(hollow));
            }
            if let Some(topper) = topper {
                shape_by_block.insert(topper, "Topper".to_string());
            }
            Some(BlockBinding {
                kind: BindingKind::Roof {
                    data: RoofData {
                        regular,
                        hollow,
                        topper,
                        width: json.get("Width").and_then(Value::as_i64).unwrap_or(1) as i32,
                        material_name: json
                            .get("MaterialName")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                    },
                },
                shape_by_block,
                shape_patterns: BTreeMap::new(),
            })
        }
        _ => None,
    }
}

fn parse_stair_like(
    value: &serde_json::Map<String, Value>,
    record: &hytale_assets::AssetRecord<hytale_assets::BlockTypeAsset>,
    assets: &AssetRegistry,
) -> StairLikeData {
    let output = |name: &str| {
        value
            .get(name)
            .and_then(|output| resolve_output(assets, record, output))
    };
    StairLikeData {
        straight: output("Straight"),
        corner_left: output("Corner_Left"),
        corner_right: output("Corner_Right"),
        inverted_corner_left: output("Inverted_Corner_Left"),
        inverted_corner_right: output("Inverted_Corner_Right"),
        material_name: value
            .get("MaterialName")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .or_else(|| Some("Stair".to_string())),
    }
}

fn stair_shape_map(data: &StairLikeData) -> BTreeMap<i32, String> {
    let mut map = BTreeMap::new();
    for stair in [
        StairType::Straight,
        StairType::CornerLeft,
        StairType::CornerRight,
        StairType::InvertedCornerLeft,
        StairType::InvertedCornerRight,
    ] {
        if let Some(id) = data.output(stair) {
            map.insert(id, StairLikeData::shape_name(stair).to_string());
        }
    }
    map
}

fn resolve_output(
    assets: &AssetRegistry,
    record: &hytale_assets::AssetRecord<hytale_assets::BlockTypeAsset>,
    value: &Value,
) -> Option<i32> {
    let object = value.as_object()?;
    let implicit_key = record
        .value
        .container_item
        .as_deref()
        .filter(|key| assets.block_types.get(key).is_some())
        .unwrap_or(&record.key);
    let base_key = object
        .get("Block")
        .and_then(Value::as_str)
        .unwrap_or(implicit_key)
        .to_string();
    let base_record = assets.block_types.get(&base_key)?;
    let container = base_record
        .value
        .container_item
        .clone()
        .unwrap_or_else(|| base_record.key.clone());
    let key = match object.get("State").and_then(Value::as_str) {
        Some("default") | None => base_key,
        Some(state) => format!("*{container}_State_Definitions_{state}"),
    };
    assets.block_types.id_of(&key)
}

/// Parse a Java `BlockPattern` string into weighted block entries.
///
/// Java splits on `,` (with `;` as an alternate separator) and `%<chance>`
/// prefixes; each entry may carry `|Yaw=`, `|Pitch=` and `|Roll=` offsets.
/// Filler tuples are not used by custom-template outputs in the release pack.
fn parse_block_pattern(assets: &AssetRegistry, pattern: &str) -> Vec<BlockPatternEntry> {
    let pattern = pattern.replace(';', ",");
    let mut entries = Vec::new();
    for argument in pattern.split(',') {
        let argument = argument.trim();
        if argument.is_empty() {
            continue;
        }
        let (weight, encoded) = match argument.split_once('%') {
            Some((chance, rest)) => match chance.parse::<f64>() {
                Ok(chance) => (chance, rest),
                Err(_) => continue,
            },
            None => (100.0, argument),
        };
        let mut parts = encoded.split('|');
        let Some(key) = parts.next() else {
            continue;
        };
        let key = key.trim();
        // State variants keep their canonical `*` prefix in the registry;
        // plain keys do not. Java accepts both spellings.
        let Some(block_id) = assets
            .block_types
            .id_of(key)
            .or_else(|| assets.block_types.id_of(key.trim_start_matches('*')))
        else {
            continue;
        };
        let mut rotation = 0;
        for part in parts {
            let Some((axis, degrees)) = part.split_once('=') else {
                continue;
            };
            let Ok(degrees) = degrees.trim().parse::<i32>() else {
                continue;
            };
            let step = degrees.rem_euclid(360) / 90;
            rotation += match axis.trim() {
                "Yaw" => step,
                "Pitch" => step * 4,
                "Roll" => step * 16,
                _ => 0,
            };
        }
        entries.push(BlockPatternEntry {
            block_id,
            rotation: rotation.rem_euclid(64),
            weight,
        });
    }
    entries
}

/// Java `BlockPattern.nextBlockTypeKey` with the block-pattern RNG.
fn select_block_pattern(
    entries: &[BlockPatternEntry],
    position: [i32; 3],
) -> Option<&BlockPatternEntry> {
    let first = entries.first()?;
    if entries.len() == 1 {
        return Some(first);
    }
    let sum: f64 = entries.iter().map(|entry| entry.weight).sum();
    let mut random = FastRandom::new(hash3(
        i64::from(position[0]),
        i64::from(position[1]),
        i64::from(position[2]),
    ));
    // Java `WeightedMap.get(random)`: 0.99999 epsilon and 1e-5 comparison.
    let mut remaining = random.next_double().min(0.99999) * sum;
    for entry in entries {
        remaining -= entry.weight;
        if remaining <= 1.0e-5 {
            return Some(entry);
        }
    }
    None
}

fn flip_type_for(assets: &AssetRegistry, block_id: i32) -> FlipType {
    assets
        .block_types
        .get_by_id(block_id)
        .map(|record| FlipType::from_value(record.value.json.get("FlipType")))
        .unwrap_or_default()
}
fn parse_custom_template(value: &Value) -> CustomTemplateData {
    let object = value.as_object();
    let shapes = object
        .and_then(|object| object.get("Shapes"))
        .and_then(Value::as_object)
        .map(|shapes| {
            shapes
                .iter()
                .map(|(name, shape)| (name.clone(), parse_template_shape(shape)))
                .collect()
        })
        .unwrap_or_default();
    CustomTemplateData {
        default_shape: object
            .and_then(|object| object.get("DefaultShape"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        dont_update_after_initial_placement: object
            .and_then(|object| object.get("DontUpdateAfterInitialPlacement"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        shapes,
    }
}

fn parse_template_shape(value: &Value) -> CustomTemplateShape {
    let object = value.as_object();
    CustomTemplateShape {
        face_tags: parse_face_tags(object.and_then(|object| object.get("FaceTags"))),
        patterns: object
            .and_then(|object| object.get("PatternsToMatchAnyOf"))
            .and_then(Value::as_array)
            .map(|patterns| patterns.iter().map(parse_template_pattern).collect())
            .unwrap_or_default(),
    }
}

/// Java `PatternRotationDefinition.getRotations` expansion.
///
/// Index order is the Java `ROTATIONS` list: no mirror, then mirror X, then
/// mirror Z; cardinal rotations follow each mirror's `None` entry.
fn pattern_transforms(cardinal: bool, mirror_x: bool, mirror_z: bool) -> Vec<PatternTransform> {
    let mut transforms = vec![PatternTransform {
        yaw: 0,
        mirror: MirrorAxis::None,
    }];
    if cardinal {
        for yaw in 1..4 {
            transforms.push(PatternTransform {
                yaw,
                mirror: MirrorAxis::None,
            });
        }
    }
    for (enabled, mirror) in [(mirror_x, MirrorAxis::X), (mirror_z, MirrorAxis::Z)] {
        if !enabled {
            continue;
        }
        transforms.push(PatternTransform { yaw: 0, mirror });
        if cardinal {
            for yaw in 1..4 {
                transforms.push(PatternTransform { yaw, mirror });
            }
        }
    }
    transforms
}

fn parse_template_pattern(value: &Value) -> CustomTemplatePattern {
    let object = value.as_object();
    let pattern_rotation = object
        .and_then(|object| object.get("AllowedPatternTransformations"))
        .and_then(Value::as_object);
    let cardinal = pattern_rotation
        .and_then(|rotation| rotation.get("IsCardinallyRotatable"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mirror_x = pattern_rotation
        .and_then(|rotation| rotation.get("MirrorX"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mirror_z = pattern_rotation
        .and_then(|rotation| rotation.get("MirrorZ"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    CustomTemplatePattern {
        rules: object
            .and_then(|object| object.get("RulesToMatch"))
            .and_then(Value::as_array)
            .map(|rules| rules.iter().map(parse_template_rule).collect())
            .unwrap_or_default(),
        transform_orientation: object
            .and_then(|object| object.get("TransformRulesToOrientation"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        transforms: pattern_transforms(cardinal, mirror_x, mirror_z),
        only_on_placement: object
            .and_then(|object| object.get("OnlyOnPlacement"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        only_on_update: object
            .and_then(|object| object.get("OnlyOnUpdate"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        yaw_offset: object
            .and_then(|object| object.get("YawToApplyAddReplacedBlockType"))
            .and_then(Value::as_str)
            .map(rotation_index_for_name)
            .unwrap_or(0),
    }
}

fn parse_template_rule(value: &Value) -> CustomTemplateRule {
    let object = value.as_object();
    let include = object
        .and_then(|object| object.get("IncludeOrExclude"))
        .and_then(Value::as_str)
        != Some("Exclude");
    CustomTemplateRule {
        position: parse_vector3i(object.and_then(|object| object.get("Position"))),
        include,
        placement_normals: object
            .and_then(|object| object.get("PlacementNormals"))
            .and_then(Value::as_array)
            .map(|normals| {
                normals
                    .iter()
                    .filter_map(Value::as_str)
                    .filter_map(normal_for_name)
                    .collect()
            }),
        face_tags: parse_face_tags(object.and_then(|object| object.get("FaceTags"))),
        shapes: object
            .and_then(|object| object.get("Shapes"))
            .and_then(Value::as_array)
            .map(|shapes| {
                shapes
                    .iter()
                    .filter_map(Value::as_str)
                    .map(parse_template_shape_entry)
                    .collect()
            })
            .unwrap_or_default(),
        block_types: object
            .and_then(|object| object.get("BlockTypes"))
            .and_then(Value::as_array)
            .map(|blocks| {
                blocks
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn parse_template_shape_entry(value: &str) -> CustomShapeEntry {
    let mut parts = value.split('|');
    let name = parts.next().unwrap_or(value).to_string();
    let mut rotation: i32 = 0;
    let mut filler = 0;
    for part in parts {
        let Some((axis, value)) = part.split_once('=') else {
            continue;
        };
        if axis.trim() == "Filler" {
            let coordinates = value
                .split(',')
                .filter_map(|coordinate| coordinate.trim().parse::<i32>().ok())
                .collect::<Vec<_>>();
            if let [x, y, z] = coordinates.as_slice() {
                filler = pack_filler(*x, *y, *z);
            }
            continue;
        }
        let step = match value.trim().parse::<i32>().unwrap_or(0).rem_euclid(360) {
            0 => 0,
            90 => 1,
            180 => 2,
            270 => 3,
            _ => continue,
        };
        rotation += match axis.trim() {
            "Yaw" => step,
            "Pitch" => step * 4,
            "Roll" => step * 16,
            _ => 0,
        };
    }
    CustomShapeEntry {
        name,
        rotation: rotation.rem_euclid(64),
        filler,
    }
}

fn rotation_index_for_name(value: &str) -> i32 {
    match value {
        "Ninety" => 1,
        "OneEighty" => 2,
        "TwoSeventy" => 3,
        _ => 0,
    }
}

fn normal_for_name(value: &str) -> Option<[i32; 3]> {
    Some(match value {
        "Up" => [0, 1, 0],
        "Down" => [0, -1, 0],
        "North" => [0, 0, -1],
        "East" => [1, 0, 0],
        "South" => [0, 0, 1],
        "West" => [-1, 0, 0],
        _ => return None,
    })
}

fn parse_rule_set(value: &Value) -> Option<RuleSetAsset> {
    let object = value.as_object()?;
    let shapes = object
        .get("Shapes")
        .and_then(Value::as_object)
        .map(|shapes| {
            shapes
                .iter()
                .filter_map(|(name, shape)| Some((name.clone(), parse_shape(shape)?)))
                .collect()
        })
        .unwrap_or_default();
    let patterns = object
        .get("Patterns")
        .and_then(Value::as_array)
        .map(|patterns| patterns.iter().filter_map(parse_pattern).collect())
        .unwrap_or_default();
    Some(RuleSetAsset {
        update_mode: match object.get("UpdateMode").and_then(Value::as_str) {
            Some("PlaceUpdate") => UpdateMode::PlaceUpdate,
            Some("IgnoreUpdates") => UpdateMode::IgnoreUpdates,
            _ => UpdateMode::UpdatePlaceUpdate,
        },
        shapes,
        patterns,
    })
}

fn parse_shape(value: &Value) -> Option<Shape> {
    let object = value.as_object()?;
    Some(Shape {
        face_tags: parse_face_tags(object.get("FaceTags")),
        relative_rotation: parse_rotation_index(object.get("RelativeRotation")),
    })
}

fn parse_pattern(value: &Value) -> Option<Pattern> {
    let object = value.as_object()?;
    let rotations = object
        .get("RotationTransforms")
        .and_then(Value::as_array)
        .map(|groups| {
            let mut rotations = Vec::new();
            for group in groups {
                for index in rotation_group(Some(group)) {
                    if !rotations.contains(&index) {
                        rotations.push(index);
                    }
                }
            }
            rotations
        })
        .filter(|rotations| !rotations.is_empty())
        .unwrap_or_else(|| vec![0]);
    Some(Pattern {
        rule: object.get("Rule").and_then(parse_rule),
        rotations,
        transform_with_orientation: object
            .get("TransformRulesWithOrientation")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        output_shape_id: object
            .get("Output")
            .and_then(|output| output.get("ShapeId"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

fn parse_rule(value: &Value) -> Option<Rule> {
    let object = value.as_object()?;
    Some(match object.get("Type").and_then(Value::as_str)? {
        "FaceTag" => Rule::FaceTag {
            position_offset: parse_vector3i(object.get("PositionOffset")),
            face_tags: parse_face_tags(object.get("FaceTags")),
            all: object.get("MatchType").and_then(Value::as_str) == Some("All"),
        },
        "And" => Rule::And(
            object
                .get("Rules")
                .and_then(Value::as_array)
                .map(|rules| rules.iter().filter_map(parse_rule).collect())
                .unwrap_or_default(),
        ),
        "Or" => Rule::Or(
            object
                .get("Rules")
                .and_then(Value::as_array)
                .map(|rules| rules.iter().filter_map(parse_rule).collect())
                .unwrap_or_default(),
        ),
        "Not" => Rule::Not(Box::new(parse_rule(object.get("Rule")?)?)),
        "Shape" => Rule::Shape {
            position_offset: parse_vector3i(object.get("PositionOffset")),
            shape_id: object
                .get("ShapeId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            allowed_rotations: object
                .get("AllowedRotations")
                .map(|rotations| rotation_group(Some(rotations)))
                .filter(|rotations| !rotations.is_empty()),
        },
        _ => return None,
    })
}

fn parse_face_tags(value: Option<&Value>) -> FaceTags {
    let Some(object) = value.and_then(Value::as_object) else {
        return FaceTags::default();
    };
    let mut faces = BTreeMap::new();
    for (name, direction) in [
        ("North", [0, 0, -1]),
        ("East", [1, 0, 0]),
        ("South", [0, 0, 1]),
        ("West", [-1, 0, 0]),
        ("Up", [0, 1, 0]),
        ("Down", [0, -1, 0]),
    ] {
        let tags: Vec<String> = object
            .get(name)
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        if !tags.is_empty() {
            faces.insert(direction, tags);
        }
    }
    FaceTags { faces }
}

fn parse_vector3i(value: Option<&Value>) -> [i32; 3] {
    let Some(object) = value.and_then(Value::as_object) else {
        return [0, 0, 0];
    };
    let axis = |name: &str| object.get(name).and_then(Value::as_i64).unwrap_or(0) as i32;
    [axis("X"), axis("Y"), axis("Z")]
}

fn parse_rotation_index(value: Option<&Value>) -> i32 {
    let Some(object) = value.and_then(Value::as_object) else {
        return 0;
    };
    let axis = |name: &str| match object.get(name).and_then(Value::as_str) {
        Some("Ninety") => 1,
        Some("OneEighty") => 2,
        Some("TwoSeventy") => 3,
        _ => 0,
    };
    axis("Roll") * 16 + axis("Pitch") * 4 + axis("Yaw")
}

fn rotation_group(value: Option<&Value>) -> Vec<i32> {
    let Some(object) = value.and_then(Value::as_object) else {
        return vec![0];
    };
    let axis = |name: &str| -> Vec<i32> {
        match object.get(name).and_then(Value::as_str) {
            Some("All") => vec![0, 1, 2, 3],
            Some("Ninety") => vec![1],
            Some("OneEighty") => vec![2],
            Some("TwoSeventy") => vec![3],
            _ => vec![0],
        }
    };
    let mut rotations = Vec::new();
    for yaw in axis("Yaw") {
        for pitch in axis("Pitch") {
            for roll in axis("Roll") {
                let index = roll * 16 + pitch * 4 + yaw;
                if !rotations.contains(&index) {
                    rotations.push(index);
                }
            }
        }
    }
    rotations
}

fn add(left: [i32; 3], right: [i32; 3]) -> [i32; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}

/// Java `Rotation3D.add`: component-wise quarter-turn addition.
fn add_rotations(left: i32, right: i32) -> i32 {
    let yaw = (left % 4 + right % 4).rem_euclid(4);
    let pitch = ((left / 4) % 4 + (right / 4) % 4).rem_euclid(4);
    let roll = ((left / 16) % 4 + (right / 16) % 4).rem_euclid(4);
    yaw + pitch * 4 + roll * 16
}

/// Java `Rotation3D.subtract`: component-wise quarter-turn subtraction.
fn subtract_rotations(left: i32, right: i32) -> i32 {
    let yaw = (left % 4 - right % 4).rem_euclid(4);
    let pitch = ((left / 4) % 4 - (right / 4) % 4).rem_euclid(4);
    let roll = ((left / 16) % 4 - (right / 16) % 4).rem_euclid(4);
    yaw + pitch * 4 + roll * 16
}

/// Add a yaw-only offset (`Rotation.add` on the yaw component).
fn add_yaw(index: i32, yaw: i32) -> i32 {
    with_yaw(index, (index % 4 + yaw).rem_euclid(4))
}

fn with_yaw(index: i32, yaw: i32) -> i32 {
    let pitch = (index / 4) % 4;
    let roll = (index / 16) % 4;
    yaw.rem_euclid(4) + pitch * 4 + roll * 16
}

fn matrix(index: i32) -> [[i32; 3]; 3] {
    let index = index.rem_euclid(64);
    let yaw = index % 4;
    let pitch = (index / 4) % 4;
    let roll = index / 16;
    let (cy, sy) = cos_sin(yaw);
    let (cp, sp) = cos_sin(pitch);
    let (cr, sr) = cos_sin(roll);
    [
        [cy * cr + sy * sp * sr, -cy * sr + sy * sp * cr, sy * cp],
        [cp * sr, cp * cr, -sp],
        [-sy * cr + cy * sp * sr, sy * sr + cy * sp * cr, cy * cp],
    ]
}

fn cos_sin(rotation: i32) -> (i32, i32) {
    match rotation.rem_euclid(4) {
        0 => (1, 0),
        1 => (0, 1),
        2 => (-1, 0),
        _ => (0, -1),
    }
}

fn multiply(a: [[i32; 3]; 3], b: [[i32; 3]; 3]) -> [[i32; 3]; 3] {
    let mut result = [[0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            result[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    result
}

fn transpose(matrix: [[i32; 3]; 3]) -> [[i32; 3]; 3] {
    [
        [matrix[0][0], matrix[1][0], matrix[2][0]],
        [matrix[0][1], matrix[1][1], matrix[2][1]],
        [matrix[0][2], matrix[1][2], matrix[2][2]],
    ]
}

fn matrix_rotation_index(matrix: [[i32; 3]; 3]) -> i32 {
    let sp = -matrix[1][2];
    let pitch = match sp {
        1 => 1,
        -1 => 3,
        _ => 0,
    };
    let (yaw, roll) = if sp != 1 && sp != -1 {
        (
            atan2_90(matrix[0][2], matrix[2][2]),
            atan2_90(matrix[1][0], matrix[1][1]),
        )
    } else {
        (atan2_90(-matrix[2][0], matrix[0][0]), 0)
    };
    roll * 16 + pitch * 4 + yaw
}

fn atan2_90(sin: i32, cos: i32) -> i32 {
    match (sin, cos) {
        (0, 1) => 0,
        (1, 0) => 1,
        (0, -1) => 2,
        (-1, 0) => 3,
        _ => 0,
    }
}

fn compose(a: i32, b: i32) -> i32 {
    if b == 0 {
        a
    } else if a == 0 {
        b
    } else {
        matrix_rotation_index(multiply(matrix(a), matrix(b)))
    }
}

fn compose_inverse(a: i32, b: i32) -> i32 {
    if a == 0 {
        b
    } else {
        matrix_rotation_index(multiply(transpose(matrix(a)), matrix(b)))
    }
}

fn apply_rotation(index: i32, vector: [i32; 3]) -> [i32; 3] {
    let matrix = matrix(index);
    [
        matrix[0][0] * vector[0] + matrix[0][1] * vector[1] + matrix[0][2] * vector[2],
        matrix[1][0] * vector[0] + matrix[1][1] * vector[1] + matrix[1][2] * vector[2],
        matrix[2][0] * vector[0] + matrix[2][1] * vector[1] + matrix[2][2] * vector[2],
    ]
}

fn undo_rotation(index: i32, vector: [i32; 3]) -> [i32; 3] {
    let matrix = transpose(matrix(index));
    [
        matrix[0][0] * vector[0] + matrix[0][1] * vector[1] + matrix[0][2] * vector[2],
        matrix[1][0] * vector[0] + matrix[1][1] * vector[1] + matrix[1][2] * vector[2],
        matrix[2][0] * vector[0] + matrix[2][1] * vector[1] + matrix[2][2] * vector[2],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_composition_matches_java_matrix_order() {
        // Yaw 90 composed with pitch 90.
        let yaw_90 = 1;
        let pitch_90 = 4;
        let composed = compose(yaw_90, pitch_90);
        assert_eq!(
            apply_rotation(composed, [1, 0, 0]),
            apply_rotation(yaw_90, apply_rotation(pitch_90, [1, 0, 0]))
        );
        assert_eq!(compose(0, yaw_90), yaw_90);
        assert_eq!(compose(yaw_90, 0), yaw_90);
    }

    #[test]
    fn inverse_rotation_undoes_the_forward_rotation() {
        for index in 0..64 {
            let vector = [1, -2, 3];
            assert_eq!(undo_rotation(index, apply_rotation(index, vector)), vector);
            assert_eq!(compose_inverse(index, compose(index, 5)), 5);
        }
    }

    #[test]
    fn pattern_transforms_follow_the_java_rotation_definition_order() {
        assert_eq!(
            pattern_transforms(false, false, false),
            vec![PatternTransform {
                yaw: 0,
                mirror: MirrorAxis::None
            }]
        );
        assert_eq!(
            pattern_transforms(true, false, false)
                .iter()
                .map(|transform| transform.yaw)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        assert_eq!(
            pattern_transforms(false, true, false),
            vec![
                PatternTransform {
                    yaw: 0,
                    mirror: MirrorAxis::None
                },
                PatternTransform {
                    yaw: 0,
                    mirror: MirrorAxis::X
                }
            ]
        );
        let transforms = pattern_transforms(true, true, true);
        assert_eq!(transforms.len(), 12, "cardinal + two mirrors");
        assert_eq!(transforms[4].mirror, MirrorAxis::X);
        assert_eq!(transforms[4].yaw, 0);
        assert_eq!(transforms[8].mirror, MirrorAxis::Z);
        assert_eq!(transforms[8].yaw, 0);
        assert_eq!(
            transforms[11],
            PatternTransform {
                yaw: 3,
                mirror: MirrorAxis::Z
            }
        );
    }

    #[test]
    fn rotation_add_and_subtract_are_component_wise() {
        let yaw_90_pitch_180_roll_270 = 1 + 2 * 4 + 3 * 16;
        assert_eq!(
            add_rotations(yaw_90_pitch_180_roll_270, 1),
            2 + 2 * 4 + 3 * 16
        );
        assert_eq!(
            subtract_rotations(yaw_90_pitch_180_roll_270, 1),
            2 * 4 + 3 * 16
        );
        assert_eq!(add_yaw(yaw_90_pitch_180_roll_270, 3), 2 * 4 + 3 * 16);
    }

    #[test]
    fn custom_template_filler_offsets_rotate_with_the_pattern() {
        assert_eq!(pack_filler(-1, 0, 0), 31);
        assert_eq!(unpack_filler(31), [-1, 0, 0]);
        // Inverse yaw 90 on (-1,0,0): (x,z) -> (z,-x) = (0,0,-1).
        let packed = pack_filler(-1, 0, 0);
        assert_eq!(unrotate_filler(packed, 1), pack_filler(0, 0, -1));
        // Mirroring yaw 0 -> 180 flips the filler X axis.
        assert_eq!(rotate_filler_yaw(packed, 2), pack_filler(1, 0, 0));
        assert_eq!(unrotate_filler(packed, 0), packed);
    }

    #[test]
    fn weighted_block_pattern_selection_is_deterministic() {
        let entries = vec![
            BlockPatternEntry {
                block_id: 1,
                rotation: 0,
                weight: 1.0,
            },
            BlockPatternEntry {
                block_id: 2,
                rotation: 0,
                weight: 1.0,
            },
        ];
        let first = select_block_pattern(&entries, [3, 64, 7]).expect("pattern entry");
        let again = select_block_pattern(&entries, [3, 64, 7]).expect("pattern entry");
        assert_eq!(first, again);
        // Different coordinates can select the other entry.
        let different = select_block_pattern(&entries, [4, 64, 7]).expect("pattern entry");
        assert!(different.block_id == 1 || different.block_id == 2);
        assert_eq!(
            select_block_pattern(&entries[..1], [0, 0, 0]).map(|entry| entry.block_id),
            Some(1)
        );
    }
}
