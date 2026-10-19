// Ported from: `.hytale-source/.../worldgen/climate/util/{DoubleMap,IntMap}.java`
//! Java `climate.util` map primitives — row-major grids initialized to `-1`.

/// Java `climate.util.IntMap`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntMap {
    /// Java `public final int width`.
    pub width: i32,
    /// Java `public final int height`.
    pub height: i32,
    /// Java `private final int[] values`.
    values: Vec<i32>,
}

impl IntMap {
    /// Java `IntMap(int width, int height)` — filled with `-1`.
    pub fn new(width: i32, height: i32) -> Self {
        Self {
            width,
            height,
            values: vec![-1; (width * height) as usize],
        }
    }

    /// Java `int index(int x, int y)` — row-major (`y * width + x`).
    #[inline]
    pub fn index(&self, x: i32, y: i32) -> usize {
        (y * self.width + x) as usize
    }

    /// Java `boolean validate(int index)`.
    pub fn validate(&self, index: usize) -> bool {
        index < self.values.len()
    }

    /// Java `void clear()`.
    pub fn clear(&mut self) {
        self.values.fill(-1);
    }

    /// Java `int at(int x, int y)`.
    #[inline]
    pub fn at(&self, x: i32, y: i32) -> i32 {
        self.values[self.index(x, y)]
    }

    /// Java `int at(int index)`.
    #[inline]
    pub fn at_index(&self, index: usize) -> i32 {
        self.values[index]
    }

    /// Java `void set(int x, int y, int value)`.
    #[inline]
    pub fn set(&mut self, x: i32, y: i32, value: i32) {
        let index = self.index(x, y);
        self.values[index] = value;
    }

    /// Java `void set(int index, int value)`.
    #[inline]
    pub fn set_index(&mut self, index: usize, value: i32) {
        self.values[index] = value;
    }
}

/// Java `climate.util.DoubleMap`.
#[derive(Debug, Clone, PartialEq)]
pub struct DoubleMap {
    /// Java `public final int width`.
    pub width: i32,
    /// Java `public final int height`.
    pub height: i32,
    /// Java `protected final double[] values`.
    values: Vec<f64>,
}

impl DoubleMap {
    /// Java `DoubleMap(int width, int height)` — filled with `-1f`.
    pub fn new(width: i32, height: i32) -> Self {
        Self {
            width,
            height,
            values: vec![-1.0; (width * height) as usize],
        }
    }

    /// Java `int index(int x, int y)`.
    #[inline]
    pub fn index(&self, x: i32, y: i32) -> usize {
        (y * self.width + x) as usize
    }

    /// Java `boolean validate(int index)`.
    pub fn validate(&self, index: usize) -> bool {
        index < self.values.len()
    }

    /// Java `void clear()`.
    pub fn clear(&mut self) {
        self.values.fill(-1.0);
    }

    /// Java `double at(int x, int y)`.
    #[inline]
    pub fn at(&self, x: i32, y: i32) -> f64 {
        self.values[self.index(x, y)]
    }

    /// Java `double at(int index)`.
    #[inline]
    pub fn at_index(&self, index: usize) -> f64 {
        self.values[index]
    }

    /// Java `void set(int x, int y, double value)`.
    #[inline]
    pub fn set(&mut self, x: i32, y: i32, value: f64) {
        let index = self.index(x, y);
        self.values[index] = value;
    }

    /// Java `void set(int index, double value)`.
    #[inline]
    pub fn set_index(&mut self, index: usize, value: f64) {
        self.values[index] = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_are_row_major_and_negative_filled() {
        let mut m = IntMap::new(4, 3);
        assert_eq!(m.at(0, 0), -1);
        m.set(2, 1, 7);
        assert_eq!(m.at(2, 1), 7);
        // row-major: index = y*width + x
        #[allow(clippy::identity_op)]
        let idx = 1 * 4 + 2;
        assert_eq!(m.index(2, 1), idx);
        assert_eq!(m.at_index(idx), 7);
        assert!(m.validate(11));
        assert!(!m.validate(12));

        let mut d = DoubleMap::new(4, 3);
        assert_eq!(d.at(1, 1), -1.0);
        d.set(0, 0, 0.5);
        assert_eq!(d.at(0, 0), 0.5);
    }
}
