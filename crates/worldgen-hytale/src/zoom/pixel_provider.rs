// Ported from: `.hytale-source/.../worldgen/zoom/PixelProvider.java`
//! Java `zoom.PixelProvider` — an RGB pixel map with clamped sampling.

/// Java `zoom.PixelProvider` (`BufferedImage` constructor becomes
/// [`PixelProvider::from_rgb_image`], provided by the asset loader).
#[derive(Clone)]
pub struct PixelProvider {
    /// Java `protected final int[] pixels` — RGB triples packed as `0xRRGGBB`.
    pub pixels: Vec<i32>,
    /// Java `protected final int width`.
    pub width: usize,
    /// Java `protected final int height`.
    pub height: usize,
}

impl PixelProvider {
    /// Java `PixelProvider(BufferedImage)` — from a row-major RGBA8 buffer
    /// (`0xAARRGGBB` ints, as `BufferedImage.getRGB` returns; masked to
    /// `0xFFFFFF`).
    pub fn from_rgb_image(width: usize, height: usize, get_rgb: impl Fn(usize, usize) -> i32) -> Self {
        let mut pixels = vec![0i32; width * height];
        for x in 0..width {
            for y in 0..height {
                pixels[x * height + y] = get_rgb(x, y) & 0xFFFFFF;
            }
        }
        Self { pixels, width, height }
    }

    /// Java `PixelProvider(PixelProvider other)` — see [`PixelProvider::clone`].
    pub fn from_pixels(width: usize, height: usize, pixels: Vec<i32>) -> Self {
        assert_eq!(pixels.len(), width * height, "pixel buffer size mismatch");
        Self { pixels, width, height }
    }

    /// Java `getWidth()`.
    pub fn get_width(&self) -> usize {
        self.width
    }

    /// Java `getHeight()`.
    pub fn get_height(&self) -> usize {
        self.height
    }

    /// Java `getPixels()`.
    pub fn get_pixels(&self) -> &[i32] {
        &self.pixels
    }

    /// Java `setPixel(int x, int y, int pixel)`.
    pub fn set_pixel(&mut self, x: i32, y: i32, pixel: i32) {
        self.pixels[arr_index(x, y, self.height)] = pixel;
    }

    /// Java `getPixel(int x, int y)` — clamps out-of-range coordinates.
    pub fn get_pixel(&self, x: i32, y: i32) -> i32 {
        let mut x = x;
        let mut y = y;
        if x < 0 {
            x = 0;
        } else if x >= self.width as i32 {
            x = self.width as i32 - 1;
        }
        if y < 0 {
            y = 0;
        } else if y >= self.height as i32 {
            y = self.height as i32 - 1;
        }
        self.pixels[arr_index(x, y, self.height)]
    }

    /// Java `copy()`.
    pub fn copy(&self) -> Self {
        self.clone()
    }
}

/// Java `protected int arrIndex(int x, int y)` — column-major (`x * height + y`).
#[inline]
pub fn arr_index(x: i32, y: i32, height: usize) -> usize {
    (x * height as i32 + y) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: usize, height: usize, color: i32) -> PixelProvider {
        PixelProvider::from_pixels(width, height, vec![color; width * height])
    }

    #[test]
    fn clamped_sampling() {
        let p = solid(4, 4, 0x00FF00);
        assert_eq!(p.get_pixel(-5, -5), 0x00FF00);
        assert_eq!(p.get_pixel(99, 99), 0x00FF00);
        assert_eq!(p.get_pixel(2, 3), 0x00FF00);
    }

    #[test]
    fn set_and_get_column_major() {
        let mut p = solid(4, 4, 0);
        p.set_pixel(2, 1, 0x123456);
        assert_eq!(p.get_pixel(2, 1), 0x123456);
        // Java arrIndex: x * height + y — verify column-major layout directly
        assert_eq!(p.pixels[2 * 4 + 1], 0x123456);
    }

    #[test]
    fn copy_is_independent() {
        let mut p = solid(4, 4, 1);
        let c = p.copy();
        p.set_pixel(0, 0, 99);
        assert_eq!(c.get_pixel(0, 0), 1);
    }
}
