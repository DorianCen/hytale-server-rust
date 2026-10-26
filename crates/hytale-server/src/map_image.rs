//! Chunk world-map images.
//!
//! This is the server-side subset of Java
//! `CoreServer/.../worldmap/provider/chunk/ImageBuilder.java`: one image per
//! loaded chunk, sampled from the legacy heightmap, column tint and block /
//! fluid assets. Java's exact shader/quantisation tuning is not byte-compared;
//! the palette form, transparency, terrain height shading and water blending
//! follow the reference implementation, and `GOLDEN_DATA.md` records this as a
//! semantic rather than byte golden.
//!
//! The image is sent through `UpdateWorldMap.chunks` and an unload is the same
//! `MapChunk` with `image: None`.

use std::collections::{BTreeMap, BTreeSet};

use hytale_assets::AssetRegistry;
use hytale_protocol::generated::common::MapImage;
use hytale_world::chunk::column::ChunkColumn;
use hytale_world::world::World;
use serde_json::Value;

/// Java `WorldWorldMapConfig.DEFAULT_IMAGE_SCALE`.
pub const MAP_IMAGE_SCALE: f32 = 3.0;
/// Java `ChunkUtil.SIZE * WorldWorldMapConfig.DEFAULT_IMAGE_SCALE`.
pub const MAP_IMAGE_SIZE: u32 = 96;
/// Java `ChunkUtil.SIZE`.
const SAMPLE_SIZE: i32 = 32;
/// Legacy column height.
const COLUMN_HEIGHT: i32 = 320;
const QUANTIZE_STEP: u8 = 8;

/// Build the world-map image of one loaded chunk.
///
/// `None` means the chunk is not loaded; every loaded chunk produces an image,
/// even an all-transparent one (void/empty columns).
pub fn render_chunk(
    world: &World,
    assets: &AssetRegistry,
    chunk_x: i32,
    chunk_z: i32,
) -> Option<MapImage> {
    let column = world.chunk(chunk_x, chunk_z)?;
    Some(render_column(column, assets))
}

fn render_column(column: &ChunkColumn, assets: &AssetRegistry) -> MapImage {
    let size = MAP_IMAGE_SIZE as i32;
    let mut heights = vec![None; (SAMPLE_SIZE * SAMPLE_SIZE) as usize];
    let mut samples = vec![Pixel::TRANSPARENT; (SAMPLE_SIZE * SAMPLE_SIZE) as usize];

    for sample_z in 0..SAMPLE_SIZE {
        for sample_x in 0..SAMPLE_SIZE {
            let index = (sample_z * SAMPLE_SIZE + sample_x) as usize;
            let height = column
                .heightmap()
                .height(sample_x, sample_z)
                .or_else(|| scan_height(column, sample_x, sample_z));
            heights[index] = height;

            let Some(height) = height else {
                continue;
            };
            let block_id = column.get_block(sample_x, height, sample_z);
            let tint = column.tint(sample_x, sample_z);
            let mut color = block_color(assets, block_id, tint);
            let fluid = top_fluid_layer(column, sample_x, height, sample_z);

            if color.alpha == 0 && fluid.is_none() {
                continue;
            }

            if color.alpha == 0 {
                color = Pixel::new(0, 0, 0, 255);
            }
            samples[index] = Pixel { fluid, ..color };
        }
    }

    for sample_z in 0..SAMPLE_SIZE {
        for sample_x in 0..SAMPLE_SIZE {
            let index = (sample_z * SAMPLE_SIZE + sample_x) as usize;
            let sample = samples[index];
            if sample.alpha == 0 && sample.fluid.is_none() {
                continue;
            }
            let height = heights[index].unwrap_or(0);
            let north = neighbor_height(&heights, sample_x, sample_z - 1, height);
            let south = neighbor_height(&heights, sample_x, sample_z + 1, height);
            let west = neighbor_height(&heights, sample_x - 1, sample_z, height);
            let east = neighbor_height(&heights, sample_x + 1, sample_z, height);
            let shade = shade(north, south, west, east);
            samples[index].shade = shade;
        }
    }

    let mut pixels = vec![0i32; (size * size) as usize];
    for pixel_z in 0..size {
        let sample_z = (pixel_z * SAMPLE_SIZE / size).min(SAMPLE_SIZE - 1);
        for pixel_x in 0..size {
            let sample_x = (pixel_x * SAMPLE_SIZE / size).min(SAMPLE_SIZE - 1);
            let index = (sample_z * SAMPLE_SIZE + sample_x) as usize;
            let mut sample = samples[index];
            if sample.alpha == 0 && sample.fluid.is_none() {
                continue;
            }

            sample.apply_shade();
            if let Some(fluid) = sample.fluid {
                sample = blend_fluid(assets, sample, fluid);
            }
            pixels[(pixel_z * size + pixel_x) as usize] = sample.pack();
        }
    }

    encode_palette(size, &pixels)
}

