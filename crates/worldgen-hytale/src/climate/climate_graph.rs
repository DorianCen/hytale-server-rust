// Ported from: `.hytale-source/.../worldgen/climate/ClimateGraph.java`
//! Java `climate.ClimateGraph` — the 512×512 climate lookup table built by
//! hierarchical weighted-voronoi in (temperature, intensity) space.

use std::collections::HashMap;
use std::rc::Rc;

use hytale_math::math_util;

use crate::climate::climate_type::ClimateType;
use crate::climate::distance_transform;
use crate::climate::maps::{DoubleMap, IntMap};

/// Java `ClimateGraph.RESOLUTION`.
pub const RESOLUTION: i32 = 512;

/// Java private `ONE_MINUS_EPS = 1.0 - MathUtil.EPSILON_DOUBLE`.
const ONE_MINUS_EPS: f64 = 1.0 - math_util::EPSILON_DOUBLE;

/// Java `ClimateGraph.FadeMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FadeMode {
    /// Java `PARENTS`.
    Parents,
    /// Java `CHILDREN`.
    Children,
}

/// Java `climate.ClimateGraph`.
pub struct ClimateGraph {
    /// Java `private final double width`.
    width: f64,
    /// Java `private final double height`.
    height: f64,
    /// Java `private final double fadeRadius`.
    fade_radius: f64,
    /// Java `private final double fadeDistance`.
    fade_distance: f64,
    /// Java `private final FadeMode fadeMode`.
    fade_mode: FadeMode,
    /// Java `private final ClimateType[] parents`.
    parents: Vec<Rc<ClimateType>>,
    /// Java `private final ClimateType[] children`.
    children: Vec<Rc<ClimateType>>,
    /// Java `private final ClimateType[] id2TypeLookup`.
    id2_type_lookup: Vec<Rc<ClimateType>>,
    /// Java `private final IntMap table`.
    table: IntMap,
    /// Java `private final DoubleMap fade`.
    fade: DoubleMap,
    /// Java `private final Object2IntMap<ClimateType> type2IdLookup`
    /// (Java uses identity equals — keyed by pointer here).
    type2_id_lookup: HashMap<usize, i32>,
}

impl ClimateGraph {
    /// Java `ClimateGraph(int resolution, ClimateType[] parents, FadeMode,
    /// double fadeRadius, double fadeDistance)`.
    pub fn new(
        resolution: i32,
        parents: Vec<Rc<ClimateType>>,
        fade_mode: FadeMode,
        fade_radius: f64,
        fade_distance: f64,
    ) -> Self {
        let table = IntMap::new(resolution, resolution);
        let fade = DoubleMap::new(resolution, resolution);
        let width = resolution as f64 * ONE_MINUS_EPS;
        let height = resolution as f64 * ONE_MINUS_EPS;

        // Java: walk all parents counting every visited type
        let mut type_count = 0usize;
        ClimateType::walk_all(&parents, &mut |_| type_count += 1);

        let mut graph = Self {
            width,
            height,
            fade_radius,
            fade_distance,
            fade_mode,
            children: Vec::with_capacity(type_count - parents.len()),
            id2_type_lookup: Vec::with_capacity(type_count),
            parents,
            table,
            fade,
            type2_id_lookup: HashMap::with_capacity(type_count),
        };

        let mut type_index = 0i32;
        let mut child_index = 0usize;
        let parents = graph.parents.clone();
        // explicit counters mirror the Java index arithmetic — children ids
        // depend on the walk order, not just the loop position
        #[allow(clippy::explicit_counter_loop)]
        for parent in &parents {
            graph.id2_type_lookup.push(parent.clone());
            graph
                .type2_id_lookup
                .insert(Rc::as_ptr(parent) as usize, type_index);
            type_index += 1;

            ClimateType::walk_all(&parent.children, &mut |climate| {
                graph.children.push(climate.clone());
                graph.id2_type_lookup.push(climate.clone());
                graph
                    .type2_id_lookup
                    .insert(Rc::as_ptr(climate) as usize, type_index);
                type_index += 1;
                child_index += 1;
            });
        }

        graph.populate_table();
        graph
    }

    /// Java `fadeRadius()`.
    pub fn fade_radius(&self) -> f64 {
        self.fade_radius
    }

    /// Java `fadeDistance()`.
    pub fn fade_distance(&self) -> f64 {
        self.fade_distance
    }

    /// Java `fadeMode()`.
    pub fn fade_mode(&self) -> FadeMode {
        self.fade_mode
    }

    /// Java `refresh()`.
    pub fn refresh(&mut self) {
        self.populate_table();
    }

    /// Java `getParents()`.
    pub fn get_parents(&self) -> &[Rc<ClimateType>] {
        &self.parents
    }

