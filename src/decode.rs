// Copyright (c) 2026 Greg
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;

use image::ColorType;
use image::ImageDecoder;

use crate::animation::Animation;

pub const BACKGROUND: [u8; 3] = [0x1c, 0x20, 0x28];

pub struct Frame {
    pub pixels: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    pub anim: Option<Arc<Animation>>,
    pub partial: bool,
}

impl Frame {
    pub fn bytes(&self) -> usize {
        let mut total = self.pixels.len();
        if let Some(anim) = &self.anim {
            total += anim.frames.iter().map(|frame| frame.len()).sum::<usize>();
        }
        total
    }
}

pub fn load(
    path: &Path,
    max_width: u32,
    max_height: u32,
    anim_cap: usize,
    full: bool,
) -> Result<Frame, String> {
    let ext = extension(path);
    let orientation = if ext == "jpg" || ext == "jpeg" {
        jpeg_orientation(path).unwrap_or(1)
    } else {
        1
    };

    if full {
        match ext.as_str() {
            "gif" => return crate::animation::from_gif(path, max_width, max_height, anim_cap),
            "webp" => return crate::animation::from_webp(path, max_width, max_height, anim_cap),
            _ => {}
        }
    }

    let mut frame = still(path, orientation, max_width, max_height)?;
    // only the first frame is decoded here, the animation waits until it is opened
    frame.partial = (ext == "gif" || ext == "webp") && !full;
    Ok(frame)
}

pub fn image_size(path: &Path) -> Result<(u32, u32), String> {
    let decoder = open_decoder(path)?;
    let (width, height) = decoder.dimensions();
    let ext = extension(path);
    let orientation = if ext == "jpg" || ext == "jpeg" {
        jpeg_orientation(path).unwrap_or(1)
    } else {
        1
    };
    Ok(display_size(width, height, orientation))
}

fn open_decoder(path: &Path) -> Result<impl image::ImageDecoder, String> {
    image::ImageReader::open(path)
        .map_err(|err| format!("{}: {}", path.display(), err))?
        .with_guessed_format()
        .map_err(|err| format!("{}: {}", path.display(), err))?
        .into_decoder()
        .map_err(|err| format!("{}: {}", path.display(), err))
}

fn still(path: &Path, orientation: u8, max_width: u32, max_height: u32) -> Result<Frame, String> {
    let decoder = open_decoder(path)?;
    let (source_width, source_height) = decoder.dimensions();
    let color = decoder.color_type();

    let (display_width, display_height) = display_size(source_width, source_height, orientation);
    let scale = fit_scale(display_width, display_height, max_width, max_height);
    let (width, height) = scaled_size(display_width, display_height, scale);

    let raw;
    let pixels = if plain(color) {
        let mut buffer = vec![0u8; decoder.total_bytes() as usize];
        decoder
            .read_image(&mut buffer)
            .map_err(|err| format!("{}: {}", path.display(), err))?;
        render(
            &buffer,
            color,
            orientation,
            source_width,
            source_height,
            width,
            height,
        )
    } else {
        // fall back through a full rgba copy for the rare color types
        let image = image::DynamicImage::from_decoder(decoder)
            .map_err(|err| format!("{}: {}", path.display(), err))?;
        raw = image.into_rgba8();
        render(
            raw.as_raw(),
            ColorType::Rgba8,
            orientation,
            source_width,
            source_height,
            width,
            height,
        )
    };

    Ok(Frame {
        pixels: Arc::new(pixels),
        width,
        height,
        anim: None,
        partial: false,
    })
}

fn plain(color: ColorType) -> bool {
    matches!(
        color,
        ColorType::L8
            | ColorType::La8
            | ColorType::Rgb8
            | ColorType::Rgba8
            | ColorType::L16
            | ColorType::La16
            | ColorType::Rgb16
            | ColorType::Rgba16
    )
}

