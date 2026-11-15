// Copyright (c) 2026 Greg
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::decode::{self, Frame};
use crate::win32::WM_USER_FRAME;

const PREFETCH_RADIUS: isize = 2;

// keeps the folder listing and a byte bounded lru cache of frames
pub struct Gallery {
    state: Arc<Mutex<State>>,
    signal: Arc<(Mutex<bool>, Condvar)>,
}

struct State {
    hwnd: isize,
    paths: Vec<String>,
    index: usize,
    cache: HashMap<String, Arc<Frame>>,
    order: VecDeque<String>,
    bytes: usize,
    budget: usize,
    errors: HashMap<String, String>,
    busy: Option<String>,
}

impl Gallery {
    pub fn new(path: &str, budget: usize) -> Gallery {
        let (paths, index) = folder_listing(path);
        Gallery {
            state: Arc::new(Mutex::new(State {
                hwnd: 0,
                paths,
                index,
                cache: HashMap::new(),
                order: VecDeque::new(),
                bytes: 0,
                budget,
                errors: HashMap::new(),
                busy: None,
            })),
            signal: Arc::new((Mutex::new(false), Condvar::new())),
        }
    }

    // decodes in the background, nearest to the current file first
    pub fn start(&self, hwnd: HWND, max_width: u32, max_height: u32) {
        let anim_cap;
        {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            state.hwnd = hwnd as isize;
            anim_cap = state.budget / 2;
        }

        let state = Arc::clone(&self.state);
        let signal = Arc::clone(&self.signal);
        thread::spawn(move || loop {
            let task = match state.lock() {
                Ok(mut state) => state.next_task(),
                Err(_) => return,
            };

            let Some((path, full)) = task else {
                let (lock, condvar) = &*signal;
                let Ok(mut ready) = lock.lock() else {
                    return;
                };
                while !*ready {
                    match condvar.wait(ready) {
                        Ok(guard) => ready = guard,
                        Err(_) => return,
                    }
                }
                *ready = false;
                continue;
            };

            let loaded = decode::load(Path::new(&path), max_width, max_height, anim_cap, full);

            let Ok(mut state) = state.lock() else {
                return;
            };
            state.busy = None;
            if !state.paths.iter().any(|known| known == &path) {
                continue;
            }
            let current = state.paths[state.index] == path;
            match loaded {
                Ok(frame) => state.store(path, Arc::new(frame)),
                Err(message) => state.fail(path, message),
            }
            let hwnd = state.hwnd;
            drop(state);

            if current {
                unsafe {
                    PostMessageW(hwnd as HWND, WM_USER_FRAME, 0, 0);
                }
            }
        });

        self.notify();
    }

    fn notify(&self) {
        let (lock, condvar) = &*self.signal;
        if let Ok(mut ready) = lock.lock() {
            *ready = true;
        }
        condvar.notify_all();
    }

    // moves through the folder and reports whether the index changed
    pub fn step(&self, delta: isize) -> bool {
        let changed = match self.state.lock() {
            Ok(mut state) => {
                let next = state.index as isize + delta;
                if next < 0 || next >= state.paths.len() as isize {
                    false
                } else {
                    state.index = next as usize;
                    true
                }
            }
            Err(_) => false,
        };
        if changed {
            self.notify();
        }
        changed
    }

    pub fn snapshot(&self) -> (Option<Arc<Frame>>, Option<String>) {
        let Ok(mut state) = self.state.lock() else {
            return (None, None);
        };
        state.snapshot()
    }

    pub fn count(&self) -> usize {
        self.state
            .lock()
            .map(|state| state.paths.len())
            .unwrap_or(0)
    }

