// Copyright (c) 2026 Greg
use std::ffi::c_void;
use std::mem::size_of;
use std::sync::Arc;

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    AlphaBlend, BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateDIBSection,
    CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, GetDC,
    GetMonitorInfoW, GetStockObject, MonitorFromWindow, ReleaseDC, SelectObject, SetBkMode,
    SetBrushOrgEx, SetStretchBltMode, SetTextColor, StretchDIBits, AC_SRC_ALPHA, BITMAPINFO,
    BITMAPINFOHEADER, BLENDFUNCTION, DEFAULT_GUI_FONT, DIB_RGB_COLORS, DT_CALCRECT, DT_CENTER,
    DT_WORDBREAK, HALFTONE, HBITMAP, HDC, HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    PAINTSTRUCT, SRCCOPY, TRANSPARENT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, GetWindowPlacement, PostMessageW, SetWindowLongPtrW, SetWindowPlacement,
    SetWindowPos, SetWindowTextW, GWL_STYLE, HWND_NOTOPMOST, HWND_TOPMOST, SWP_FRAMECHANGED,
    SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, WM_CLOSE, WS_POPUP, WS_VISIBLE,
};

use crate::animation::{Animation, Playback};
use crate::gallery::{recycle_file, Gallery};
use crate::win32::{self, invalidate, wide, IMAGE_INSET_X, IMAGE_INSET_Y};

pub const MIN_ZOOM: f64 = 1.0;
pub const MAX_ZOOM: f64 = 8.0;
pub const WHEEL_ZOOM: f64 = 1.2;

const NAV_BUTTON_SIZE: i32 = 30;
const NAV_BUTTON_MARGIN: i32 = 12;
const NAV_BUTTON_ALPHA: f64 = 0.75;
const NAV_ARROW_ALPHA: f64 = 0.92;

const BACKGROUND_COLOR: u32 = 0x28201c;
const MESSAGE_COLOR: u32 = 0x00e0e0e0;
const NAV_BUTTON_COLOR: u32 = 0x003c312c;
const NAV_ARROW_COLOR: u32 = 0x00efe9e6;

pub struct Viewer {
    hwnd: HWND,
    gallery: Gallery,
    pixels: Arc<Vec<u8>>,
    width: i32,
    height: i32,
    message: String,

    anim: Option<Arc<Animation>>,
    anim_frame: usize,
    anim_gen: u64,
    playback: Option<Playback>,
    paused: bool,
    suspended: bool,

    zoom: f64,
    offset_x: f64,
    offset_y: f64,

    dragging: bool,
    drag_x: i32,
    drag_y: i32,

    fullscreen: bool,
    saved_style: isize,
    saved_placement: windows_sys::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT,

    mem_dc: HDC,
    mem_bitmap: HBITMAP,
    mem_old: HGDIOBJ,
    mem_width: i32,
    mem_height: i32,

    nav_prev: Option<NavGlyph>,
    nav_next: Option<NavGlyph>,
}

#[derive(Default, Clone, Copy)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f64,
}

impl Viewer {
    pub fn new(hwnd: HWND, gallery: Gallery) -> Viewer {
        Viewer {
            hwnd,
            gallery,
            pixels: Arc::new(Vec::new()),
            width: 0,
            height: 0,
            message: String::new(),
            anim: None,
            anim_frame: 0,
            anim_gen: 0,
            playback: None,
            paused: false,
            suspended: false,
            zoom: MIN_ZOOM,
            offset_x: 0.0,
            offset_y: 0.0,
            dragging: false,
            drag_x: 0,
            drag_y: 0,
            fullscreen: false,
            saved_style: 0,
            saved_placement: unsafe { std::mem::zeroed() },
            mem_dc: std::ptr::null_mut(),
            mem_bitmap: std::ptr::null_mut(),
            mem_old: std::ptr::null_mut(),
            mem_width: 0,
            mem_height: 0,
            nav_prev: None,
            nav_next: None,
        }
    }

    pub fn start(&mut self, max_width: i32, max_height: i32) {
        self.gallery
            .start(self.hwnd, max_width as u32, max_height as u32);
    }

