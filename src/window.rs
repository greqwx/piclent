// Copyright (c) 2026 Greg
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{BeginPaint, EndPaint, ScreenToClient, PAINTSTRUCT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VK_ADD, VK_C, VK_CONTROL, VK_DELETE, VK_ESCAPE, VK_F11, VK_INSERT, VK_LEFT,
    VK_NEXT, VK_OEM_MINUS, VK_OEM_PLUS, VK_PRIOR, VK_RIGHT, VK_SPACE, VK_SUBTRACT,
};
use windows_sys::Win32::UI::Shell::{DragFinish, DragQueryFileW, HDROP};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, GetCursorPos, GetWindowLongPtrW, LoadCursorW, PostQuitMessage, SetCursor,
    GWL_USERDATA, HTCLIENT, IDC_ARROW, IDC_HAND, IDC_SIZEALL, SIZE_MINIMIZED, WM_CAPTURECHANGED,
    WM_CONTEXTMENU, WM_DESTROY, WM_DROPFILES, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDBLCLK,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_RBUTTONDOWN,
    WM_RBUTTONUP, WM_SETCURSOR, WM_SIZE,
};

use crate::gallery::is_image_file;
use crate::viewer::{Viewer, WHEEL_ZOOM};
use crate::win32::{self, mouse_point, wheel_delta, WM_USER_ANIM, WM_USER_FRAME};

pub unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_USER_FRAME => {
            if let Some(viewer) = viewer(hwnd) {
                viewer.refresh();
            }
            0
        }

        WM_USER_ANIM => {
            if let Some(viewer) = viewer(hwnd) {
                viewer.show_animation_frame(wparam as u64, lparam as usize);
            }
            0
        }

        WM_PAINT => {
            match viewer(hwnd) {
                Some(viewer) => viewer.paint(),
                None => {
                    // no viewer yet, just validate
                    let mut paint: PAINTSTRUCT = std::mem::zeroed();
                    BeginPaint(hwnd, &mut paint);
                    EndPaint(hwnd, &paint);
                }
            }
            0
        }

        WM_SIZE => {
            if let Some(viewer) = viewer(hwnd) {
                viewer.set_suspended(wparam == SIZE_MINIMIZED as usize);
                win32::invalidate(hwnd);
            }
            0
        }

        // wm_paint handles the background, erasing it here only causes flicker
        WM_ERASEBKGND => 1,

        WM_DROPFILES => {
            if let Some(path) = dropped_file(wparam as HDROP) {
                if is_image_file(&path) {
                    if let Some(viewer) = viewer(hwnd) {
                        viewer.open_path(&path);
                    }
                }
            }
            0
        }

        WM_MOUSEWHEEL => {
            if let Some(viewer) = viewer(hwnd) {
                let (x, y) = mouse_point(lparam);
                viewer.zoom_wheel(x, y, wheel_delta(wparam));
            }
            0
        }

        WM_KEYDOWN => {
            if let Some(viewer) = viewer(hwnd) {
                let key = wparam as u16;
                if key == VK_C || key == VK_INSERT {
                    if GetKeyState(VK_CONTROL as i32) < 0 {
                        viewer.copy_current();
                        return 0;
                    }
                }
                if key == VK_LEFT || key == VK_PRIOR {
                    viewer.navigate(-1);
                    return 0;
                }
                if key == VK_RIGHT || key == VK_NEXT {
                    viewer.navigate(1);
                    return 0;
                }
                if key == VK_SPACE {
                    viewer.toggle_pause();
                    return 0;
                }
                if key == VK_F11 {
                    viewer.toggle_fullscreen();
                    return 0;
                }
                if key == VK_ESCAPE {
                    viewer.exit_fullscreen();
                    return 0;
                }
                if key == VK_DELETE {
                    viewer.delete_current();
                    return 0;
                }
                if key == VK_ADD || key == VK_OEM_PLUS {
                    viewer.zoom_step(WHEEL_ZOOM);
                    return 0;
                }
                if key == VK_SUBTRACT || key == VK_OEM_MINUS {
                    viewer.zoom_step(1.0 / WHEEL_ZOOM);
                    return 0;
                }
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }

        WM_SETCURSOR => {
            if (lparam & 0xffff) as u32 == HTCLIENT {
                if let Some(viewer) = viewer(hwnd) {
                    if viewer.is_dragging() {
                        SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_SIZEALL));
                        return 1;
                    }
                    let mut point = POINT { x: 0, y: 0 };
                    GetCursorPos(&mut point);
                    ScreenToClient(hwnd, &mut point);
                    if viewer.button_at(point.x, point.y) != 0 {
                        SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_HAND));
                        return 1;
                    }
                }
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }

        WM_LBUTTONDOWN => {
            if let Some(viewer) = viewer(hwnd) {
                let (x, y) = mouse_point(lparam);
                let direction = viewer.button_at(x, y);
                if direction != 0 {
                    viewer.navigate(direction as isize);
                    return 0;
                }
                if !viewer.is_dragging() {
                    viewer.begin_drag(x, y);
                    SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_SIZEALL));
                }
            }
            0
        }

        WM_LBUTTONDBLCLK => {
            if let Some(viewer) = viewer(hwnd) {
                let (x, y) = mouse_point(lparam);
                if viewer.button_at(x, y) == 0 {
                    viewer.toggle_actual_size();
                }
            }
            0
        }

        WM_RBUTTONDOWN => {
            if let Some(viewer) = viewer(hwnd) {
                if !viewer.is_dragging() {
                    let (x, y) = mouse_point(lparam);
                    viewer.begin_drag(x, y);
                    SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_SIZEALL));
                }
            }
            0
        }

        // the right-click drag above would otherwise end with Windows still queuing its
        // default context menu once the button comes up; swallow it so capture state
        // (and with it the toolbar buttons' hover/cursor feedback) never gets stuck
        WM_CONTEXTMENU => 0,

        WM_MOUSEMOVE => {
            if let Some(viewer) = viewer(hwnd) {
                if viewer.is_dragging() {
                    let (x, y) = mouse_point(lparam);
                    viewer.drag_to(x, y);
                    SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_SIZEALL));
                }
            }
            0
        }

        WM_LBUTTONUP | WM_RBUTTONUP => {
            if let Some(viewer) = viewer(hwnd) {
                viewer.end_drag();
                SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_ARROW));
            }
            0
        }

        WM_CAPTURECHANGED => {
            if let Some(viewer) = viewer(hwnd) {
                viewer.cancel_drag();
            }
            0
        }

        WM_DESTROY => {
            if let Some(viewer) = viewer(hwnd) {
                viewer.shutdown();
            }
            PostQuitMessage(0);
            0
        }

        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn viewer(hwnd: HWND) -> Option<&'static mut Viewer> {
    let pointer = GetWindowLongPtrW(hwnd, GWL_USERDATA);
    if pointer == 0 {
        return None;
    }
    Some(&mut *(pointer as *mut Viewer))
}

// takes the first path out of a wm_dropfiles handle and always finishes the drop
unsafe fn dropped_file(drop: HDROP) -> Option<String> {
    let length = DragQueryFileW(drop, 0, std::ptr::null_mut(), 0);
    let path = if length == 0 {
        None
    } else {
        let mut buffer = vec![0u16; length as usize + 1];
        if DragQueryFileW(drop, 0, buffer.as_mut_ptr(), buffer.len() as u32) == 0 {
            None
        } else {
            Some(String::from_utf16_lossy(&buffer[..length as usize]))
        }
    };
    DragFinish(drop);
    path
}
