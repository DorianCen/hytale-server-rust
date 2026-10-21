// Ported from: `.hytale-source/.../worldgen/loader/MaskProviderJsonLoader.java`
//! Java `worldgen.loader.MaskProviderJsonLoader` — the **image** mask path
//! (`Mask.png`).
//!
//! Java reads the mask through `javax.imageio.ImageIO` (a `BufferedImage`)
//! and hands it to the `PixelProvider`, which keeps the pixels in the
//! column-major `x * height + y` order. The Rust port decodes the PNG with
//! the `png` crate and reproduces the same pixel values (`0xRRGGBB`, the
//! alpha channel dropped).

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::chunk::mask_provider::MaskProvider;
use crate::zoom::fuzzy_zoom::FuzzyZoom;
use crate::zoom::pixel_provider::PixelProvider;
use procedural::json::coordinate_randomizer_json_loader::CoordinateRandomizerJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `MaskProviderJsonLoader`.
pub struct MaskProviderJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, MaskProvider>`.
    pub base: JsonLoaderBase,
    /// Java `protected final Path file`.
    file: PathBuf,
    /// Java `protected final Vector2i zoomSize`.
    zoom_size: (i32, i32),
    /// Java `protected final Vector2i worldOffset`.
    world_offset: (i32, i32),
}

impl MaskProviderJsonLoader {
    /// Java `MaskProviderJsonLoader(SeedString<SeedStringResource> seed, Path dataFolder, JsonElement json, Path file, Vector2i zoomSize, Vector2i worldOffset)`.
    ///
    /// Note: the Java constructor **appends** the `".MaskProvider"` suffix to
    /// the seed.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        file: PathBuf,
        zoom_size: (i32, i32),
        world_offset: (i32, i32),
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".MaskProvider"), data_folder, json),
            file,
            zoom_size,
            world_offset,
        }
    }

    /// Java `MaskProvider load()`.
    pub fn load(&self) -> MaskProvider {
        match self.try_load() {
            Ok(provider) => provider,
            // Java: `throw new Error("Error while loading MaskProvider in " +
            // file.toAbsolutePath(), e)`.
            Err(cause) => panic!(
                "Error while loading MaskProvider in {}: {cause}",
                absolute_path(&self.file).display()
            ),
        }
    }

    /// Java `MaskProvider load()` — the fallible variant.
    pub fn try_load(&self) -> Result<MaskProvider, String> {
        let mask = Self::load_image(&self.file)?;
        Ok(MaskProvider::new(self.load_fuzzy_zoom(&mask)))
    }

    /// Java `static BufferedImage loadImage(Path file)`.
    pub fn load_image(file: &Path) -> Result<PixelProvider, String> {
        let bytes = std::fs::read(file)
            .map_err(|e| format!("Failed to load image {}: {e}", file.display()))?;
        Self::decode_png(&bytes)
            .map_err(|e| format!("Failed to load image {}: {e}", file.display()))
    }

    /// Decodes a PNG into a [`PixelProvider`].
    ///
    /// Java's `ImageIO.read` returns a `BufferedImage` whose `getRGB` values
    /// are `0xAARRGGBB`; the `PixelProvider` masks them to `0xRRGGBB`. The
    /// Rust decoder expands palettes / grayscale to 8-bit RGB (like
    /// `ImageIO` does for the common PNG flavours) and drops the alpha
    /// channel.
    fn decode_png(bytes: &[u8]) -> Result<PixelProvider, String> {
        let mut decoder = png::Decoder::new(bytes);
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder
            .read_info()
            .map_err(|e| format!("cannot read the PNG header: {e}"))?;
        let mut buffer = vec![0u8; reader.output_buffer_size()];
        let info = reader
            .next_frame(&mut buffer)
            .map_err(|e| format!("cannot decode the PNG image: {e}"))?;

        let width = info.width as usize;
        let height = info.height as usize;
        let channels = match info.color_type {
            png::ColorType::Rgb => 3usize,
            png::ColorType::Rgba => 4usize,
            other => return Err(format!("unsupported PNG color type: {other:?}")),
        };

        Ok(PixelProvider::from_rgb_image(width, height, |x, y| {
            let index = (y * width + x) * channels;
            let r = buffer[index] as i32;
            let g = buffer[index + 1] as i32;
            let b = buffer[index + 2] as i32;
            (r << 16) | (g << 8) | b
        }))
    }

    /// Java `FuzzyZoom loadFuzzyZoom(BufferedImage mask)`.
    pub fn load_fuzzy_zoom(&self, mask: &PixelProvider) -> FuzzyZoom {
        FuzzyZoom::new(
            CoordinateRandomizerJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.json.clone(),
            )
            .load(),
            mask.clone(),
            self.zoom_size.0 as f64 / mask.get_width() as f64,
            self.zoom_size.1 as f64 / mask.get_height() as f64,
            self.world_offset.0,
            self.world_offset.1,
        )
    }

    /// Java `protected final Path file`.
    pub fn get_file(&self) -> &Path {
        &self.file
    }
}