fn extension(path: &Path) -> String {
    path.extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

// one pass over the source: average each box, rotate, flatten onto the window
// background and emit top-down bgra, without a full-size rgba copy in between
pub fn render(
    source: &[u8],
    color: ColorType,
    orientation: u8,
    source_width: u32,
    source_height: u32,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    if width == 0 || height == 0 {
        return pixels;
    }

    let workers = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .min(height as usize);

    if (width as u64 * height as u64) < (1 << 20) || workers < 2 {
        band(
            color,
            source,
            source_width,
            source_height,
            orientation,
            width,
            height,
            &mut pixels,
            0,
            height,
        );
        return pixels;
    }

    let rows = (height as usize).div_ceil(workers);
    let stride = rows * width as usize * 4;
    std::thread::scope(|scope| {
        for (index, chunk) in pixels.chunks_mut(stride).enumerate() {
            let start = (index * rows) as u32;
            let end = ((index + 1) * rows).min(height as usize) as u32;
            scope.spawn(move || {
                band(
                    color,
                    source,
                    source_width,
                    source_height,
                    orientation,
                    width,
                    height,
                    chunk,
                    start,
                    end,
                );
            });
        }
    });
    pixels
}

#[allow(clippy::too_many_arguments)]
fn band(
    color: ColorType,
    source: &[u8],
    source_width: u32,
    source_height: u32,
    orientation: u8,
    width: u32,
    height: u32,
    out: &mut [u8],
    start: u32,
    end: u32,
) {
    match color {
        ColorType::L8 => rows::<1, true, false>(
            source,
            source_width,
            source_height,
            orientation,
            width,
            height,
            out,
            start,
            end,
        ),
        ColorType::La8 => rows::<2, true, false>(
            source,
            source_width,
            source_height,
            orientation,
            width,
            height,
            out,
            start,
            end,
        ),
        ColorType::Rgb8 => rows::<3, false, false>(
            source,
            source_width,
            source_height,
            orientation,
            width,
            height,
            out,
            start,
            end,
        ),
        ColorType::Rgba8 => rows::<4, false, false>(
            source,
            source_width,
            source_height,
            orientation,
            width,
            height,
            out,
            start,
            end,
        ),
        ColorType::L16 => rows::<1, true, true>(
            source,
            source_width,
            source_height,
            orientation,
            width,
            height,
            out,
            start,
            end,
        ),
        ColorType::La16 => rows::<2, true, true>(
            source,
            source_width,
            source_height,
            orientation,
            width,
            height,
            out,
            start,
            end,
        ),
        ColorType::Rgb16 => rows::<3, false, true>(
            source,
            source_width,
            source_height,
            orientation,
            width,
            height,
            out,
            start,
            end,
        ),
        ColorType::Rgba16 => rows::<4, false, true>(
            source,
            source_width,
            source_height,
            orientation,
            width,
            height,
            out,
            start,
            end,
        ),
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn rows<const CHANNELS: usize, const GRAY: bool, const SIXTEEN: bool>(
    source: &[u8],
    source_width: u32,
    source_height: u32,
    orientation: u8,
    width: u32,
    height: u32,
    out: &mut [u8],
    start: u32,
    end: u32,
) {
    let unit = if SIXTEEN { 2 } else { 1 };
    let stride = source_width as usize * CHANNELS * unit;
    let (display_width, display_height) = display_size(source_width, source_height, orientation);

    for y in start..end {
        let (top, bottom) = scaled_range(y, height, display_height);
        let row = &mut out[(y - start) as usize * width as usize * 4..];

        for x in 0..width {
            let (left, right) = scaled_range(x, width, display_width);
            let (sx0, sy0, sx1, sy1) = source_box(
                orientation,
                left,
                right,
                top,
                bottom,
                source_width,
                source_height,
            );

            let mut sum = [0u64; 4];
            let mut count = 0u64;
            for sy in sy0..sy1 {
                let mut at = sy as usize * stride + sx0 as usize * CHANNELS * unit;
                for _ in sx0..sx1 {
                    let alpha = if CHANNELS == 1 || CHANNELS == 3 {
                        255
                    } else {
                        sample::<SIXTEEN>(source, at + (CHANNELS - 1) * unit)
                    };
                    if GRAY {
                        let level = sample::<SIXTEEN>(source, at) * alpha;
                        sum[0] += level;
                        sum[1] += level;
                        sum[2] += level;
                    } else {
                        sum[0] += sample::<SIXTEEN>(source, at) * alpha;
                        sum[1] += sample::<SIXTEEN>(source, at + unit) * alpha;
                        sum[2] += sample::<SIXTEEN>(source, at + 2 * unit) * alpha;
                    }
                    sum[3] += alpha;
                    count += 1;
                    at += CHANNELS * unit;
                }
            }
            if count == 0 {
                continue;
            }

            let denominator = count * 255;
            let weight = denominator - sum[3];
            let at = x as usize * 4;
            row[at] = blend(sum[2], BACKGROUND[2], weight, denominator);
            row[at + 1] = blend(sum[1], BACKGROUND[1], weight, denominator);
            row[at + 2] = blend(sum[0], BACKGROUND[0], weight, denominator);
            row[at + 3] = 0xff;
        }
    }
}

#[inline(always)]
fn sample<const SIXTEEN: bool>(source: &[u8], at: usize) -> u64 {
    if SIXTEEN {
        (u16::from_ne_bytes([source[at], source[at + 1]]) >> 8) as u64
    } else {
        source[at] as u64
    }
}

#[inline]
fn blend(sum: u64, background: u8, weight: u64, denominator: u64) -> u8 {
    ((sum + background as u64 * weight + denominator / 2) / denominator) as u8
}

// maps a destination pixel back to its span in the source
pub fn scaled_range(position: u32, dest_size: u32, source_size: u32) -> (u32, u32) {
    let start = position * source_size / dest_size;
    let mut end = (position + 1) * source_size / dest_size;
    if end <= start {
        end = start + 1;
    }
    (start, end)
}

// maps a box in oriented coordinates back to the stored image
pub fn source_box(
    orientation: u8,
    left: u32,
    right: u32,
    top: u32,
    bottom: u32,
    width: u32,
    height: u32,
) -> (u32, u32, u32, u32) {
    match orientation {
        2 => (width - right, top, width - left, bottom),
        3 => (width - right, height - bottom, width - left, height - top),
        4 => (left, height - bottom, right, height - top),
        5 => (top, left, bottom, right),
        6 => (top, height - right, bottom, height - left),
        7 => (width - bottom, height - right, width - top, height - left),
        8 => (width - bottom, left, width - top, right),
        _ => (left, top, right, bottom),
    }
}

pub fn display_size(width: u32, height: u32, orientation: u8) -> (u32, u32) {
    if (5..=8).contains(&orientation) {
        (height, width)
    } else {
        (width, height)
    }
}

pub fn fit_scale(width: u32, height: u32, max_width: u32, max_height: u32) -> f64 {
    if max_width == 0 || max_height == 0 || (width <= max_width && height <= max_height) {
        return 1.0;
    }
    (max_width as f64 / width as f64).min(max_height as f64 / height as f64)
}

pub fn scaled_size(width: u32, height: u32, scale: f64) -> (u32, u32) {
    let scaled_width = (width as f64 * scale + 0.5) as u32;
    let scaled_height = (height as f64 * scale + 0.5) as u32;
    (scaled_width.max(1), scaled_height.max(1))
}

// jpeg segments are walked to the exif tag, 1 when the file has none
fn jpeg_orientation(path: &Path) -> Option<u8> {
    let mut file = File::open(path).ok()?;
    let mut header = [0u8; 2];
    file.read_exact(&mut header).ok()?;
    if header != [0xff, 0xd8] {
        return None;
    }

    loop {
        let mut byte = [0u8; 1];
        loop {
            file.read_exact(&mut byte).ok()?;
            if byte[0] != 0xff {
                return None;
            }
            file.read_exact(&mut byte).ok()?;
            if byte[0] != 0xff {
                break;
            }
        }

        let marker = byte[0];
        match marker {
            0xd8 | 0x01 | 0xd0..=0xd7 => continue,
            0xd9 | 0xda => return None,
            _ => {}
        }

        let mut size_bytes = [0u8; 2];
        file.read_exact(&mut size_bytes).ok()?;
        let size = u16::from_be_bytes(size_bytes) as i64;
        if size < 2 {
            return None;
        }

        if marker != 0xe1 {
            file.seek(SeekFrom::Current(size - 2)).ok()?;
            continue;
        }

        let mut payload = vec![0u8; (size - 2) as usize];
        file.read_exact(&mut payload).ok()?;
        if payload.len() >= 6 && &payload[..4] == b"Exif" && payload[4..6] == [0, 0] {
            return exif_orientation(&payload[6..]);
        }
    }
}

fn exif_orientation(tiff: &[u8]) -> Option<u8> {
    let little = match tiff.get(..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    if read_u16(tiff, 2, little)? != 42 {
        return None;
    }

    let ifd = read_u32(tiff, 4, little)? as usize;
    if ifd < 8 || ifd + 2 > tiff.len() {
        return None;
    }
    let count = read_u16(tiff, ifd, little)? as usize;
    for index in 0..count {
        let entry = ifd + 2 + index * 12;
        if entry + 12 > tiff.len() {
            return None;
        }
        if read_u16(tiff, entry, little)? != 0x0112 {
            continue;
        }
        if read_u16(tiff, entry + 2, little)? != 3 || read_u32(tiff, entry + 4, little)? != 1 {
            return None;
        }
        let value = read_u16(tiff, entry + 8, little)?;
        return if (2..=8).contains(&value) {
            Some(value as u8)
        } else {
            None
        };
    }
    None
}

fn read_u16(data: &[u8], at: usize, little: bool) -> Option<u16> {
    let bytes = data.get(at..at + 2)?;
    Some(if little {
        u16::from_le_bytes([bytes[0], bytes[1]])
    } else {
        u16::from_be_bytes([bytes[0], bytes[1]])
    })
}

fn read_u32(data: &[u8], at: usize, little: bool) -> Option<u32> {
    let bytes = data.get(at..at + 4)?;
    Some(if little {
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    } else {
        u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    })
}

pub fn open_reader(path: &Path) -> Result<BufReader<File>, String> {
    File::open(path)
        .map(BufReader::new)
        .map_err(|err| format!("{}: {}", path.display(), err))
}