    // where the image lands in the client area, clamping the pan
    pub fn layout(&mut self, client_width: i32, client_height: i32) -> Geometry {
        if self.width <= 0 || self.height <= 0 {
            return Geometry::default();
        }

        let (inset_x, inset_y) = image_insets(client_width, client_height);
        let inner_width = client_width - 2 * inset_x;
        let inner_height = client_height - 2 * inset_y;

        let (fit_width, fit_height) = fit_size(self.width, self.height, inner_width, inner_height);
        if fit_width <= 0 || fit_height <= 0 {
            return Geometry::default();
        }

        let scale = fit_width as f64 / self.width as f64 * self.zoom;
        let draw_width = ((self.width as f64 * scale).round() as i32).max(1);
        let draw_height = ((self.height as f64 * scale).round() as i32).max(1);

        self.offset_x = clamp_offset(self.offset_x, draw_width, client_width);
        self.offset_y = clamp_offset(self.offset_y, draw_height, client_height);

        Geometry {
            x: inset_x + (inner_width - draw_width) / 2 + self.offset_x.round() as i32,
            y: inset_y + (inner_height - draw_height) / 2 + self.offset_y.round() as i32,
            width: draw_width,
            height: draw_height,
            scale,
        }
    }

    // zooms around the cursor, keeping the point under it in place
    pub fn zoom_wheel(&mut self, screen_x: i32, screen_y: i32, delta: i32) {
        if delta == 0 {
            return;
        }
        let mut point = windows_sys::Win32::Foundation::POINT {
            x: screen_x,
            y: screen_y,
        };
        unsafe {
            windows_sys::Win32::Graphics::Gdi::ScreenToClient(self.hwnd, &mut point);
        }
        self.zoom_at(point.x, point.y, WHEEL_ZOOM.powf(delta as f64 / 120.0));
    }

    fn zoom_at(&mut self, cursor_x: i32, cursor_y: i32, factor: f64) {
        if self.pixels.is_empty() || self.width <= 0 || self.height <= 0 {
            return;
        }
        let Some((width, height)) = win32::client_size(self.hwnd) else {
            return;
        };
        if width <= 0 || height <= 0 {
            return;
        }

        let current = self.layout(width, height);
        if current.scale <= 0.0 {
            return;
        }

        // never shrink a small image past its own pixels, large ones stop at fit
        let native = self.zoom / current.scale;
        let lowest = MIN_ZOOM.min(native);
        let zoom = (self.zoom * factor).clamp(lowest, MAX_ZOOM);
        if zoom == self.zoom {
            return;
        }

        let image_x = (cursor_x as f64 - current.x as f64) / current.scale;
        let image_y = (cursor_y as f64 - current.y as f64) / current.scale;

        let scale = current.scale / self.zoom * zoom;
        self.zoom = zoom;
        let draw_width = ((self.width as f64 * scale).round() as i32).max(1);
        let draw_height = ((self.height as f64 * scale).round() as i32).max(1);

        self.offset_x = cursor_x as f64 - image_x * scale - ((width - draw_width) / 2) as f64;
        self.offset_y = cursor_y as f64 - image_y * scale - ((height - draw_height) / 2) as f64;
        self.layout(width, height);

        invalidate(self.hwnd);
    }

    // zooms around the center of the view
    pub fn zoom_step(&mut self, factor: f64) {
        let Some((width, height)) = win32::client_size(self.hwnd) else {
            return;
        };
        if self.pixels.is_empty() || width <= 0 || height <= 0 {
            return;
        }
        self.zoom_at(width / 2, height / 2, factor);
    }

    // switches between fitting the window and 1:1 pixels
    pub fn toggle_actual_size(&mut self) {
        if self.pixels.is_empty() || self.width <= 0 || self.height <= 0 {
            return;
        }
        let Some((width, height)) = win32::client_size(self.hwnd) else {
            return;
        };
        if width <= 0 || height <= 0 {
            return;
        }

        let current = self.layout(width, height);
        if current.scale <= 0.0 {
            return;
        }
        let native = self.zoom / current.scale;

        let zoom = if (self.zoom - MIN_ZOOM).abs() < 0.001 {
            native.clamp(0.001, MAX_ZOOM)
        } else {
            MIN_ZOOM
        };
        if zoom == self.zoom {
            return;
        }

        self.zoom = zoom;
        self.offset_x = 0.0;
        self.offset_y = 0.0;
        invalidate(self.hwnd);
    }