fn scan_height(column: &ChunkColumn, x: i32, z: i32) -> Option<i32> {
    (0..COLUMN_HEIGHT)
        .rev()
        .find(|y| column.get_block(x, *y, z) != 0)
}

fn neighbor_height(heights: &[Option<i32>], x: i32, z: i32, fallback: i32) -> i32 {
    if !(0..SAMPLE_SIZE).contains(&x) || !(0..SAMPLE_SIZE).contains(&z) {
        return fallback;
    }
    heights[(z * SAMPLE_SIZE + x) as usize].unwrap_or(fallback)
}

fn shade(north: i32, south: i32, west: i32, east: i32) -> f32 {
    let dhdx = (east - west) as f32 * 0.5;
    let dhdz = (south - north) as f32 * 0.5;
    let nx = dhdx;
    let ny = 3.0;
    let nz = dhdz;
    let inv = 1.0 / (nx * nx + ny * ny + nz * nz).sqrt();
    let (lx, ly, lz) = (-0.2f32, 0.8f32, 0.5f32);
    let inv_l = 1.0 / (lx * lx + ly * ly + lz * lz).sqrt();
    let lambert = (nx * inv * lx * inv_l + ny * inv * ly * inv_l + nz * inv * lz * inv_l).max(0.0);
    0.4 + 0.6 * lambert
}

#[derive(Debug, Clone, Copy)]
struct FluidLayer {
    id: i32,
    depth: u16,
    environment: i32,
}

fn top_fluid_layer(column: &ChunkColumn, x: i32, ground: i32, z: i32) -> Option<FluidLayer> {
    let mut fluid_id = 0;
    let mut depth = 0u16;
    let mut top = None;
    for y in (ground.max(0)..COLUMN_HEIGHT).rev() {
        let id = column.get_fluid_id(x, y, z);
        if id != 0 {
            if fluid_id == 0 {
                top = Some(y);
            }
            fluid_id = id;
            depth = depth.saturating_add(1);
        } else if fluid_id != 0 {
            break;
        }
    }
    let top = top?;
    Some(FluidLayer {
        id: fluid_id,
        depth,
        environment: column.get_environment(x, top, z),
    })
}

#[derive(Debug, Clone, Copy)]
struct Pixel {
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
    shade: f32,
    fluid: Option<FluidLayer>,
}

impl Pixel {
    const TRANSPARENT: Self = Self {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 0,
        shade: 1.0,
        fluid: None,
    };

    const fn new(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
            shade: 1.0,
            fluid: None,
        }
    }

    fn apply_shade(&mut self) {
        self.red = (f32::from(self.red) * self.shade).clamp(0.0, 255.0) as u8;
        self.green = (f32::from(self.green) * self.shade).clamp(0.0, 255.0) as u8;
        self.blue = (f32::from(self.blue) * self.shade).clamp(0.0, 255.0) as u8;
    }

    fn pack(self) -> i32 {
        ((i32::from(self.red) & 0xFF) << 24)
            | ((i32::from(self.green) & 0xFF) << 16)
            | ((i32::from(self.blue) & 0xFF) << 8)
            | (i32::from(self.alpha) & 0xFF)
    }
}

