// Copyright (c) 2026 Greg
use std::mem::size_of;

use windows_sys::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows_sys::Win32::System::Console::GetConsoleWindow;
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_NOCHANGEDIR,
    OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, MessageBoxW, HICON, ICONINFO, MB_ICONERROR,
};

use crate::win32::wide;

// asks for an image file, an empty result means the dialog was cancelled
pub fn open_image_dialog() -> Option<String> {
    let mut file = vec![0u16; 32768];
    let filter = utf16_filter(&[
        "Images (*.png;*.jpg;*.jpeg;*.gif;*.bmp;*.webp)",
        "*.png;*.jpg;*.jpeg;*.gif;*.bmp;*.webp",
        "All files (*.*)",
        "*.*",
    ]);
    let title = wide("Open image");

    let mut dialog: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    dialog.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    dialog.lpstrFilter = filter.as_ptr();
    dialog.nFilterIndex = 1;
    dialog.lpstrFile = file.as_mut_ptr();
    dialog.nMaxFile = file.len() as u32;
    dialog.lpstrTitle = title.as_ptr();
    dialog.Flags =
        OFN_HIDEREADONLY | OFN_NOCHANGEDIR | OFN_PATHMUSTEXIST | OFN_FILEMUSTEXIST | OFN_EXPLORER;

    if unsafe { GetOpenFileNameW(&mut dialog) } == 0 {
        return None;
    }

    let end = file
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(file.len());
    Some(String::from_utf16_lossy(&file[..end]))
}

// joins dialog filter strings into the double null terminated list
fn utf16_filter(parts: &[&str]) -> Vec<u16> {
    let mut out = Vec::new();
    for part in parts {
        out.extend(part.encode_utf16());
        out.push(0);
    }
    out.push(0);
    out
}

pub fn fatal(message: &str) {
    eprintln!("{message}");

    unsafe {
        if !GetConsoleWindow().is_null() {
            return;
        }
        let text = wide(message);
        let title = wide("piclent");
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_ICONERROR,
        );
    }
}

pub fn load_icon_from_png(data: &[u8]) -> HICON {
    let Ok(decoded) = image::load_from_memory(data) else {
        return std::ptr::null_mut();
    };
    let image = decoded.into_rgba8();
    let (width, height) = image.dimensions();

    let mut bgra = Vec::with_capacity((width * height * 4) as usize);
    for pixel in image.pixels() {
        bgra.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }

    unsafe {
        let color = CreateBitmap(
            width as i32,
            height as i32,
            1,
            32,
            bgra.as_ptr() as *const _,
        );
        if color.is_null() {
            return std::ptr::null_mut();
        }

        // a 1 bit mask of zeroes means the color bitmap carries all the pixels
        let mask_size = ((width + 15) / 16) * 2 * height;
        let mask_bits = vec![0u8; mask_size as usize];
        let mask = CreateBitmap(
            width as i32,
            height as i32,
            1,
            1,
            mask_bits.as_ptr() as *const _,
        );
        if mask.is_null() {
            DeleteObject(color);
            return std::ptr::null_mut();
        }

        let info = ICONINFO {
            fIcon: 1,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = CreateIconIndirect(&info);

        DeleteObject(color);
        DeleteObject(mask);
        icon
    }
}
