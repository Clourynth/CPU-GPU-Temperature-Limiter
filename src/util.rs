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

//! Shared helpers: paths (with an optional test root), logging, shell-ish sysfs helpers.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

pub const CFG_DIR: &str = "/sdcard/Android/naki/clmt";
pub const CFG_PATH: &str = "/sdcard/Android/naki/clmt/clmt.conf";
pub const LOG_PATH: &str = "/sdcard/Android/naki/clmt/clmt.log";
pub const MODE_PATH: &str = "/sdcard/Android/naki/clmt/clmt_mode";
pub const LEGACY_CFG: &str = "/data/cpu_limiter.conf";
pub const CPUFREQ_DIR: &str = "/sys/devices/system/cpu/cpufreq";

/// Target core temperature in milli-degrees (0 = not configured yet).
pub static TARGET: AtomicU32 = AtomicU32::new(0);
/// Latest measured core temperature in milli-degrees (0 = no reading yet).
pub static CURRENT: AtomicU32 = AtomicU32::new(0);

/// `tuning=smooth`: the clock moves one step per SMOOTH_WINDOW ms toward the newest value.
pub static SMOOTH: AtomicBool = AtomicBool::new(false);
/// `lowlatency=on`: sample/act every FAST_PERIOD instead of SLOW_PERIOD.
pub static LOWLAT: AtomicBool = AtomicBool::new(false);
/// Smooth filter window in ms (config `smoothWindow`).
pub static SMOOTH_WINDOW: AtomicU32 = AtomicU32::new(SMOOTH_WINDOW_DEFAULT);
/// Temperature jump (milli-degrees) that bypasses the smooth filter (config `smoothSpike`).
pub static SMOOTH_SPIKE: AtomicU32 = AtomicU32::new(SMOOTH_SPIKE_DEFAULT);
pub const SMOOTH_WINDOW_DEFAULT: u32 = 310;
pub const SMOOTH_SPIKE_DEFAULT: u32 = 5_000;
pub const SMOOTH_WINDOW_RANGE: (u32, u32) = (10, 5_000);
pub const SMOOTH_SPIKE_RANGE: (u32, u32) = (0, 50_000);

/// Normal period of both the sampler and the limiter (same as the original binary).
pub const SLOW_PERIOD: Duration = Duration::from_micros(33_333);
/// Period used when lowlatency is on.
pub const FAST_PERIOD: Duration = Duration::from_millis(8);

pub fn flag(a: &AtomicBool) -> bool {
    a.load(Ordering::Relaxed)
}

pub fn load(a: &AtomicU32) -> u32 {
    a.load(Ordering::Relaxed)
}

pub fn store(a: &AtomicU32, v: u32) {
    a.store(v, Ordering::Relaxed)
}

/// Every absolute path goes through here so tests can redirect to a fake tree
/// by exporting `CLMT_ROOT`. On a phone the variable is never set.
pub fn p(path: &str) -> PathBuf {
    static ROOT: OnceLock<String> = OnceLock::new();
    let root = ROOT.get_or_init(|| std::env::var("CLMT_ROOT").unwrap_or_default());
    PathBuf::from(format!("{root}{path}"))
}

fn tz_offset_secs() -> i64 {
    static OFF: OnceLock<i64> = OnceLock::new();
    *OFF.get_or_init(|| {
        // The binary is statically linked against musl, which knows nothing about
        // Android's tz database, so ask the system `date` once for "+0800".
        let out = Command::new("date").arg("+%z").output().ok();
        let s = out
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        if s.len() == 5 && (s.starts_with('+') || s.starts_with('-')) {
            let h: i64 = s[1..3].parse().unwrap_or(0);
            let m: i64 = s[3..5].parse().unwrap_or(0);
            let v = h * 3600 + m * 60;
            if s.starts_with('-') {
                -v
            } else {
                v
            }
        } else {
            0
        }
    })
}

pub fn timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let s = (now + tz_offset_secs()).rem_euclid(86_400);
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

/// Append `HH:MM:SS L message` to the log file (same layout as the original).
pub fn log(level: char, msg: &str) {
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o644)
        .open(p(LOG_PATH))
    {
        let _ = writeln!(f, "{} {} {}", timestamp(), level, msg);
    }
}

/// Create the config directory and truncate the log (the original does both on start).
pub fn log_init() {
    let _ = fs::create_dir_all(p(CFG_DIR));
    let _ = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o644)
        .open(p(LOG_PATH));
}

pub fn chmod(path: &std::path::Path, mode: u32) {
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode));
}

pub fn read_trim(path: &std::path::Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

/// Android system property: `getprop`, then the build.prop files (also used on a host with CLMT_ROOT).
pub fn getprop(name: &str) -> Option<String> {
    if let Ok(o) = std::process::Command::new("getprop").arg(name).output() {
        let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }
    let key = format!("{name}=");
    for f in ["/system/build.prop", "/product/etc/build.prop", "/system_ext/etc/build.prop", "/vendor/build.prop", "/odm/etc/build.prop"] {
        if let Ok(t) = std::fs::read_to_string(p(f)) {
            if let Some(v) = t.lines().find_map(|l| l.trim().strip_prefix(key.as_str())) {
                if !v.trim().is_empty() {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}