    pub fn has_navigation(&self) -> bool {
        self.gallery.count() > 1
    }

    // switches to the neighboring image and resets the view
    pub fn navigate(&mut self, direction: isize) {
        if !self.has_navigation() || !self.gallery.step(direction) {
            return;
        }
        self.zoom = MIN_ZOOM;
        self.offset_x = 0.0;
        self.offset_y = 0.0;
        self.message.clear();
        self.paused = false;
        self.update_title();
        self.refresh();
    }

    pub fn update_title(&self) {
        let title = wide(&self.gallery.current_name());
        unsafe {
            SetWindowTextW(self.hwnd, title.as_ptr());
        }
    }

    // pulls the current frame from the cache or switches to its error
    pub fn refresh(&mut self) {
        let (frame, message) = self.gallery.snapshot();
        if let Some(message) = message {
            self.message = message;
            self.pixels = Arc::new(Vec::new());
            self.width = 0;
            self.height = 0;
            self.anim = None;
            self.anim_frame = 0;
            self.update_playback();
        } else if let Some(frame) = frame {
            self.message.clear();
            self.width = frame.width as i32;
            self.height = frame.height as i32;
            self.anim = frame.anim.clone();
            self.anim_frame = 0;
            self.pixels = match &self.anim {
                Some(anim) if !anim.frames.is_empty() => Arc::clone(&anim.frames[0]),
                _ => Arc::clone(&frame.pixels),
            };
            self.update_playback();
        } else {
            // the next image is still decoding, keep the previous one frozen
            self.stop_playback();
        }
        invalidate(self.hwnd);
    }

    // starts the frame ticker only when there is something to play
    fn update_playback(&mut self) {
        self.stop_playback();
        let Some(anim) = self.anim.clone() else {
            return;
        };
        if anim.frames.len() < 2 || self.paused || self.suspended {
            return;
        }
        self.playback = Some(Playback::start(
            self.hwnd,
            anim,
            self.anim_frame,
            self.anim_gen,
        ));
    }

    fn stop_playback(&mut self) {
        // bumping the generation drops frames already queued in the message loop
        self.anim_gen += 1;
        self.playback = None;
    }

    pub fn show_animation_frame(&mut self, generation: u64, index: usize) {
        let Some(anim) = &self.anim else {
            return;
        };
        if generation != self.anim_gen || index >= anim.frames.len() {
            return;
        }
        self.anim_frame = index;
        self.pixels = Arc::clone(&anim.frames[index]);
        invalidate(self.hwnd);
    }

    pub fn toggle_pause(&mut self) {
        let Some(anim) = &self.anim else {
            return;
        };
        if anim.frames.len() < 2 {
            return;
        }
        self.paused = !self.paused;
        self.update_playback();
    }

    pub fn set_suspended(&mut self, suspended: bool) {
        if self.suspended == suspended {
            return;
        }
        self.suspended = suspended;
        self.update_playback();
    }

    pub fn toggle_fullscreen(&mut self) {
        if self.fullscreen {
            self.exit_fullscreen();
        } else {
            self.enter_fullscreen();
        }
    }

    // covers the current monitor with a borderless topmost window
    fn enter_fullscreen(&mut self) {
        unsafe {
            let mut saved: windows_sys::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT =
                std::mem::zeroed();
            saved.length =
                size_of::<windows_sys::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT>() as u32;
            if GetWindowPlacement(self.hwnd, &mut saved) == 0 {
                return;
            }

            let monitor = MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST);
            if monitor.is_null() {
                return;
            }
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut info) == 0 {
                return;
            }

            self.saved_style = GetWindowLongPtrW(self.hwnd, GWL_STYLE);
            self.saved_placement = saved;
            self.fullscreen = true;

