// Ported from: `.hytale-source/.../worldgen/zone/ZoneGeneratorResult.java`
//! Java `zone.ZoneGeneratorResult` — the per-column (zone, border distance)
//! result produced by the (laterported) `ZonePatternGenerator`; re-used as a
//! mutable scratch record by callers (Java mutates it via the setters).
use std::rc::Rc;

use crate::zone::zone::Zone;

/// Java `zone.ZoneGeneratorResult`.
#[derive(Debug, Clone, Default)]
pub struct ZoneGeneratorResult {
    /// Java `protected Zone zone` — nullable (the empty constructor leaves it
    /// unset; Java callers re-use instances as scratch via the setters).
    pub zone: Option<Rc<Zone>>,
    /// Java `protected double borderDistance`.
    pub border_distance: f64,
}

impl ZoneGeneratorResult {
    /// Java `ZoneGeneratorResult()`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Java `ZoneGeneratorResult(Zone zone, double borderDistance)`.
    pub fn new_with(zone: Option<Rc<Zone>>, border_distance: f64) -> Self {
        Self {
            zone,
            border_distance,
        }
    }

    /// Java `void setZone(Zone zone)`.
    pub fn set_zone(&mut self, zone: Option<Rc<Zone>>) {
        self.zone = zone;
    }

    /// Java `void setBorderDistance(double borderDistance)`.
    pub fn set_border_distance(&mut self, border_distance: f64) {
        self.border_distance = border_distance;
    }

    /// Java `Zone getZone()`.
    pub fn get_zone(&self) -> Option<&Rc<Zone>> {
        self.zone.as_ref()
    }

    /// Java `double getBorderDistance()`.
    pub fn get_border_distance(&self) -> f64 {
        self.border_distance
    }
}
