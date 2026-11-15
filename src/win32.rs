// Copyright (c) 2026 Greg
use std::mem::size_of;
use std::path::Path;

use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, InvalidateRect, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;

// messages posted by the decode and playback threads
pub const WM_USER_FRAME: u32 = 0x0400 + 1;
pub const WM_USER_ANIM: u32 = 0x0400 + 2;

// dark borders around the image so it never sits under the nav buttons
pub const IMAGE_INSET_X: i32 = 56;
pub const IMAGE_INSET_Y: i32 = 28;

pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn client_size(hwnd: HWND) -> Option<(i32, i32)> {
    let mut rect: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetClientRect(hwnd, &mut rect) } == 0 {
        return None;
    }
    Some((rect.right - rect.left, rect.bottom - rect.top))
}

pub fn invalidate(hwnd: HWND) {
    unsafe {
        InvalidateRect(hwnd, std::ptr::null(), 0);
    }
}

pub fn mouse_point(lparam: LPARAM) -> (i32, i32) {
    let x = (lparam & 0xffff) as u16 as i16 as i32;
    let y = ((lparam >> 16) & 0xffff) as u16 as i16 as i32;
    (x, y)
}

pub fn wheel_delta(wparam: WPARAM) -> i32 {
    ((wparam >> 16) & 0xffff) as u16 as i16 as i32
}

pub fn work_area() -> (i32, i32, i32, i32) {
    unsafe {
        let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        if monitor.is_null() {
            return (0, 0, 0, 0);
        }
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return (0, 0, 0, 0);
        }
        (
            info.rcWork.left,
            info.rcWork.top,
            info.rcWork.right - info.rcWork.left,
            info.rcWork.bottom - info.rcWork.top,
        )
    }
}

pub fn initial_window_size(
    image_width: i32,
    image_height: i32,
    area_width: i32,
    area_height: i32,
) -> (i32, i32) {
    let mut width = image_width;
    let mut height = image_height;

    if area_width > 0 {
        let max_width = (area_width * 9 / 10 - 2 * IMAGE_INSET_X).max(1);
        if width > max_width {
            height = (height as i64 * max_width as i64 / width as i64) as i32;
            width = max_width;
        }
    }
    if area_height > 0 {
        let max_height = (area_height * 9 / 10 - 2 * IMAGE_INSET_Y).max(1);
        if height > max_height {
            width = (width as i64 * max_height as i64 / height as i64) as i32;
            height = max_height;
        }
    }

    (
        width.max(1) + 2 * IMAGE_INSET_X,
        height.max(1) + 2 * IMAGE_INSET_Y,
    )
}