fn block_color(assets: &AssetRegistry, block_id: i32, biome_tint: i32) -> Pixel {
    let Some(record) = assets.block_types.get_by_id(block_id) else {
        return Pixel::TRANSPARENT;
    };
    let json = &record.value.json;
    let self_tint = tint_color(json.get("TintUp").or_else(|| json.get("Tint")));
    let biome_multiplier = json
        .get("BiomeTintUp")
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        / 100.0;
    let (biome_r, biome_g, biome_b) = rgb_from_packed(biome_tint);
    let self_r = self_tint.map_or(255, |color| color.0);
    let self_g = self_tint.map_or(255, |color| color.1);
    let self_b = self_tint.map_or(255, |color| color.2);
    let mut red =
        (f64::from(self_r) + (f64::from(biome_r) - f64::from(self_r)) * biome_multiplier) as u8;
    let mut green =
        (f64::from(self_g) + (f64::from(biome_g) - f64::from(self_g)) * biome_multiplier) as u8;
    let mut blue =
        (f64::from(self_b) + (f64::from(biome_b) - f64::from(self_b)) * biome_multiplier) as u8;
    if biome_multiplier < 1.0 {
        if let Some(particle) = color_value(json.get("ParticleColor")) {
            red = (u16::from(red) * u16::from(particle.0) / 255) as u8;
            green = (u16::from(green) * u16::from(particle.1) / 255) as u8;
            blue = (u16::from(blue) * u16::from(particle.2) / 255) as u8;
        }
    }
    Pixel::new(red, green, blue, 255)
}

fn blend_fluid(assets: &AssetRegistry, mut base: Pixel, fluid: FluidLayer) -> Pixel {
    let Some(record) = assets.fluids.get_by_id(fluid.id) else {
        return base;
    };
    let json = &record.value.json;
    let mut tint = (255u8, 255u8, 255u8);
    let water_tint = assets
        .environments
        .get_by_id(fluid.environment)
        .and_then(|environment| color_value(environment.value.get("WaterTint")));
    if let Some(water) = water_tint {
        tint = (
            (u16::from(tint.0) * u16::from(water.0) / 255) as u8,
            (u16::from(tint.1) * u16::from(water.1) / 255) as u8,
            (u16::from(tint.2) * u16::from(water.2) / 255) as u8,
        );
    }
    if let Some(particle) = color_value(json.get("ParticleColor")) {
        tint = (
            (u16::from(tint.0) * u16::from(particle.0) / 255) as u8,
            (u16::from(tint.1) * u16::from(particle.1) / 255) as u8,
            (u16::from(tint.2) * u16::from(particle.2) / 255) as u8,
        );
    }

    if water_tint.is_some() {
        // Java water shading: blend the fluid tint with the block colour using
        // a depth attenuation.
        let depth = 1.0 / (f32::from(fluid.depth) + 1.0);
        base.red = (f32::from(tint.0) + (f32::from(base.red) - f32::from(tint.0)) * depth) as u8;
        base.green =
            (f32::from(tint.1) + (f32::from(base.green) - f32::from(tint.1)) * depth) as u8;
        base.blue = (f32::from(tint.2) + (f32::from(base.blue) - f32::from(tint.2)) * depth) as u8;
    } else {
        let depth_shade = (1.0 - f32::from(fluid.depth) * 0.02).max(0.75);
        base.red = (f32::from(tint.0) * depth_shade) as u8;
        base.green = (f32::from(tint.1) * depth_shade) as u8;
        base.blue = (f32::from(tint.2) * depth_shade) as u8;
        base.alpha = 255;
    }
    base
}

fn tint_color(value: Option<&Value>) -> Option<(u8, u8, u8)> {
    let value = value?;
    if let Some(values) = value.as_array() {
        return color_value(Some(values.first()?));
    }
    color_value(Some(value))
}

fn color_value(value: Option<&Value>) -> Option<(u8, u8, u8)> {
    let text = value?.as_str()?;
    let hex = text.strip_prefix('#').unwrap_or(text);
    let digits: Vec<char> = hex.chars().collect();
    let (red, green, blue) = match digits.len() {
        3 => (
            format!("{}{}", digits[0], digits[0]),
            format!("{}{}", digits[1], digits[1]),
            format!("{}{}", digits[2], digits[2]),
        ),
        6 | 8 => (
            digits[digits.len() - 6..digits.len() - 4].iter().collect(),
            digits[digits.len() - 4..digits.len() - 2].iter().collect(),
            digits[digits.len() - 2..].iter().collect(),
        ),
        _ => return None,
    };
    Some((
        u8::from_str_radix(&red, 16).ok()?,
        u8::from_str_radix(&green, 16).ok()?,
        u8::from_str_radix(&blue, 16).ok()?,
    ))
}

fn rgb_from_packed(value: i32) -> (u8, u8, u8) {
    (
        ((value >> 16) & 0xFF) as u8,
        ((value >> 8) & 0xFF) as u8,
        (value & 0xFF) as u8,
    )
}

