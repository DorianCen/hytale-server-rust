// Ported from: `.hytale-source/.../worldgen/cave/shape/distorted/DistortedShapes.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.shape.distorted.DistortedShapes`
//! — the registry of the distorted shape factories and the factory interface
//! itself.
//!
//! # Java → Rust
//!
//! * `DistortedShape.Factory` (a nested interface with five `create`
//!   overloads) becomes the [`DistortedShapeFactory`] trait; the default
//!   overloads are provided methods with the same parameter interpretation as
//!   the Java code.
//! * `DistortedShapes` holds the three built-in factories as `'static`
//!   references (`CYLINDER`, `ELLIPSE`, `PIPE`). Java also exposes
//!   `register(String, Factory)`, `forEach*` and the `ConcurrentHashMap` they
//!   fill; those exist so extensions can add custom shapes. The port has no
//!   extension mechanism (and no loader yet — Phase 4e), so registration is
//!   omitted; [`DistortedShapes::get_by_name`] recognises exactly the three
//!   names Java registers in its static initializer
//!   (`"PIPE"`, `"CYLINDER"`, `"ELLIPSOID"`).
//! * `getDefault()` / `getOrDefault(name)` / `getByName(name)` keep the Java
//!   null semantics (`getByName` → `Option`, `getOrDefault` → the pipe).

use procedural::noise::InterpolationMode;

use crate::cave::shape::distorted::distorted_cylinder_shape::DistortedCylinderShapeFactory;
use crate::cave::shape::distorted::distorted_ellipsoid_shape::DistortedEllipsoidShapeFactory;
use crate::cave::shape::distorted::distorted_pipe_shape::DistortedPipeShapeFactory;
use crate::cave::shape::distorted::distorted_shape::{DistortedShape, DEFAULT_INTERPOLATION};
use crate::loader::util::Vector3d;

/// Java `interface DistortedShape.Factory`.
///
/// The full `create` is the abstract member; the other `create` overloads are
/// the Java default methods.
#[allow(clippy::too_many_arguments)] // The Java signatures have up to ten parameters.
pub trait DistortedShapeFactory {
    /// Java `DistortedShape create(Vector3d origin, Vector3d direction, double
    /// length, double startWidth, double startHeight, double midWidth, double
    /// midHeight, double endWidth, double endHeight, GeneralNoise.InterpolationFunction
    /// interpolation)`.
    fn create(
        &self,
        origin: Vector3d,
        direction: Vector3d,
        length: f64,
        start_width: f64,
        start_height: f64,
        mid_width: f64,
        mid_height: f64,
        end_width: f64,
        end_height: f64,
        interpolation: InterpolationMode,
    ) -> Box<dyn DistortedShape>;

    /// Java `default DistortedShape create(origin, direction, length,
    /// startWidth, startHeight, midWidth, midHeight, endWidth, endHeight)`.
    #[allow(clippy::too_many_arguments)]
    fn create_default_interpolation(
        &self,
        origin: Vector3d,
        direction: Vector3d,
        length: f64,
        start_width: f64,
        start_height: f64,
        mid_width: f64,
        mid_height: f64,
        end_width: f64,
        end_height: f64,
    ) -> Box<dyn DistortedShape> {
        self.create(
            origin,
            direction,
            length,
            start_width,
            start_height,
            mid_width,
            mid_height,
            end_width,
            end_height,
            DEFAULT_INTERPOLATION,
        )
    }

    /// Java `default DistortedShape create(origin, direction, length,
    /// startWidth, startHeight, endWidth, endHeight, interpolation)` — the
    /// middle dimensions are the averages of the start and the end.
    #[allow(clippy::too_many_arguments)]
    fn create_with_end_dims(
        &self,
        origin: Vector3d,
        direction: Vector3d,
        length: f64,
        start_width: f64,
        start_height: f64,
        end_width: f64,
        end_height: f64,
        interpolation: InterpolationMode,
    ) -> Box<dyn DistortedShape> {
        let mid_width = (start_width + end_width) * 0.5;
        let mid_height = (start_height + end_height) * 0.5;
        self.create(
            origin,
            direction,
            length,
            start_width,
            start_height,
            mid_width,
            mid_height,
            end_width,
            end_height,
            interpolation,
        )
    }

    /// Java `default DistortedShape create(origin, direction, length,
    /// startWidth, startHeight, endWidth, endHeight)`.
    #[allow(clippy::too_many_arguments)]
    fn create_with_end_dims_default_interpolation(
        &self,
        origin: Vector3d,
        direction: Vector3d,
        length: f64,
        start_width: f64,
        start_height: f64,
        end_width: f64,
        end_height: f64,
    ) -> Box<dyn DistortedShape> {
        self.create_with_end_dims(
            origin,
            direction,
            length,
            start_width,
            start_height,
            end_width,
            end_height,
            DEFAULT_INTERPOLATION,
        )
    }