            SetWindowLongPtrW(self.hwnd, GWL_STYLE, (WS_POPUP | WS_VISIBLE) as isize);
            SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                info.rcMonitor.left,
                info.rcMonitor.top,
                info.rcMonitor.right - info.rcMonitor.left,
                info.rcMonitor.bottom - info.rcMonitor.top,
                SWP_FRAMECHANGED | SWP_SHOWWINDOW,
            );
        }
    }

    pub fn exit_fullscreen(&mut self) {
        if !self.fullscreen {
            return;
        }
        self.fullscreen = false;

        unsafe {
            SetWindowLongPtrW(self.hwnd, GWL_STYLE, self.saved_style);
            SetWindowPlacement(self.hwnd, &self.saved_placement);
            SetWindowPos(
                self.hwnd,
                HWND_NOTOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED | SWP_SHOWWINDOW,
            );
        }
        invalidate(self.hwnd);
    }

    // moves the open file to the recycle bin and shows the next one
    pub fn delete_current(&mut self) {
        let path = self.gallery.current_path();
        if path.is_empty() {
            return;
        }

        if let Err(message) = recycle_file(&path) {
            self.message = message;
            self.pixels = Arc::new(Vec::new());
            self.width = 0;
            self.height = 0;
            self.anim = None;
            self.anim_frame = 0;
            self.stop_playback();
            invalidate(self.hwnd);
            return;
        }

        if self.gallery.forget_current() == 0 {
            unsafe {
                PostMessageW(self.hwnd, WM_CLOSE, 0, 0);
            }
            return;
        }

        self.zoom = MIN_ZOOM;
        self.offset_x = 0.0;
        self.offset_y = 0.0;
        self.message.clear();
        self.paused = false;
        self.update_title();
        self.refresh();
    }

    // switches to a file dropped onto the window
    pub fn open_path(&mut self, path: &str) {
        self.gallery.reset(path);

        self.zoom = MIN_ZOOM;
        self.offset_x = 0.0;
        self.offset_y = 0.0;
        self.message.clear();
        self.pixels = Arc::new(Vec::new());
        self.width = 0;
        self.height = 0;
        self.anim = None;
        self.anim_frame = 0;
        self.paused = false;
        self.stop_playback();
        self.update_title();
        self.refresh();
    }

    pub fn pan(&mut self, dx: i32, dy: i32) {
        if dx == 0 && dy == 0 {
            return;
        }
        self.offset_x += dx as f64;
        self.offset_y += dy as f64;

        let Some((width, height)) = win32::client_size(self.hwnd) else {
            return;
        };
        self.layout(width, height);
        invalidate(self.hwnd);
    }

    // returns -1 for the previous button, 1 for the next one, 0 for none
    pub fn button_at(&self, x: i32, y: i32) -> i32 {
        if !self.has_navigation() {
            return 0;
        }
        let Some((width, height)) = win32::client_size(self.hwnd) else {
            return 0;
        };
        let (previous, next) = nav_button_rects(width, height);
        if point_in_rect(&previous, x, y) {
            return -1;
        }
        if point_in_rect(&next, x, y) {
            return 1;
        }
        0
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    pub fn begin_drag(&mut self, x: i32, y: i32) {
        self.dragging = true;
        self.drag_x = x;
        self.drag_y = y;
        unsafe {
            windows_sys::Win32::UI::Input::KeyboardAndMouse::SetCapture(self.hwnd);
        }
    }

    pub fn drag_to(&mut self, x: i32, y: i32) {
        let (dx, dy) = (x - self.drag_x, y - self.drag_y);
        self.drag_x = x;
        self.drag_y = y;
        self.pan(dx, dy);
    }

    pub fn end_drag(&mut self) {
        if self.dragging {
            self.dragging = false;
            unsafe {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
            }
        }
    }

    pub fn cancel_drag(&mut self) {
        self.dragging = false;
    }

    pub fn paint(&mut self) {
        unsafe {
            let mut paint: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(self.hwnd, &mut paint);
            if !hdc.is_null() {
                self.paint_client(hdc);
            }
            EndPaint(self.hwnd, &paint);
        }
    }

    fn paint_client(&mut self, hdc: HDC) {
        let Some((width, height)) = win32::client_size(self.hwnd) else {
            return;
        };
        if width <= 0 || height <= 0 {
            return;
        }

        let target = if self.ensure_back_buffer(hdc, width, height) {
            self.mem_dc
        } else {
            hdc
        };

        unsafe {
            let brush = CreateSolidBrush(BACKGROUND_COLOR);
            let rect = RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            };
            FillRect(target, &rect, brush);
            DeleteObject(brush);
        }

        if !self.message.is_empty() {
            draw_message(target, width, height, &self.message);
        } else if !self.pixels.is_empty() && self.width > 0 && self.height > 0 {
            let geometry = self.layout(width, height);
            if geometry.width > 0 && geometry.height > 0 {
                unsafe {
                    SetStretchBltMode(target, HALFTONE);
                    SetBrushOrgEx(target, 0, 0, std::ptr::null_mut());

                    let mut info: BITMAPINFO = std::mem::zeroed();
                    info.bmiHeader = BITMAPINFOHEADER {
                        biSize: size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: self.width,
                        biHeight: -self.height,
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: 0,
                        biSizeImage: self.pixels.len() as u32,
                        biXPelsPerMeter: 0,
                        biYPelsPerMeter: 0,
                        biClrUsed: 0,
                        biClrImportant: 0,
                    };

                    StretchDIBits(
                        target,
                        geometry.x,
                        geometry.y,
                        geometry.width,
                        geometry.height,
                        0,
                        0,
                        self.width,
                        self.height,
                        self.pixels.as_ptr() as *const c_void,
                        &info,
                        DIB_RGB_COLORS,
                        SRCCOPY,
                    );
                }
            }
        }

        if self.has_navigation() {
            let (previous, next) = nav_button_rects(width, height);
            self.draw_nav_button(target, previous, -1);
            self.draw_nav_button(target, next, 1);
        }

        if target != hdc {
            unsafe {
                BitBlt(hdc, 0, 0, width, height, target, 0, 0, SRCCOPY);
            }
        }
    }

    fn ensure_back_buffer(&mut self, hdc: HDC, width: i32, height: i32) -> bool {
        if !self.mem_dc.is_null() && self.mem_width == width && self.mem_height == height {
            return true;
        }
        self.release_back_buffer();

        unsafe {
            let memory = CreateCompatibleDC(hdc);
            if memory.is_null() {
                return false;
            }
            let bitmap = CreateCompatibleBitmap(hdc, width, height);
            if bitmap.is_null() {
                DeleteDC(memory);
                return false;
            }
            let old = SelectObject(memory, bitmap);

            self.mem_dc = memory;
            self.mem_bitmap = bitmap;
            self.mem_old = old;
            self.mem_width = width;
            self.mem_height = height;
        }
        true
    }

    fn release_back_buffer(&mut self) {
        if self.mem_dc.is_null() {
            return;
        }
        unsafe {
            if !self.mem_old.is_null() {
                SelectObject(self.mem_dc, self.mem_old);
            }
            DeleteObject(self.mem_bitmap);
            DeleteDC(self.mem_dc);
        }
        self.mem_dc = std::ptr::null_mut();
        self.mem_bitmap = std::ptr::null_mut();
        self.mem_old = std::ptr::null_mut();
        self.mem_width = 0;
        self.mem_height = 0;
    }

    fn nav_glyph(&mut self, size: i32, direction: i32) -> Option<&NavGlyph> {
        let slot = if direction < 0 {
            &mut self.nav_prev
        } else {
            &mut self.nav_next
        };
        if slot.as_ref().map(|glyph| glyph.size) != Some(size) {
            *slot = NavGlyph::new(size, direction);
        }
        slot.as_ref()
    }

    fn draw_nav_button(&mut self, hdc: HDC, rect: RECT, direction: i32) {
        let size = rect.right - rect.left;
        let Some(glyph) = self.nav_glyph(size, direction) else {
            return;
        };
        let blend = BLENDFUNCTION {
            BlendOp: 0,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        unsafe {
            AlphaBlend(
                hdc,
                rect.left,
                rect.top,
                size,
                rect.bottom - rect.top,
                glyph.dc,
                0,
                0,
                size,
                glyph.size,
                blend,
            );
        }
    }

    pub fn shutdown(&mut self) {
        self.stop_playback();
        self.release_back_buffer();
        self.nav_prev = None;
        self.nav_next = None;
    }
}

