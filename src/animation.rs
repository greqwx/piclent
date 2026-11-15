// Copyright (c) 2026 Greg
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use gif::DisposalMethod;
use image::ColorType;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::decode::{self, Frame};
use crate::win32::WM_USER_ANIM;

pub struct Animation {
    pub frames: Vec<Arc<Vec<u8>>>,
    pub delays: Vec<u32>,
    pub loops: u32,
}

struct GifFrame {
    left: u32,
    top: u32,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    delay: u32,
    dispose: DisposalMethod,
}

struct GifCanvas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    saved: Vec<u8>,
    saved_rect: (u32, u32, u32, u32),
    previous: (u32, u32, u32, u32),
}

impl GifCanvas {
    fn new(width: u32, height: u32) -> GifCanvas {
        GifCanvas {
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 4],
            saved: Vec::new(),
            saved_rect: (0, 0, 0, 0),
            previous: (0, 0, 0, 0),
        }
    }

    fn advance(&mut self, index: usize, frames: &[GifFrame]) {
        if index > 0 {
            match frames[index - 1].dispose {
                DisposalMethod::Background => self.clear(self.previous),
                DisposalMethod::Previous => self.restore(),
                _ => {}
            }
        }

        let frame = &frames[index];
        if frame.left >= self.width || frame.top >= self.height {
            return;
        }
        let left = frame.left;
        let top = frame.top;
        let right = (left + frame.width).min(self.width);
        let bottom = (top + frame.height).min(self.height);
        if right <= left || bottom <= top {
            return;
        }

        if index + 1 < frames.len() && frame.dispose == DisposalMethod::Previous {
            self.saved_rect = (left, top, right, bottom);
            self.saved.clear();
            self.saved
                .reserve(((right - left) * (bottom - top) * 4) as usize);
            for y in top..bottom {
                let start = ((y * self.width + left) * 4) as usize;
                let end = ((y * self.width + right) * 4) as usize;
                self.saved.extend_from_slice(&self.pixels[start..end]);
            }
        }

        draw_over(
            &mut self.pixels,
            self.width,
            left,
            top,
            &frame.pixels,
            frame.width,
            right - left,
            bottom - top,
            left - frame.left,
            top - frame.top,
        );
        self.previous = (left, top, right, bottom);
    }

    fn clear(&mut self, rect: (u32, u32, u32, u32)) {
        let (left, top, right, bottom) = rect;
        for y in top..bottom {
            let start = ((y * self.width + left) * 4) as usize;
            let end = ((y * self.width + right) * 4) as usize;
            self.pixels[start..end].fill(0);
        }
    }

    fn restore(&mut self) {
        let (left, top, right, bottom) = self.saved_rect;
        if self.saved.is_empty() || left >= right || top >= bottom {
            return;
        }
        let row = ((right - left) * 4) as usize;
        for y in top..bottom {
            let target = ((y * self.width + left) * 4) as usize;
            let offset = ((y - top) as usize) * row;
            self.pixels[target..target + row].copy_from_slice(&self.saved[offset..offset + row]);
        }
    }
}

// src-over of a non premultiplied rgba frame onto the canvas
#[allow(clippy::too_many_arguments)]
fn draw_over(
    canvas: &mut [u8],
    canvas_width: u32,
    left: u32,
    top: u32,
    source: &[u8],
    source_width: u32,
    width: u32,
    height: u32,
    offset_x: u32,
    offset_y: u32,
) {
    for row in 0..height {
        let mut from = (((row + offset_y) * source_width + offset_x) * 4) as usize;
        let mut to = (((top + row) * canvas_width + left) * 4) as usize;
        for _ in 0..width {
            let alpha = source[from + 3] as u32;
            if alpha == 255 {
                canvas[to..to + 4].copy_from_slice(&source[from..from + 4]);
            } else if alpha > 0 {
                let rest = 255 - alpha;
                let under = canvas[to + 3] as u32;
                let result = alpha + under * rest / 255;
                if result > 0 {
                    for channel in 0..3 {
                        let value = (source[from + channel] as u32 * alpha
                            + canvas[to + channel] as u32 * under * rest / 255)
                            / result;
                        canvas[to + channel] = value.min(255) as u8;
                    }
                    canvas[to + 3] = result as u8;
                }
            }
            from += 4;
            to += 4;
        }
    }
}

enum Source<'a> {
    Gif {
        frames: &'a [GifFrame],
        canvas: GifCanvas,
    },
    Webp {
        frames: &'a [Vec<u8>],
        width: u32,
        height: u32,
        color: ColorType,
    },
}