    /// Java `default DistortedShape create(origin, direction, length, width,
    /// height, interpolation)` — one cross-section for the whole shape.
    fn create_uniform(
        &self,
        origin: Vector3d,
        direction: Vector3d,
        length: f64,
        width: f64,
        height: f64,
        interpolation: InterpolationMode,
    ) -> Box<dyn DistortedShape> {
        self.create(
            origin,
            direction,
            length,
            width,
            height,
            width,
            height,
            width,
            height,
            interpolation,
        )
    }
}

/// Java `public static final DistortedShape.Factory CYLINDER`.
pub static CYLINDER: DistortedCylinderShapeFactory = DistortedCylinderShapeFactory;
/// Java `public static final DistortedShape.Factory ELLIPSE`.
pub static ELLIPSE: DistortedEllipsoidShapeFactory = DistortedEllipsoidShapeFactory;
/// Java `public static final DistortedShape.Factory PIPE`.
pub static PIPE: DistortedPipeShapeFactory = DistortedPipeShapeFactory;

/// Java `final class DistortedShapes`.
pub struct DistortedShapes;

impl DistortedShapes {
    /// Java `public static final DistortedShape.Factory CYLINDER`.
    pub const CYLINDER: &'static dyn DistortedShapeFactory = &CYLINDER;
    /// Java `public static final DistortedShape.Factory ELLIPSE`.
    pub const ELLIPSE: &'static dyn DistortedShapeFactory = &ELLIPSE;
    /// Java `public static final DistortedShape.Factory PIPE`.
    pub const PIPE: &'static dyn DistortedShapeFactory = &PIPE;

    /// Java `public static DistortedShape.Factory getDefault()` — the pipe.
    pub fn get_default() -> &'static dyn DistortedShapeFactory {
        Self::PIPE
    }

    /// Java `public static DistortedShape.Factory getOrDefault(String name)`.
    pub fn get_or_default(name: &str) -> &'static dyn DistortedShapeFactory {
        Self::get_by_name(name).unwrap_or_else(Self::get_default)
    }

    /// Java `@Nullable public static DistortedShape.Factory getByName(String
    /// name)`.
    ///
    /// The Java static initializer registers the factories under the
    /// `CaveNodeShapeEnum` names:
    ///
    /// ```java
    /// register(CaveNodeShapeEnum.PIPE.name(), PIPE);
    /// register(CaveNodeShapeEnum.CYLINDER.name(), CYLINDER);
    /// register(CaveNodeShapeEnum.ELLIPSOID.name(), ELLIPSE);
    /// ```
    ///
    /// Note the ellipse is registered as `"ELLIPSOID"` (the enum constant
    /// name), not `"ELLIPSE"` (the factory field name).
    pub fn get_by_name(name: &str) -> Option<&'static dyn DistortedShapeFactory> {
        match name {
            "PIPE" => Some(Self::PIPE),
            "CYLINDER" => Some(Self::CYLINDER),
            "ELLIPSOID" => Some(Self::ELLIPSE),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_knows_the_three_built_in_names() {
        assert!(DistortedShapes::get_by_name("PIPE").is_some());
        assert!(DistortedShapes::get_by_name("CYLINDER").is_some());
        // The Java registry uses the *enum* name.
        assert!(DistortedShapes::get_by_name("ELLIPSOID").is_some());
        assert!(DistortedShapes::get_by_name("ELLIPSE").is_none());
        assert!(DistortedShapes::get_by_name("pipe").is_none());
        assert!(DistortedShapes::get_by_name("SPHERE").is_none());
    }

    #[test]
    fn the_default_shape_is_the_pipe() {
        let default = DistortedShapes::get_default();
        let pipe = DistortedShapes::get_by_name("PIPE").unwrap();
        assert!(std::ptr::eq(default, pipe));
        let fallback = DistortedShapes::get_or_default("SPHERE");
        assert!(std::ptr::eq(fallback, pipe));
    }

    #[test]
    fn the_overloads_average_the_middle_dimensions() {
        let shape = DistortedShapes::PIPE.create_with_end_dims(
            Vector3d::new(0.0, 40.0, 0.0),
            Vector3d::new(10.0, 0.0, 0.0),
            10.0,
            2.0,
            4.0,
            6.0,
            8.0,
            InterpolationMode::Linear,
        );
        // Linear interpolation through the averaged middle == the midpoint
        // between the start and the end.
        assert_eq!(shape.get_width_at(0.5), 4.0);
        assert_eq!(shape.get_height_at(0.5), 6.0);
        assert_eq!(shape.get_width_at(0.25), 3.0);
        assert_eq!(shape.get_height_at(0.25), 5.0);
    }

    #[test]
    fn the_uniform_overload_keeps_one_cross_section() {
        let shape = DistortedShapes::CYLINDER.create_uniform(
            Vector3d::new(0.0, 40.0, 0.0),
            Vector3d::new(10.0, 0.0, 0.0),
            10.0,
            3.0,
            5.0,
            InterpolationMode::Linear,
        );
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert_eq!(shape.get_width_at(t), 3.0);
            assert_eq!(shape.get_height_at(t), 5.0);
        }
    }
}
