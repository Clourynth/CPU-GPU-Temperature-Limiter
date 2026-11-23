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

//! `cpu_limiter dump-gpu`: one read-only text snapshot of the GPU / thermal interfaces of this device,
//! so support for a GPU that was never seen can be written from real data instead of guesses.
//!
//! Only a fixed list of GPU / thermal files is read (no serial numbers, IDs, accounts or networks),
//! nothing is written to the system, and the result is a single text file the user can read before
//! sharing it. Running it again replaces the file.

use crate::util::*;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

pub const OUT: &str = "/sdcard/Android/naki/clmt/gpu-dump.txt";
const MAX_READ: usize = 4096;

const PROPS: &[&str] = &[
    "ro.product.manufacturer",
    "ro.product.brand",
    "ro.product.model",
    "ro.product.device",
    "ro.product.vendor.manufacturer",
    "ro.product.vendor.brand",
    "ro.soc.manufacturer",
    "ro.soc.model",
    "ro.board.platform",
    "ro.hardware",
    "ro.build.version.release",
    "ro.build.version.sdk",
    "ro.build.display.id",
    "ro.mi.os.version.name",
    "ro.miui.ui.version.name",
];

const DEVFREQ_FILES: &[&str] = &[
    "available_frequencies", "min_freq", "max_freq", "cur_freq", "target_freq", "governor",
    "available_governors", "polling_interval", "device/of_node/name", "device/of_node/compatible",
];

const KGSL_FILES: &[&str] = &[
    "freq_table_mhz", "gpu_available_frequencies", "min_clock_mhz", "max_clock_mhz", "gpuclk", "gpu_model",
    "gpubusy", "min_pwrlevel", "max_pwrlevel", "default_pwrlevel", "thermal_pwrlevel", "num_pwrlevels", "temp",
    "devfreq/available_frequencies", "devfreq/min_freq", "devfreq/max_freq", "devfreq/cur_freq", "devfreq/governor",
];

/// Content of a small sysfs/procfs file (NULs shown as spaces), or why it could not be read.
fn val(path: &Path) -> String {
    match fs::read(path) {
        Ok(b) => String::from_utf8_lossy(&b[..b.len().min(MAX_READ)]).replace('\0', " ").trim().to_string(),
        Err(e) => format!("(unreadable: {})", e.kind()),
    }
}

/// Sorted entry names of a directory, or None if it is not there.
fn names(dir: &Path) -> Option<Vec<String>> {
    let mut v: Vec<String> = fs::read_dir(dir).ok()?.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
    v.sort();
    Some(v)
}

fn section(o: &mut String, title: &str) {
    let _ = writeln!(o, "\n=== {title} ===");
}

fn files(o: &mut String, dir: &Path, list: &[&str]) {
    for f in list {
        let path = dir.join(f);
        if path.exists() {
            let _ = writeln!(o, "  {f}: {}", val(&path));
        }
    }
}

/// Output of one of this binary's own read-only subcommands.
fn own(args: &[&str]) -> String {
    if cfg!(test) {
        return "(skipped in unit tests)".into(); // current_exe() is the test harness there
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| std::process::Command::new(exe).args(args).output().ok())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "(not available)".into())
}

