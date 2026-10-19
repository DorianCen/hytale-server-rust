// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/ResultBuffer.java`
//! Preallocated result buffers used by the cell-based noises.
//!
//! Java keeps a singleton `ResultBuffer.buffer2d`/`buffer3d` (effectively a
//! thread-local scratch buffer). In the Rust port callers pass `&mut
//! ResultBuffer2d` explicitly (see PORTING.md — scratch buffers become
//! explicit parameters; the eventual `GenContext` will own them).

/// Java `ResultBuffer.ResultBuffer2d`.
#[derive(Debug, Clone)]
pub struct ResultBuffer2d {
    pub hash: i32,
    pub hash2: i32,
    pub ix: i32,
    pub iy: i32,
    pub ix2: i32,
    pub iy2: i32,
    pub distance: f64,
    pub distance2: f64,
    pub x: f64,
    pub y: f64,
    pub x2: f64,
    pub y2: f64,
}

impl ResultBuffer2d {
    /// Java relies on default zero-initialization of the static buffer.
    pub fn new() -> Self {
        Self {
            hash: 0,
            hash2: 0,
            ix: 0,
            iy: 0,
            ix2: 0,
            iy2: 0,
            distance: 0.0,
            distance2: 0.0,
            x: 0.0,
            y: 0.0,
            x2: 0.0,
            y2: 0.0,
        }
    }

    /// Java `ResultBuffer.ResultBuffer2d.register`.
    pub fn register(&mut self, hash: i32, ix: i32, iy: i32, distance: f64, x: f64, y: f64) {
        if distance < self.distance {
            self.ix = ix;
            self.iy = iy;
            self.distance = distance;
            self.x = x;
            self.y = y;
            self.hash = hash;
        }
    }

    /// Java `ResultBuffer.ResultBuffer2d.register2`.
    pub fn register2(&mut self, hash: i32, ix: i32, iy: i32, distance: f64, x: f64, y: f64) {
        if distance < self.distance {
            self.distance2 = self.distance;
            self.x2 = self.x;
            self.y2 = self.y;
            self.ix2 = self.ix;
            self.iy2 = self.iy;
            self.distance = distance;
            self.x = x;
            self.y = y;
            self.ix = ix;
            self.iy = iy;
            self.hash2 = self.hash;
            self.hash = hash;
        } else if distance < self.distance2 {
            self.distance2 = distance;
            self.x2 = x;
            self.y2 = y;
            self.ix2 = ix;
            self.iy2 = iy;
            self.hash2 = hash;
        }
    }
}

impl Default for ResultBuffer2d {
    fn default() -> Self {
        Self::new()
    }
}

/// Java `ResultBuffer.ResultBuffer3d`.
#[derive(Debug, Clone)]
pub struct ResultBuffer3d {
    pub hash: i32,
    pub hash2: i32,
    pub ix: i32,
    pub iy: i32,
    pub iz: i32,
    pub ix2: i32,
    pub iy2: i32,
    pub iz2: i32,
    pub distance: f64,
    pub distance2: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub x2: f64,
    pub y2: f64,
    pub z2: f64,
}

impl ResultBuffer3d {
    /// Java relies on default zero-initialization of the static buffer.
    pub fn new() -> Self {
        Self {
            hash: 0,
            hash2: 0,
            ix: 0,
            iy: 0,
            iz: 0,
            ix2: 0,
            iy2: 0,
            iz2: 0,
            distance: 0.0,
            distance2: 0.0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            x2: 0.0,
            y2: 0.0,
            z2: 0.0,
        }
    }

    /// Java `ResultBuffer.ResultBuffer3d.register`.
    pub fn register(
        &mut self,
        hash: i32,
        ix: i32,
        iy: i32,
        iz: i32,
        distance: f64,
        x: f64,
        y: f64,
        z: f64,
    ) {
        if distance < self.distance {
            self.hash = hash;
            self.ix = ix;
            self.iy = iy;
            self.iz = iz;
            self.distance = distance;
            self.x = x;
            self.y = y;
            self.z = z;
        }
    }

    /// Java `ResultBuffer.ResultBuffer3d.register2`.
    pub fn register2(
        &mut self,
        hash: i32,
        ix: i32,
        iy: i32,
        iz: i32,
        distance: f64,
        x: f64,
        y: f64,
        z: f64,
    ) {
        if distance < self.distance {
            self.distance2 = self.distance;
            self.x2 = self.x;
            self.y2 = self.y;
            self.z2 = self.z;
            self.ix2 = self.ix;
            self.iy2 = self.iy;
            self.iz2 = self.iz;
            self.distance = distance;
            self.x = x;
            self.y = y;
            self.z = z;
            self.ix = ix;
            self.iy = iy;
            self.iz = iz;
            self.hash2 = self.hash;
            self.hash = hash;
        } else if distance < self.distance2 {
            self.distance2 = distance;
            self.x2 = x;
            self.y2 = y;
            self.z2 = z;
            self.ix2 = ix;
            self.iy2 = iy;
            self.iz2 = iz;
            self.hash2 = hash;
        }
    }
}

impl Default for ResultBuffer3d {
    fn default() -> Self {
        Self::new()
    }
}

/// Java `ResultBuffer.Bounds2d`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds2d {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Bounds2d {
    pub fn new() -> Self {
        Self {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 0.0,
            max_y: 0.0,
        }
    }

    /// Java `Bounds2d.assign`.
    pub fn assign(&mut self, min_x: f64, min_y: f64, max_x: f64, max_y: f64) {
        self.min_x = min_x;
        self.min_y = min_y;
        self.max_x = max_x;
        self.max_y = max_y;
    }

    /// Java `Bounds2d.contains`.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }
}

impl Default for Bounds2d {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_keeps_closest() {
        let mut b = ResultBuffer2d::new();
        b.distance = f64::INFINITY;
        b.register(1, 3, 4, 2.0, 10.0, 20.0);
        b.register(2, 7, 8, 1.0, 30.0, 40.0);
        assert_eq!(b.hash, 2);
        assert_eq!(b.ix, 7);
        assert_eq!(b.iy, 8);
        assert_eq!(b.distance, 1.0);
        assert_eq!(b.x, 30.0);
        assert_eq!(b.y, 40.0);
    }

    #[test]
    fn register2_keeps_two_closest() {
        let mut b = ResultBuffer2d::new();
        b.distance = f64::INFINITY;
        b.distance2 = f64::INFINITY;
        b.register2(1, 1, 1, 5.0, 1.0, 1.0);
        b.register2(2, 2, 2, 3.0, 2.0, 2.0);
        b.register2(3, 3, 3, 4.0, 3.0, 3.0);
        // closest is still (2,2,2); second closest updated from 5.0 -> 4.0
        assert_eq!(b.hash, 2);
        assert_eq!(b.distance, 3.0);
        assert_eq!(b.hash2, 3);
        assert_eq!(b.distance2, 4.0);
        assert_eq!(b.ix2, 3);
    }

    #[test]
    fn bounds_contains() {
        let mut b = Bounds2d::new();
        b.assign(-1.0, -2.0, 3.0, 4.0);
        assert!(b.contains(0.0, 0.0));
        assert!(b.contains(-1.0, 4.0));
        assert!(!b.contains(3.5, 0.0));
        assert!(!b.contains(0.0, -2.5));
    }
}
