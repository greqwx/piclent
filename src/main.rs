// Copyright (c) 2026 Greg
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod animation;
mod decode;
mod gallery;
mod metadata;
mod system;
mod viewer;
mod win32;
mod window;

use std::mem::size_of;
use std::path::Path;

use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows_sys::Win32::Graphics::Gdi::{UpdateWindow, COLOR_WINDOW, HBRUSH};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::DragAcceptFiles;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRect, CreateWindowExW, DispatchMessageW, GetMessageW, LoadCursorW,
    RegisterClassExW, SetWindowLongPtrW, ShowWindow, TranslateMessage, CS_DBLCLKS, CS_HREDRAW,
    CS_VREDRAW, GWL_USERDATA, IDC_ARROW, MSG, SW_SHOW, WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
};

use crate::gallery::Gallery;
use crate::viewer::Viewer;
use crate::win32::wide;

const ICON_16: &[u8] = include_bytes!("../assets/icon-16.png");
const ICON_128: &[u8] = include_bytes!("../assets/icon-128.png");

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let (path, budget) = match parse_args(std::env::args().skip(1)) {
        Ok(value) => value,
        Err(message) => {
            system::fatal(&message);
            return 2;
        }
    };

    let path = match path {
        Some(path) => path,
        None => match system::open_image_dialog() {
            Some(path) => path,
            None => return 0,
        },
    };

    let size = match decode::image_size(Path::new(&path)) {
        Ok(size) => size,
        Err(message) => {
            system::fatal(&message);
            return 1;
        }
    };

    let gallery = Gallery::new(&path, budget as usize * 1024 * 1024);
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };

    let icon = system::load_icon_from_png(ICON_128);
    let icon_small = system::load_icon_from_png(ICON_16);
    let class_name = wide("piclent_wnd");

    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
        lpfnWndProc: Some(window::wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: instance,
        hIcon: icon,
        hCursor: unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) },
        hbrBackground: (COLOR_WINDOW + 1) as usize as HBRUSH,
        lpszMenuName: std::ptr::null(),
        lpszClassName: class_name.as_ptr(),
        hIconSm: icon_small,
    };
    if unsafe { RegisterClassExW(&class) } == 0 {
        system::fatal("RegisterClassEx failed");
        return 1;
    }

    let (monitor_x, monitor_y, monitor_width, monitor_height) = win32::work_area();
    let (window_width, window_height) =
        win32::initial_window_size(size.0 as i32, size.1 as i32, monitor_width, monitor_height);
    let x = monitor_x + (monitor_width - window_width) / 2;
    let y = monitor_y + (monitor_height - window_height) / 2;

    let title = wide(&win32::file_name(&path));
    let style = WS_OVERLAPPEDWINDOW;
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: window_width,
        bottom: window_height,
    };
    unsafe {
        AdjustWindowRect(&mut rect, style, 0);
    }

    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            style,
            x,
            y,
            rect.right - rect.left,
            rect.bottom - rect.top,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        system::fatal("CreateWindow failed");
        return 1;
    }

    let dark = 1i32;
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            &dark as *const i32 as *const _,
            size_of::<i32>() as u32,
        );
        DragAcceptFiles(hwnd, 1);
    }

    let viewer = Box::into_raw(Box::new(Viewer::new(hwnd, gallery)));
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_USERDATA, viewer as isize);
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
    }

    // decoding starts after the first paint, the frame arrives by message
    unsafe {
        (*viewer).start(monitor_width, monitor_height);
    }

    let mut message: MSG = unsafe { std::mem::zeroed() };
    loop {
        let result = unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) };
        if result == 0 || result == -1 {
            break;
        }
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    0
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<(Option<String>, u32), String> {
    const USAGE: &str = "Usage: piclent.exe [image]";

    let args: Vec<String> = args.collect();
    let mut budget = 512;
    let mut path = None;

    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if argument == "-m" || argument == "--m" {
            index += 1;
            let value = args.get(index).ok_or(USAGE)?;
            budget = parse_budget(value)?;
        } else if let Some(value) = argument
            .strip_prefix("-m=")
            .or_else(|| argument.strip_prefix("--m="))
        {
            budget = parse_budget(value)?;
        } else if argument.starts_with('-') {
            return Err(USAGE.to_string());
        } else if path.is_some() {
            return Err(USAGE.to_string());
        } else {
            path = Some(argument.clone());
        }
        index += 1;
    }

    Ok((path, budget))
}

fn parse_budget(value: &str) -> Result<u32, String> {
    value
        .parse::<u32>()
        .ok()
        .filter(|megabytes| *megabytes > 0)
        .ok_or_else(|| format!("invalid memory limit: {value}"))
}