fn build() -> String {
    let mut o = String::new();
    let _ = writeln!(o, "cpu_limiter gpu-dump (read-only snapshot, nothing in it identifies the owner)");
    let _ = writeln!(o, "module: {} (binary {})", crate::prop::module_version(), env!("CARGO_PKG_VERSION"));
    let _ = writeln!(o, "taken: {}", timestamp());

    section(&mut o, "device");
    for k in PROPS {
        if let Some(v) = getprop(k) {
            let _ = writeln!(o, "{k}: {v}");
        }
    }
    let _ = writeln!(o, "kernel: {}", val(&p("/proc/version")));
    let _ = writeln!(o, "device-tree compatible: {}", val(&p("/proc/device-tree/compatible")));

    section(&mut o, "what the module detected (info)");
    o.push_str(&own(&["info"]));
    o.push('\n');
    section(&mut o, "module status");
    o.push_str(&own(&["status"]));
    o.push('\n');

    section(&mut o, "devfreq (/sys/class/devfreq)");
    match names(&p("/sys/class/devfreq")) {
        Some(list) => {
            for n in list {
                let d = p("/sys/class/devfreq").join(&n);
                let _ = writeln!(o, "[{n}]");
                if let Some(f) = names(&d) {
                    let _ = writeln!(o, "  files: {}", f.join(" "));
                }
                files(&mut o, &d, DEVFREQ_FILES);
            }
        }
        None => o.push_str("(not present)\n"),
    }

    section(&mut o, "kgsl (/sys/class/kgsl)");
    match names(&p("/sys/class/kgsl")) {
        Some(list) => {
            for n in list {
                let d = p("/sys/class/kgsl").join(&n);
                let _ = writeln!(o, "[{n}]");
                if let Some(f) = names(&d) {
                    let _ = writeln!(o, "  files: {}", f.join(" "));
                }
                files(&mut o, &d, KGSL_FILES);
            }
        }
        None => o.push_str("(not present)\n"),
    }

    section(&mut o, "other GPU nodes (names only)");
    for dir in ["/proc/gpufreqv2", "/proc/gpufreq", "/sys/class/misc"] {
        if let Some(list) = names(&p(dir)) {
            let list: Vec<String> = if dir == "/sys/class/misc" { list.into_iter().filter(|n| n.contains("mali") || n.contains("gpu") || n.contains("kgsl")).collect() } else { list };
            let _ = writeln!(o, "{dir}: {}", list.join(" "));
        }
    }
    for dir in ["/dev", "/sys/devices/platform"] {
        if let Some(list) = names(&p(dir)) {
            let list: Vec<String> = list.into_iter().filter(|n| { let n = n.to_ascii_lowercase(); n.contains("mali") || n.contains("kgsl") || n.contains("adreno") || n.contains("gpu") }).collect();
            let _ = writeln!(o, "{dir} (gpu related): {}", list.join(" "));
        }
    }

    section(&mut o, "thermal zones");
    let tz = p("/sys/class/thermal");
    for n in names(&tz).unwrap_or_default() {
        if n.starts_with("thermal_zone") {
            let _ = writeln!(o, "{n}: type={} temp={}", val(&tz.join(&n).join("type")), val(&tz.join(&n).join("temp")));
        }
    }
    section(&mut o, "cooling devices");
    for n in names(&tz).unwrap_or_default() {
        if n.starts_with("cooling_device") {
            let d: PathBuf = tz.join(&n);
            let _ = writeln!(o, "{n}: type={} cur_state={} max_state={}", val(&d.join("type")), val(&d.join("cur_state")), val(&d.join("max_state")));
        }
    }

    section(&mut o, "module config");
    o.push_str(&val(&p(CFG_PATH)));
    o.push('\n');
    section(&mut o, "module log (last lines)");
    let log = fs::read_to_string(p(LOG_PATH)).unwrap_or_default();
    let lines: Vec<&str> = log.lines().collect();
    for l in &lines[lines.len().saturating_sub(40)..] {
        o.push_str(l);
        o.push('\n');
    }
    o
}

/// `cpu_limiter dump-gpu`: prints the path of the file on success.
pub fn run() -> i32 {
    let out = p(OUT);
    if let Some(dir) = out.parent() {
        let _ = fs::create_dir_all(dir);
    }
    match fs::write(&out, build()) {
        Ok(_) => {
            println!("{OUT}");
            0
        }
        Err(e) => {
            eprintln!("cannot write {OUT}: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_reported_not_fatal() {
        assert!(val(Path::new("/definitely/not/here")).starts_with("(unreadable"));
        assert_eq!(names(Path::new("/definitely/not/here")), None);
    }

    #[test]
    fn dump_has_every_section() {
        let d = build();
        for s in ["=== device ===", "=== devfreq", "=== kgsl", "=== thermal zones ===", "=== cooling devices ===", "=== module config ==="] {
            assert!(d.contains(s), "{s}");
        }
    }
}