    /// Java `getChildren()`.
    pub fn get_children(&self) -> &[Rc<ClimateType>] {
        &self.children
    }

    /// Java `getTable()`.
    pub fn get_table(&self) -> &IntMap {
        &self.table
    }

    /// Java `getFade()`.
    pub fn get_fade_map(&self) -> &DoubleMap {
        &self.fade
    }

    /// Java `indexOf(double x, double y)`.
    pub fn index_of(&self, x: f64, y: f64) -> usize {
        let cx = math_util::floor(x * self.width);
        let cy = math_util::floor(y * self.height);
        self.table.index(cx, cy)
    }

    /// Java `getId(int index)`.
    pub fn get_id_at(&self, index: usize) -> i32 {
        self.table.at_index(index)
    }

    /// Java `getFade(int index)`.
    pub fn get_fade_at(&self, index: usize) -> f64 {
        self.fade.at_index(index) * self.fade_distance
    }

    /// Java `getFadeRaw(int index)`.
    pub fn get_fade_raw_at(&self, index: usize) -> f64 {
        self.fade.at_index(index)
    }

    /// Java `getId(double x, double y)`.
    pub fn get_id(&self, x: f64, y: f64) -> i32 {
        let cx = math_util::floor(x * self.width);
        let cy = math_util::floor(y * self.height);
        self.table.at(cx, cy)
    }

    /// Java `getFade(double x, double y)`.
    pub fn get_fade(&self, x: f64, y: f64) -> f64 {
        self.get_fade_raw(x, y) * self.fade_distance
    }

    /// Java `getFadeRaw(double x, double y)`.
    pub fn get_fade_raw(&self, x: f64, y: f64) -> f64 {
        let cx = math_util::floor(x * self.width);
        let cy = math_util::floor(y * self.height);
        self.fade.at(cx, cy)
    }

    /// Java `getType(double x, double y)`.
    pub fn get_type_at(&self, x: f64, y: f64) -> Rc<ClimateType> {
        let cx = math_util::floor(x * self.width);
        let cy = math_util::floor(y * self.height);
        let id = self.table.at(cx, cy);
        self.get_type(id)
    }

    /// Java `validate(int id)`.
    pub fn validate(&self, id: i32) -> bool {
        (id & crate::climate::climate_type::MASK) < self.id2_type_lookup.len() as i32
    }

    /// Java `getType(int id)`.
    pub fn get_type(&self, id: i32) -> Rc<ClimateType> {
        self.id2_type_lookup[(id & crate::climate::climate_type::MASK) as usize].clone()
    }

    /// Java `private void populateTable(IntMap, DoubleMap)`.
    fn populate_table(&mut self) {
        if self.fade_mode == FadeMode::Parents {
            for x in 0..self.table.width {
                for y in 0..self.table.height {
                    self.populate_pixel(x, y, false);
                }
            }
            distance_transform::apply(&self.table, &mut self.fade, self.fade_radius);
        }

        for x in 0..self.table.width {
            for y in 0..self.table.height {
                self.populate_pixel(x, y, true);
            }
        }

        if self.fade_mode == FadeMode::Children {
            distance_transform::apply(&self.table, &mut self.fade, self.fade_radius);
        }
    }

    /// Java `private void populatePixel(int x, int y, IntMap table, boolean recursive)`.
    fn populate_pixel(&mut self, x: i32, y: i32, recursive: bool) {
        let mut selected: Option<Rc<ClimateType>> = None;
        let mut climates: &[Rc<ClimateType>] = &self.parents;

        while !climates.is_empty() {
            let mut nearest: Option<&Rc<ClimateType>> = None;
            let mut min_dist1 = f64::MAX;
            let mut min_dist2 = f64::MAX;

            for climate in climates {
                for point in &climate.points {
                    let dx = x as f64 - point.temperature * self.width;
                    let dy = y as f64 - point.intensity * self.height;
                    let dist2 = math_util::length_squared2_f64(dx, dy) / point.modifier;

                    if dist2 < min_dist1 {
                        min_dist2 = min_dist1;
                        min_dist1 = dist2;
                        nearest = Some(climate);
                    } else if dist2 < min_dist2 {
                        min_dist2 = dist2;
                    }
                }
            }

            let nearest = match nearest {
                Some(n) => n,
                None => break,
            };

            selected = Some(nearest.clone());
            climates = &nearest.children;

            if !recursive {
                break;
            }
        }

        let selected = match selected {
            Some(s) => s,
            None => {
                self.table.set(x, y, -1);
                return;
            }
        };

        let id = self
            .type2_id_lookup
            .get(&(Rc::as_ptr(&selected) as usize))
            .copied()
            .unwrap_or(-1);
        self.table.set(x, y, id);
    }
}