    pub fn current_name(&self) -> String {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.current().map(|path| file_name(&path)))
            .unwrap_or_default()
    }

    pub fn current_path(&self) -> String {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.current())
            .unwrap_or_default()
    }

    // points the gallery at the folder of path and drops the old cache
    pub fn reset(&self, path: &str) {
        let (paths, index) = folder_listing(path);
        if let Ok(mut state) = self.state.lock() {
            state.paths = paths;
            state.index = index;
            state.cache.clear();
            state.order.clear();
            state.bytes = 0;
            state.errors.clear();
            state.busy = None;
        }
        self.notify();
    }

    // drops the current entry after its file left the disk
    pub fn forget_current(&self) -> usize {
        let remaining = match self.state.lock() {
            Ok(mut state) => state.forget_current(),
            Err(_) => 0,
        };
        self.notify();
        remaining
    }
}

impl State {
    fn current(&self) -> Option<String> {
        self.paths.get(self.index).cloned()
    }

    fn next_task(&mut self) -> Option<(String, bool)> {
        if self.busy.is_some() {
            return None;
        }

        let mut candidates = Vec::with_capacity(1 + PREFETCH_RADIUS as usize * 2);
        candidates.push(self.index as isize);
        for radius in 1..=PREFETCH_RADIUS {
            candidates.push(self.index as isize + radius);
            candidates.push(self.index as isize - radius);
        }

        for candidate in candidates {
            if candidate < 0 || candidate >= self.paths.len() as isize {
                continue;
            }
            let index = candidate as usize;
            let path = self.paths[index].clone();
            let current = index == self.index;

            if let Some(frame) = self.cache.get(&path) {
                if !current || !frame.partial {
                    continue;
                }
            } else if self.errors.contains_key(&path) {
                continue;
            }

            self.busy = Some(path.clone());
            return Some((path, current));
        }
        None
    }

    fn snapshot(&mut self) -> (Option<Arc<Frame>>, Option<String>) {
        let Some(path) = self.current() else {
            return (None, None);
        };
        if let Some(frame) = self.cache.get(&path) {
            let frame = Arc::clone(frame);
            self.touch(&path);
            return (Some(frame), None);
        }
        (None, self.errors.get(&path).cloned())
    }

    fn store(&mut self, path: String, frame: Arc<Frame>) {
        if let Some(old) = self.cache.remove(&path) {
            self.bytes -= old.bytes();
            self.order.retain(|known| known != &path);
        }
        self.bytes += frame.bytes();
        self.cache.insert(path.clone(), frame);
        self.order.push_back(path);

        let current = self.current().unwrap_or_default();
        while self.bytes > self.budget {
            let Some(victim) = self.order.iter().find(|known| **known != current).cloned() else {
                break;
            };
            if let Some(frame) = self.cache.remove(&victim) {
                self.bytes -= frame.bytes();
            }
            self.order.retain(|known| known != &victim);
        }
    }

    fn fail(&mut self, path: String, message: String) {
        let settled = self
            .cache
            .get(&path)
            .map(|frame| frame.partial)
            .unwrap_or(false);
        if settled {
            // the first frame is still good, only the animation failed
            if let Some(frame) = self.cache.get(&path) {
                let frame = Arc::new(Frame {
                    pixels: Arc::clone(&frame.pixels),
                    width: frame.width,
                    height: frame.height,
                    anim: frame.anim.clone(),
                    partial: false,
                });
                self.cache.insert(path, frame);
            }
        } else {
            self.errors.insert(path, message);
        }
    }

    fn touch(&mut self, path: &str) {
        if let Some(position) = self.order.iter().position(|known| known == path) {
            self.order.remove(position);
            self.order.push_back(path.to_string());
        }
    }

    fn forget_current(&mut self) -> usize {
        let Some(path) = self.current() else {
            return 0;
        };
        self.paths.remove(self.index);
        if let Some(frame) = self.cache.remove(&path) {
            self.bytes -= frame.bytes();
        }
        self.order.retain(|known| known != &path);
        self.errors.remove(&path);
        if self.index >= self.paths.len() {
            self.index = self.paths.len().saturating_sub(1);
        }
        self.paths.len()
    }
}