/// Java `Path.toAbsolutePath()` — the path is made absolute without
/// normalizing it.
fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(dir) => dir.join(path),
            Err(_) => path.to_path_buf(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extracts the message of a caught panic.
    fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
        if let Some(message) = payload.downcast_ref::<String>() {
            return message.clone();
        }
        if let Some(message) = payload.downcast_ref::<&'static str>() {
            return (*message).to_string();
        }
        "<non-string panic>".to_string()
    }

    /// A 2×2 PNG (red, green / blue, white) built by hand.
    fn tiny_png() -> Vec<u8> {
        // A minimal 2x2 RGB PNG.
        let mut data: Vec<u8> = Vec::new();
        // signature
        data.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        // IHDR
        let mut ihdr: Vec<u8> = Vec::new();
        ihdr.extend_from_slice(&2u32.to_be_bytes());
        ihdr.extend_from_slice(&2u32.to_be_bytes());
        ihdr.push(8); // bit depth
        ihdr.push(2); // color type: RGB
        ihdr.push(0); // compression
        ihdr.push(0); // filter
        ihdr.push(0); // interlace
        push_chunk(&mut data, b"IHDR", &ihdr);
        // IDAT (zlib-compressed scanlines: filter byte + 3 RGB bytes)
        let scanlines: Vec<u8> = vec![
            0, 0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00, // row 0: red, green
            0, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, // row 1: blue, white
        ];
        let compressed = zlib_store(&scanlines);
        push_chunk(&mut data, b"IDAT", &compressed);
        push_chunk(&mut data, b"IEND", &[]);
        data
    }

    /// A minimal zlib "stored" (uncompressed) deflate stream.
    fn zlib_store(data: &[u8]) -> Vec<u8> {
        let mut out = vec![0x78, 0x01];
        let mut offset = 0usize;
        while offset < data.len() {
            let len = (data.len() - offset).min(0xFFFF);
            let last = offset + len >= data.len();
            out.push(if last { 1 } else { 0 });
            out.extend_from_slice(&(len as u16).to_le_bytes());
            out.extend_from_slice(&(!(len as u16)).to_le_bytes());
            out.extend_from_slice(&data[offset..offset + len]);
            offset += len;
        }
        // Adler-32 checksum
        let (mut a, mut b) = (1u32, 0u32);
        for byte in data {
            a = (a + *byte as u32) % 65521;
            b = (b + a) % 65521;
        }
        out.extend_from_slice(&((b << 16) | a).to_be_bytes());
        out
    }

    fn push_chunk(data: &mut Vec<u8>, kind: &[u8; 4], payload: &[u8]) {
        data.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        data.extend_from_slice(kind);
        data.extend_from_slice(payload);
        let mut crc_input = Vec::with_capacity(4 + payload.len());
        crc_input.extend_from_slice(kind);
        crc_input.extend_from_slice(payload);
        data.extend_from_slice(&crc32(&crc_input).to_be_bytes());
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for byte in data {
            crc ^= *byte as u32;
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }

    #[test]
    fn png_is_decoded_into_rgb_pixels() {
        let provider = MaskProviderJsonLoader::decode_png(&tiny_png()).expect("decodes");
        assert_eq!(provider.get_width(), 2);
        assert_eq!(provider.get_height(), 2);
        // Java: `image.getRGB(x, y) & 0xFFFFFF`.
        assert_eq!(provider.get_pixel(0, 0), 0xFF0000);
        assert_eq!(provider.get_pixel(1, 0), 0x00FF00);
        assert_eq!(provider.get_pixel(0, 1), 0x0000FF);
        assert_eq!(provider.get_pixel(1, 1), 0xFFFFFF);
    }

    #[test]
    fn missing_file_reports_the_java_error() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            MaskProviderJsonLoader::new(
                SeedString::new("0"),
                Path::new("."),
                None,
                PathBuf::from("/definitely/missing/Mask.png"),
                (1, 1),
                (0, 0),
            )
            .load()
        }));
        let message = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => panic_message(payload),
        };
        assert!(
            message.starts_with("Error while loading MaskProvider in "),
            "{message}"
        );
    }
}
