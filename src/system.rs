// Copyright (c) 2026 Greg
use std::mem::size_of;

use windows_sys::Win32::Foundation::{GlobalFree, HWND, POINT};
use windows_sys::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows_sys::Win32::System::Console::GetConsoleWindow;
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_NOCHANGEDIR,
    OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows_sys::Win32::UI::Shell::DROPFILES;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, MessageBoxW, HICON, ICONINFO, MB_ICONERROR,
};

use crate::win32::{file_name, wide};

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

// hands the file to the clipboard as a drop list, the way explorer's copy does
pub fn copy_path_to_clipboard(hwnd: HWND, path: &str) -> Result<(), String> {
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

    // cf_hdrop from winuser.h, the shell format for a list of dropped files
    const CF_HDROP: u32 = 15;

    let absolute = std::path::absolute(path).map_err(|err| err.to_string())?;
    let text: Vec<u16> = absolute.to_string_lossy().encode_utf16().collect();
    let header = size_of::<DROPFILES>();
    let size = header + (text.len() + 2) * size_of::<u16>();

    unsafe {
        let handle = GlobalAlloc(GMEM_MOVEABLE, size);
        if handle.is_null() {
            return Err("cannot allocate clipboard data".to_string());
        }

        let memory = GlobalLock(handle);
        if memory.is_null() {
            GlobalFree(handle);
            return Err("cannot lock the clipboard data".to_string());
        }

        let drop = memory as *mut DROPFILES;
        (*drop).pFiles = header as u32;
        (*drop).pt = POINT { x: 0, y: 0 };
        (*drop).fNC = 0;
        (*drop).fWide = 1;

        // the path follows the header and ends with an empty string
        let files = memory.cast::<u8>().add(header) as *mut u16;
        std::ptr::copy_nonoverlapping(text.as_ptr(), files, text.len());
        *files.add(text.len()) = 0;
        *files.add(text.len() + 1) = 0;

        GlobalUnlock(handle);

        if OpenClipboard(hwnd) == 0 {
            GlobalFree(handle);
            return Err("another program is using the clipboard".to_string());
        }

        // a stored handle belongs to the clipboard, a failed one is still ours
        let stored = EmptyClipboard() != 0 && !SetClipboardData(CF_HDROP, handle).is_null();
        CloseClipboard();
        if !stored {
            GlobalFree(handle);
            return Err(format!(
                "cannot copy {} to the clipboard",
                file_name(path)
            ));
        }
    }

    Ok(())
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