// builds the sorted folder contents and the index of path
fn folder_listing(path: &str) -> (Vec<String>, usize) {
    let directory = Path::new(path).parent().unwrap_or(Path::new(""));
    let mut paths = list_images(directory);
    if paths.is_empty() {
        paths = vec![path.to_string()];
    }

    let mut index = index_of(&paths, path);
    if index.is_none() {
        paths.push(path.to_string());
        sort_paths(&mut paths);
        index = index_of(&paths, path);
    }
    (paths, index.unwrap_or(0))
}

fn index_of(paths: &[String], path: &str) -> Option<usize> {
    paths
        .iter()
        .position(|known| known.eq_ignore_ascii_case(path))
}

pub fn is_image_file(name: &str) -> bool {
    matches!(
        Path::new(name)
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default()
            .as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp"
    )
}

fn list_images(directory: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut paths: Vec<String> = entries
        .flatten()
        .filter(|entry| {
            entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
        })
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if is_image_file(&name) {
                Some(
                    PathBuf::from(directory)
                        .join(name)
                        .to_string_lossy()
                        .into_owned(),
                )
            } else {
                None
            }
        })
        .collect();
    sort_paths(&mut paths);
    paths
}

fn sort_paths(paths: &mut [String]) {
    paths.sort_by(|left, right| {
        let (left_name, right_name) = (file_name(left), file_name(right));
        natural_less(&left_name, &right_name)
            .cmp(&natural_less(&right_name, &left_name))
            .then_with(|| left.cmp(right))
    });
}

// compares names so that digit runs count as numbers, keeping photo-2 before photo-10
fn natural_less(left: &str, right: &str) -> bool {
    let left = left.to_lowercase();
    let right = right.to_lowercase();
    let left = left.as_bytes();
    let right = right.as_bytes();

    let (mut li, mut ri) = (0, 0);
    while li < left.len() && ri < right.len() {
        if left[li].is_ascii_digit() && right[ri].is_ascii_digit() {
            let left_end = digit_end(left, li);
            let right_end = digit_end(right, ri);
            let left_number = trim_zeros(&left[li..left_end]);
            let right_number = trim_zeros(&right[ri..right_end]);
            if left_number.len() != right_number.len() {
                return left_number.len() < right_number.len();
            }
            if left_number != right_number {
                return left_number < right_number;
            }
            if left_end - li != right_end - ri {
                return left_end - li < right_end - ri;
            }
            li = left_end;
            ri = right_end;
            continue;
        }

        if left[li] != right[ri] {
            return left[li] < right[ri];
        }
        li += 1;
        ri += 1;
    }
    left.len() - li < right.len() - ri
}

fn digit_end(text: &[u8], start: usize) -> usize {
    let mut end = start;
    while end < text.len() && text[end].is_ascii_digit() {
        end += 1;
    }
    end
}

fn trim_zeros(digits: &[u8]) -> &[u8] {
    let start = digits
        .iter()
        .position(|digit| *digit != b'0')
        .unwrap_or(digits.len());
    &digits[start..]
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

// sends the file to the recycle bin
pub fn recycle_file(path: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::{
        SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FO_DELETE,
        SHFILEOPSTRUCTW,
    };

    // shfileoperation needs a fully qualified path
    let absolute = std::path::absolute(path).map_err(|err| err.to_string())?;
    let mut from: Vec<u16> = absolute.to_string_lossy().encode_utf16().collect();
    from.push(0);
    // the path list is terminated by an empty string
    from.push(0);

    let mut operation: SHFILEOPSTRUCTW = unsafe { std::mem::zeroed() };
    operation.wFunc = FO_DELETE;
    operation.pFrom = from.as_ptr();
    operation.fFlags = (FOF_SILENT | FOF_NOCONFIRMATION | FOF_ALLOWUNDO | FOF_NOERRORUI) as u16;

    let result = unsafe { SHFileOperationW(&mut operation) };
    if result != 0 {
        return Err(format!(
            "cannot move {} to the Recycle Bin",
            file_name(path)
        ));
    }
    if operation.fAnyOperationsAborted != 0 {
        return Err(format!("{} was not deleted", file_name(path)));
    }
    Ok(())
}
