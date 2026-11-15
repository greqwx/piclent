// Copyright (c) 2026 Greg
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use exif::{In, Reader, Tag};
use windows_sys::Win32::Foundation::SYSTEMTIME;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;

use crate::decode;
use crate::win32::file_name;

pub struct Field {
    pub label: String,
    pub value: String,
}

// fields shown first and in this order, the rest follow as the file lists them
const PREFERRED: &[(Tag, &str)] = &[
    (Tag::ImageDescription, "Description"),
    (Tag::Artist, "Author"),
    (Tag::Copyright, "Copyright"),
    (Tag::UserComment, "Comment"),
    (Tag::Make, "Camera make"),
    (Tag::Model, "Camera model"),
    (Tag::LensModel, "Lens"),
    (Tag::DateTimeOriginal, "Taken"),
    (Tag::DateTime, "Date"),
    (Tag::ExposureTime, "Exposure"),
    (Tag::FNumber, "Aperture"),
    (Tag::PhotographicSensitivity, "ISO"),
    (Tag::FocalLength, "Focal length"),
    (Tag::FocalLengthIn35mmFilm, "Focal length 35 mm"),
    (Tag::ExposureProgram, "Program"),
    (Tag::MeteringMode, "Metering"),
    (Tag::Flash, "Flash"),
    (Tag::WhiteBalance, "White balance"),
    (Tag::Orientation, "Orientation"),
    (Tag::Software, "Software"),
    (Tag::GPSLatitude, "Latitude"),
    (Tag::GPSLongitude, "Longitude"),
    (Tag::GPSAltitude, "Altitude"),
];

pub fn read(path: &Path) -> Vec<Field> {
    let mut fields = file_fields(path);
    match exif_fields(path) {
        Some(exif) if !exif.is_empty() => fields.extend(exif),
        _ => fields.push(Field {
            label: "EXIF".to_string(),
            value: "no metadata in this file".to_string(),
        }),
    }
    fields
}

fn file_fields(path: &Path) -> Vec<Field> {
    let mut fields = Vec::new();
    fields.push(Field {
        label: "File".to_string(),
        value: file_name(&path.to_string_lossy()),
    });

    if let Ok(info) = std::fs::metadata(path) {
        fields.push(Field {
            label: "Size".to_string(),
            value: format_size(info.len()),
        });
        if let Ok(modified) = info.modified() {
            fields.push(Field {
                label: "Modified".to_string(),
                value: format_time(modified),
            });
        }
    }

    if let Ok((width, height)) = decode::image_size(path) {
        fields.push(Field {
            label: "Pixels".to_string(),
            value: format!("{width} x {height}"),
        });
    }

    fields
}

fn exif_fields(path: &Path) -> Option<Vec<Field>> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let exif = Reader::new()
        .continue_on_error(true)
        .read_from_container(&mut reader)
        .or_else(|err| err.distill_partial_result(|_| {}))
        .ok()?;

    let mut fields = Vec::new();
    for (tag, label) in PREFERRED {
        if let Some(field) = exif.get_field(*tag, In::PRIMARY) {
            let value = display(&exif, field);
            if !value.is_empty() {
                push(&mut fields, label, value);
            }
        }
    }

    for field in exif.fields() {
        if field.ifd_num != In::PRIMARY
            || PREFERRED.iter().any(|(tag, _)| *tag == field.tag)
            || REDUNDANT.contains(&field.tag)
        {
            continue;
        }
        let value = display(&exif, field);
        if value.is_empty() {
            continue;
        }
        push(&mut fields, &field.tag.to_string(), value);
    }

    Some(fields)
}

// the reference tags only repeat what the latitude and longitude already say
const REDUNDANT: &[Tag] = &[
    Tag::GPSVersionID,
    Tag::GPSLatitudeRef,
    Tag::GPSLongitudeRef,
    Tag::GPSAltitudeRef,
];

fn display(exif: &exif::Exif, field: &exif::Field) -> String {
    let value = field.display_value().with_unit(exif).to_string();
    let value = value.trim();
    // ascii values come out quoted, the quotes only get in the way here
    value
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(value)
        .trim()
        .to_string()
}

fn push(fields: &mut Vec<Field>, label: &str, mut value: String) {
    if value.chars().count() > 200 {
        value = value.chars().take(200).collect::<String>() + "...";
    }
    fields.push(Field {
        label: label.to_string(),
        value,
    });
}

fn format_size(bytes: u64) -> String {
    const KILOBYTE: f64 = 1024.0;
    let bytes = bytes as f64;
    if bytes >= KILOBYTE * KILOBYTE {
        format!("{:.1} MB", bytes / KILOBYTE / KILOBYTE)
    } else if bytes >= KILOBYTE {
        format!("{:.0} KB", bytes / KILOBYTE)
    } else {
        format!("{} bytes", bytes as u64)
    }
}

fn format_time(time: SystemTime) -> String {
    let Ok(since) = time.duration_since(UNIX_EPOCH) else {
        return String::new();
    };
    let seconds = since.as_secs() as i64 + local_offset();
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    let clock = seconds.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        clock / 3600,
        clock % 3600 / 60
    )
}

// rounds to a quarter hour so a clock tick between the two reads cannot skew it
fn local_offset() -> i64 {
    let mut local: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe {
        GetLocalTime(&mut local);
    }
    let local_seconds = days_from_civil(local.wYear as i64, local.wMonth as u32, local.wDay as u32)
        * 86_400
        + (local.wHour as i64 * 3600 + local.wMinute as i64 * 60 + local.wSecond as i64);
    let utc_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0);
    (local_seconds - utc_seconds).div_euclid(900) * 900
}

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = month as i64;
    let day = day as i64;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 { shifted } else { shifted - 146_096 } / 146_097;
    let day_of_era = (shifted - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_part + 2) / 5 + 1) as u32;
    let month = if month_part < 10 {
        month_part + 3
    } else {
        month_part - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}