impl Source<'_> {
    fn describe(&self) -> (u32, u32, ColorType) {
        match self {
            Source::Gif { canvas, .. } => (canvas.width, canvas.height, ColorType::Rgba8),
            Source::Webp {
                width,
                height,
                color,
                ..
            } => (*width, *height, *color),
        }
    }

    fn frame(&mut self, index: usize) -> (&[u8], u32, u32, ColorType) {
        match self {
            Source::Gif { frames, canvas } => {
                canvas.advance(index, frames);
                (
                    &canvas.pixels,
                    canvas.width,
                    canvas.height,
                    ColorType::Rgba8,
                )
            }
            Source::Webp {
                frames,
                width,
                height,
                color,
            } => (&frames[index], *width, *height, *color),
        }
    }
}

pub fn from_gif(
    path: &Path,
    max_width: u32,
    max_height: u32,
    anim_cap: usize,
) -> Result<Frame, String> {
    let reader = decode::open_reader(path)?;
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options
        .read_info(reader)
        .map_err(|err| format!("{}: {}", path.display(), err))?;

    let width = decoder.width() as u32;
    let height = decoder.height() as u32;
    let loops = match decoder.repeat() {
        gif::Repeat::Infinite => 0,
        gif::Repeat::Finite(count) => count as u32 + 1,
    };

    let mut frames = Vec::new();
    while let Some(frame) = decoder
        .read_next_frame()
        .map_err(|err| format!("{}: {}", path.display(), err))?
    {
        frames.push(GifFrame {
            left: frame.left as u32,
            top: frame.top as u32,
            width: frame.width as u32,
            height: frame.height as u32,
            pixels: frame.buffer.to_vec(),
            delay: frame_delay(frame.delay as u32),
            dispose: frame.dispose,
        });
    }
    if frames.is_empty() {
        return Err(format!("{}: empty gif", path.display()));
    }

    let delays: Vec<u32> = frames.iter().map(|frame| frame.delay).collect();
    let mut source = Source::Gif {
        frames: &frames,
        canvas: GifCanvas::new(width, height),
    };
    Ok(prepare(
        &mut source,
        &delays,
        loops,
        max_width,
        max_height,
        anim_cap,
    ))
}

pub fn from_webp(
    path: &Path,
    max_width: u32,
    max_height: u32,
    anim_cap: usize,
    full: bool,
) -> Result<Frame, String> {
    let reader = decode::open_reader(path)?;
    let mut decoder = image_webp::WebPDecoder::new(reader)
        .map_err(|err| format!("{}: {}", path.display(), err))?;

    let (width, height) = decoder.dimensions();
    let color = if decoder.has_alpha() {
        ColorType::Rgba8
    } else {
        ColorType::Rgb8
    };
    let frame_bytes = decoder
        .output_buffer_size()
        .ok_or_else(|| format!("{}: image is too large", path.display()))?;

    if !full || !decoder.is_animated() || decoder.num_frames() <= 1 {
        let mut buffer = vec![0u8; frame_bytes];
        decoder
            .read_image(&mut buffer)
            .map_err(|err| format!("{}: {}", path.display(), err))?;
        let scale = decode::fit_scale(width, height, max_width, max_height);
        let (draw_width, draw_height) = decode::scaled_size(width, height, scale);
        return Ok(Frame {
            pixels: Arc::new(decode::render(
                &buffer,
                color,
                1,
                width,
                height,
                draw_width,
                draw_height,
            )),
            width: draw_width,
            height: draw_height,
            anim: None,
            partial: !full,
        });
    }

    let loops = match decoder.loop_count() {
        image_webp::LoopCount::Forever => 0,
        image_webp::LoopCount::Times(count) => count.get() as u32,
    };

    let mut frames = Vec::with_capacity(decoder.num_frames() as usize);
    let mut delays = Vec::with_capacity(decoder.num_frames() as usize);
    let mut buffer = vec![0u8; frame_bytes];
    loop {
        match decoder.read_frame(&mut buffer) {
            Ok(delay) => {
                frames.push(buffer.clone());
                delays.push(delay.max(1) as u32);
            }
            Err(image_webp::DecodingError::NoMoreFrames) => break,
            Err(err) => return Err(format!("{}: {}", path.display(), err)),
        }
    }

    let mut source = Source::Webp {
        frames: &frames,
        width,
        height,
        color,
    };
    Ok(prepare(
        &mut source,
        &delays,
        loops,
        max_width,
        max_height,
        anim_cap,
    ))
}