impl Drop for Viewer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// an antialiased button image with per pixel alpha
struct NavGlyph {
    dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    size: i32,
}

impl NavGlyph {
    fn new(size: i32, direction: i32) -> Option<NavGlyph> {
        if size <= 0 {
            return None;
        }
        unsafe {
            let screen = GetDC(std::ptr::null_mut());
            if screen.is_null() {
                return None;
            }
            let dc = CreateCompatibleDC(screen);
            ReleaseDC(std::ptr::null_mut(), screen);
            if dc.is_null() {
                return None;
            }

            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0,
                biSizeImage: (size * size * 4) as u32,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            };

            let mut bits: *mut c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(
                dc,
                &info,
                DIB_RGB_COLORS,
                &mut bits,
                std::ptr::null_mut(),
                0,
            );
            if bitmap.is_null() {
                DeleteDC(dc);
                return None;
            }
            let old = SelectObject(dc, bitmap);
            draw_glyph(bits as *mut u8, size, direction);
            Some(NavGlyph {
                dc,
                bitmap,
                old,
                size,
            })
        }
    }
}

impl Drop for NavGlyph {
    fn drop(&mut self) {
        unsafe {
            if !self.old.is_null() {
                SelectObject(self.dc, self.old);
            }
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}

// paints a translucent circle with a chevron into a top-down 32 bit dib,
// premultiplying alpha so alphablend can composite it
fn draw_glyph(bits: *mut u8, size: i32, direction: i32) {
    if bits.is_null() || size <= 0 {
        return;
    }
    let pixels = unsafe { std::slice::from_raw_parts_mut(bits, (size * size * 4) as usize) };

    let extent = size as f64;
    let center = extent / 2.0;
    let radius = center - 0.35;
    let width = extent * 0.17;
    let arm = extent * 0.24;
    let stroke = extent * 0.05;

    let (vertex_x, arm_x) = if direction < 0 {
        (center - width, center + width)
    } else {
        (center + width, center - width)
    };
    // nudge the chevron toward the side it points to, a bare vertex reads
    // off-centre otherwise
    let shift = extent * 0.05 * direction as f64;
    let (first_x, first_y) = (arm_x + shift, center - arm);
    let (corner_x, corner_y) = (vertex_x + shift, center);
    let (last_x, last_y) = (arm_x + shift, center + arm);

    let (button_r, button_g, button_b) = color_parts(NAV_BUTTON_COLOR);
    let (arrow_r, arrow_g, arrow_b) = color_parts(NAV_ARROW_COLOR);

    for y in 0..size {
        for x in 0..size {
            let cx = x as f64 + 0.5;
            let cy = y as f64 + 0.5;

            let circle = coverage(radius - (cx - center).hypot(cy - center));
            let mut alpha = circle * NAV_BUTTON_ALPHA;
            let mut red = button_r * alpha;
            let mut green = button_g * alpha;
            let mut blue = button_b * alpha;

            let mut chevron = 0.0;
            if circle > 0.0 {
                let distance = segment_distance(cx, cy, first_x, first_y, corner_x, corner_y)
                    .min(segment_distance(cx, cy, corner_x, corner_y, last_x, last_y));
                chevron = coverage(stroke - distance) * circle;
            }
            if chevron > 0.0 {
                let source = chevron * NAV_ARROW_ALPHA;
                red = arrow_r * source + red * (1.0 - source);
                green = arrow_g * source + green * (1.0 - source);
                blue = arrow_b * source + blue * (1.0 - source);
                alpha = source + alpha * (1.0 - source);
            }

            let at = ((y * size + x) * 4) as usize;
            pixels[at] = (blue * 255.0).round() as u8;
            pixels[at + 1] = (green * 255.0).round() as u8;
            pixels[at + 2] = (red * 255.0).round() as u8;
            pixels[at + 3] = (alpha * 255.0).round() as u8;
        }
    }
}

// turns a signed distance to a shape edge into a 0..1 pixel share
fn coverage(distance: f64) -> f64 {
    if distance >= 0.5 {
        1.0
    } else if distance <= -0.5 {
        0.0
    } else {
        distance + 0.5
    }
}

fn segment_distance(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let (dx, dy) = (bx - ax, by - ay);
    let length = dx * dx + dy * dy;
    let mut t = 0.0;
    if length > 0.0 {
        t = (((px - ax) * dx + (py - ay) * dy) / length).clamp(0.0, 1.0);
    }
    (px - (ax + t * dx)).hypot(py - (ay + t * dy))
}

// splits a colorref (0x00bbggrr) into normalized components
fn color_parts(color: u32) -> (f64, f64, f64) {
    (
        (color & 0xff) as f64 / 255.0,
        ((color >> 8) & 0xff) as f64 / 255.0,
        ((color >> 16) & 0xff) as f64 / 255.0,
    )
}

fn fit_size(image_width: i32, image_height: i32, width: i32, height: i32) -> (i32, i32) {
    if image_width <= 0 || image_height <= 0 || width <= 0 || height <= 0 {
        return (0, 0);
    }
    let mut draw_width = width;
    let mut draw_height = (image_height as i64 * width as i64 / image_width as i64) as i32;
    if draw_height > height {
        draw_height = height;
        draw_width = (image_width as i64 * height as i64 / image_height as i64) as i32;
    }
    (draw_width, draw_height)
}

// keeps dark borders at the window edges where the buttons live
fn image_insets(width: i32, height: i32) -> (i32, i32) {
    let mut inset_x = IMAGE_INSET_X;
    let mut inset_y = IMAGE_INSET_Y;
    if width - 2 * inset_x < 1 {
        inset_x = 0;
    }
    if height - 2 * inset_y < 1 {
        inset_y = 0;
    }
    (inset_x, inset_y)
}

fn clamp_offset(offset: f64, draw_size: i32, client_size: i32) -> f64 {
    if draw_size <= client_size {
        return 0.0;
    }
    let limit = (draw_size - client_size) as f64 / 2.0;
    offset.clamp(-limit, limit)
}

fn nav_button_rects(width: i32, height: i32) -> (RECT, RECT) {
    let mut size = NAV_BUTTON_SIZE;
    if height < size * 3 {
        size = (height / 3).max(size / 2);
    }
    let margin = NAV_BUTTON_MARGIN;
    let top = (height - size) / 2;
    (
        RECT {
            left: margin,
            top,
            right: margin + size,
            bottom: top + size,
        },
        RECT {
            left: width - margin - size,
            top,
            right: width - margin,
            bottom: top + size,
        },
    )
}

fn point_in_rect(rect: &RECT, x: i32, y: i32) -> bool {
    x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom
}

fn draw_message(hdc: HDC, width: i32, height: i32, message: &str) {
    unsafe {
        let font = GetStockObject(DEFAULT_GUI_FONT);
        let old = SelectObject(hdc, font);
        SetBkMode(hdc, TRANSPARENT as i32);
        SetTextColor(hdc, MESSAGE_COLOR);

        let text = wide(message);
        let mut rect = RECT {
            left: 40,
            top: 40,
            right: width - 40,
            bottom: height - 40,
        };

        let mut measured = rect;
        DrawTextW(
            hdc,
            text.as_ptr(),
            -1,
            &mut measured,
            DT_CALCRECT | DT_CENTER | DT_WORDBREAK,
        );
        let text_height = measured.bottom - measured.top;
        let top = (height - text_height) / 2;
        rect.top = top;
        rect.bottom = top + text_height;

        DrawTextW(hdc, text.as_ptr(), -1, &mut rect, DT_CENTER | DT_WORDBREAK);

        SelectObject(hdc, old);
    }
}
