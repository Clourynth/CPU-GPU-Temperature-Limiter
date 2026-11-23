// CPU & GPU Temperature Limiter
// Copyright (C) 2026  Clourynth
//
// Credits: nakixii (Magisk_CPULimiter) and ahmed-alnassif.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Thermal sensor discovery + the temperature polling thread (TempMonitor).

use crate::util::*;
use std::fs::{self, File};
use std::os::unix::fs::FileExt;
use std::path::PathBuf;

/// (zone dir name, `type`) of every thermal zone, sorted by name.
pub fn zone_list() -> Vec<(String, String)> {
    let dir = p("/sys/class/thermal");
    let mut zones: Vec<(String, String)> = Vec::new();
    if let Ok(rd) = fs::read_dir(&dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.contains("thermal_zone") {
                continue;
            }
            if let Some(t) = read_trim(&dir.join(&name).join("type")) {
                zones.push((name, t));
            }
        }
    }
    zones.sort();
    zones
}

/// Same selection rules as the original binary:
///  * a zone whose `type` contains `soc_max` or `mtktscpu` is used on its own;
///  * otherwise every zone whose `type` contains `cpu-1-` is used (max of them).
/// `filter` (config key `sensor=`) overrides this: every zone whose type contains it.
pub fn discover(filter: Option<&str>) -> Vec<PathBuf> {
    let dir = p("/sys/class/thermal");
    let zones = zone_list();
    let temp = |name: &str| dir.join(name).join("temp");

    if let Some(f) = filter.filter(|f| !f.is_empty()) {
        return zones.iter().filter(|(_, t)| t.contains(f)).map(|(n, _)| temp(n)).collect();
    }
    if let Some((n, _)) = zones.iter().find(|(_, t)| t.contains("soc_max") || t.contains("mtktscpu")) {
        return vec![temp(n)];
    }
    zones.iter().filter(|(_, t)| t.contains("cpu-1-")).map(|(n, _)| temp(n)).collect()
}

pub fn read_temp(f: &File) -> Option<u32> {
    let mut buf = [0u8; 16];
    let n = f.read_at(&mut buf, 0).ok()?;
    let v: i64 = std::str::from_utf8(&buf[..n]).ok()?.trim().parse().ok()?;
    (v > 0 && v < 200_000).then_some(v as u32)
}

/// The CPU temperature sensors, kept open. Read from the limiter loop (no thread of its own).
pub struct Reader {
    paths: Vec<PathBuf>,
    files: Vec<Option<File>>,
}

impl Reader {
    pub fn new(paths: Vec<PathBuf>) -> Self {
        let files = paths.iter().map(|_| None).collect();
        Reader { paths, files }
    }

    /// Maximum reading of all sensors (raw, no filtering: smoothing is done on the clock by the
    /// limiter), also published into `CURRENT`. None when no sensor could be read.
    pub fn read(&mut self) -> Option<u32> {
        let mut max = 0u32;
        for (path, slot) in self.paths.iter().zip(self.files.iter_mut()) {
            if slot.is_none() {
                *slot = File::open(path).ok();
            }
            if let Some(f) = slot {
                match read_temp(f) {
                    Some(v) => max = max.max(v),
                    None => *slot = None, // reopen next round
                }
            }
        }
        if max > 0 {
            store(&CURRENT, max);
            Some(max)
        } else {
            None
        }
    }
}