fn encode_palette(size: i32, pixels: &[i32]) -> MapImage {
    let quantized: Vec<i32> = pixels.iter().map(|pixel| quantize(*pixel)).collect();
    let palette: Vec<i32> = quantized
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut color_to_index = BTreeMap::new();
    for (index, color) in palette.iter().enumerate() {
        color_to_index.insert(*color, index);
    }
    let bits = match palette.len() {
        0..=16 => 4,
        17..=256 => 8,
        257..=4096 => 12,
        _ => 16,
    };
    let byte_len = (pixels.len() * bits).div_ceil(8);
    let mut packed = vec![0u8; byte_len];
    for (index, color) in quantized.iter().enumerate() {
        let value = color_to_index.get(color).copied().unwrap_or(0);
        let bit = index * bits;
        for offset in 0..bits {
            if (value >> offset) & 1 == 1 {
                packed[(bit + offset) / 8] |= 1 << ((bit + offset) % 8);
            }
        }
    }

    MapImage {
        width: size,
        height: size,
        palette: Some(palette),
        bits_per_index: bits as i8,
        packed_indices: Some(packed.into_iter().map(|byte| byte as i8).collect()),
    }
}

fn quantize(pixel: i32) -> i32 {
    let red = ((pixel >> 24) & 0xFF) as u8;
    let green = ((pixel >> 16) & 0xFF) as u8;
    let blue = ((pixel >> 8) & 0xFF) as u8;
    let alpha = pixel & 0xFF;
    let quantize = |channel: u8| -> i32 {
        let half = i32::from(QUANTIZE_STEP) / 2;
        ((i32::from(channel) + half) / i32::from(QUANTIZE_STEP) * i32::from(QUANTIZE_STEP)).min(255)
    };
    (quantize(red) << 24) | (quantize(green) << 16) | (quantize(blue) << 8) | alpha
}

/// Decode one palette index from a `MapImage` (test/diagnostic helper).
pub fn pixel_index(image: &MapImage, x: i32, z: i32) -> Option<usize> {
    if x < 0 || z < 0 || x >= image.width || z >= image.height {
        return None;
    }
    let bits = usize::try_from(image.bits_per_index).ok()?;
    let packed = image.packed_indices.as_ref()?;
    let index = (z as usize) * image.width as usize + x as usize;
    let bit = index * bits;
    let mut value = 0usize;
    for offset in 0..bits {
        let position = bit + offset;
        let byte = *packed.get(position / 8)? as u8;
        if (byte >> (position % 8)) & 1 == 1 {
            value |= 1 << offset;
        }
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(pixels: &[i32]) -> MapImage {
        encode_palette(2, pixels)
    }

    #[test]
    fn palette_round_trips_every_pixel() {
        let colors = [0x1122_33FF, 0x1122_33FF, 0x4455_66FF, 0x7788_99FF];
        let encoded = image(&colors);
        assert_eq!(encoded.width, 2);
        assert_eq!(encoded.height, 2);
        assert_eq!(encoded.bits_per_index, 4);
        let palette = encoded.palette.as_ref().expect("palette");
        for (index, expected) in colors.iter().enumerate() {
            let decoded =
                pixel_index(&encoded, index as i32 % 2, index as i32 / 2).expect("pixel index");
            assert_eq!(palette[decoded], quantize(*expected));
        }
    }

    #[test]
    fn transparent_void_pixels_are_kept() {
        let encoded = image(&[0, 0, 0, 0]);
        assert_eq!(encoded.palette.as_deref(), Some(&[0][..]));
        assert_eq!(encoded.bits_per_index, 4);
        assert_eq!(pixel_index(&encoded, 1, 1), Some(0));
    }

    #[test]
    fn bits_per_index_follows_palette_size() {
        let mut pixels = Vec::new();
        for value in 0..300 {
            let red = (value & 0xF) * 16;
            let green = ((value >> 4) & 0xF) * 16;
            let blue = ((value >> 8) & 0xF) * 16;
            pixels.push((red << 24) | (green << 16) | (blue << 8) | 0xFF);
        }
        let encoded = image(&pixels);
        assert_eq!(encoded.bits_per_index, 12);
        assert_eq!(
            pixel_index(&encoded, 0, 0).and_then(|index| encoded.palette.as_ref()?.get(index)),
            Some(&quantize(pixels[0]))
        );
    }
}