// scales the frames to the window and keeps as many as the memory cap allows
fn prepare(
    source: &mut Source,
    delays: &[u32],
    loops: u32,
    max_width: u32,
    max_height: u32,
    anim_cap: usize,
) -> Frame {
    let (source_width, source_height, _) = source.describe();

    if delays.len() <= 1 {
        let (pixels, width, height, color) = source.frame(0);
        let scale = decode::fit_scale(width, height, max_width, max_height);
        let (draw_width, draw_height) = decode::scaled_size(width, height, scale);
        return Frame {
            pixels: Arc::new(decode::render(
                pixels,
                color,
                1,
                width,
                height,
                draw_width,
                draw_height,
            )),
            width: draw_width,
            height: draw_height,
            anim: None,
            partial: false,
        };
    }

    let scale = decode::fit_scale(source_width, source_height, max_width, max_height);
    let (mut draw_width, mut draw_height) = decode::scaled_size(source_width, source_height, scale);

    let step = frame_step(
        delays.len(),
        draw_width as usize * draw_height as usize * 4,
        anim_cap,
    );
    let kept = delays.len().div_ceil(step);
    let frame_bytes = draw_width as usize * draw_height as usize * 4;
    if kept * frame_bytes > anim_cap {
        let shrink = (anim_cap as f64 / (kept * frame_bytes) as f64).sqrt();
        (draw_width, draw_height) =
            decode::scaled_size(source_width, source_height, scale * shrink);
    }

    let mut frames = Vec::with_capacity(kept);
    let mut kept_delays = Vec::with_capacity(kept);
    let mut pending = 0;

    for index in 0..delays.len() {
        let (pixels, width, height, color) = source.frame(index);
        pending += delays[index];
        if index % step == 0 {
            frames.push(Arc::new(decode::render(
                pixels,
                color,
                1,
                width,
                height,
                draw_width,
                draw_height,
            )));
            kept_delays.push(pending);
            pending = 0;
        }
    }
    // the tail after the last kept frame still has to be shown
    if let Some(last) = kept_delays.last_mut() {
        *last += pending;
    }

    if frames.len() <= 1 {
        let pixels = frames.pop().unwrap_or_else(|| Arc::new(Vec::new()));
        return Frame {
            pixels,
            width: draw_width,
            height: draw_height,
            anim: None,
            partial: false,
        };
    }

    Frame {
        pixels: Arc::clone(&frames[0]),
        width: draw_width,
        height: draw_height,
        anim: Some(Arc::new(Animation {
            frames,
            delays: kept_delays,
            loops,
        })),
        partial: false,
    }
}

fn frame_step(count: usize, frame_bytes: usize, cap: usize) -> usize {
    let max_frames = (cap / frame_bytes.max(1)).max(1);
    count.div_ceil(max_frames).max(1)
}

// gif delays are in centiseconds, very fast frames get the floor browsers use
fn frame_delay(centiseconds: u32) -> u32 {
    if centiseconds == 0 {
        100
    } else if centiseconds * 10 < 20 {
        20
    } else {
        centiseconds * 10
    }
}

fn millis(value: u32) -> Duration {
    Duration::from_millis(value.max(1) as u64)
}

pub struct Playback {
    stop: Arc<(Mutex<bool>, Condvar)>,
}

impl Playback {
    pub fn start(
        hwnd: HWND,
        animation: Arc<Animation>,
        start_frame: usize,
        generation: u64,
    ) -> Playback {
        let stop = Arc::new((Mutex::new(false), Condvar::new()));
        let signal = Arc::clone(&stop);
        let hwnd = hwnd as isize;
        std::thread::spawn(move || run(hwnd, animation, start_frame, generation, signal));
        Playback { stop }
    }

    fn halt(&self) {
        let (lock, signal) = &*self.stop;
        if let Ok(mut stopped) = lock.lock() {
            *stopped = true;
        }
        signal.notify_all();
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        self.halt();
    }
}

fn run(
    hwnd: isize,
    animation: Arc<Animation>,
    start_frame: usize,
    generation: u64,
    stop: Arc<(Mutex<bool>, Condvar)>,
) {
    let _resolution = TimerResolution::begin();

    let mut index = start_frame.min(animation.delays.len().saturating_sub(1));
    let mut plays = 1;
    let mut next_at = Instant::now() + millis(animation.delays[index]);

    loop {
        {
            let (lock, signal) = &*stop;
            let Ok(mut stopped) = lock.lock() else {
                return;
            };
            loop {
                if *stopped {
                    return;
                }
                let now = Instant::now();
                if now >= next_at {
                    break;
                }
                match signal.wait_timeout(stopped, next_at - now) {
                    Ok((guard, _)) => stopped = guard,
                    Err(_) => return,
                }
            }
        }

        // the machine fell behind, jump to the frame that should be shown now
        loop {
            let mut next = index + 1;
            if next >= animation.frames.len() {
                plays += 1;
                if animation.loops > 0 && plays > animation.loops {
                    return;
                }
                next = 0;
            }
            index = next;
            next_at += millis(animation.delays[index]);
            if Instant::now() < next_at {
                break;
            }
        }

        unsafe {
            PostMessageW(
                hwnd as HWND,
                WM_USER_ANIM,
                generation as usize,
                index as isize,
            );
        }
    }
}

struct TimerResolution;

impl TimerResolution {
    fn begin() -> TimerResolution {
        unsafe {
            timeBeginPeriod(1);
        }
        TimerResolution
    }
}

impl Drop for TimerResolution {
    fn drop(&mut self) {
        unsafe {
            timeEndPeriod(1);
        }
    }
}
