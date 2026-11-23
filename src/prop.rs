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

//! Dynamic `description=` line of module.prop: daemon status, active mode and target.
//! The module manager shows this line, so it works as a tiny status display.

use crate::config::{self, DEFAULTS};
use crate::util::*;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

const BASE: &str = "CPU and GPU frequency limiter based on temperature.";

const MODES: [(&str, &str, &str); 4] = [
    ("powersave", "🍃", "Power save"),
    ("balance", "⚖️", "Balanced"),
    ("performance", "🚀", "Performance"),
    ("fast", "⚡", "Fast"),
];

fn temp_label(mc: u32) -> String {
    if mc % 1000 == 0 {
        format!("{}°C", mc / 1000)
    } else {
        format!("{:.1}°C", mc as f64 / 1000.0)
    }
}

fn value(m: &HashMap<String, String>, key: &str) -> u32 {
    config::get_u32(m, key).unwrap_or_else(|| DEFAULTS.iter().find(|(k, _)| *k == key).map(|(_, v)| *v).unwrap_or(0))
}

/// Mode name for the current target, or custom when no mode has that temperature.
fn mode_label(m: &HashMap<String, String>, mode_file: &str) -> String {
    let target = value(m, "targetTemp");
    let hit = MODES
        .iter()
        .find(|(k, _, _)| *k == mode_file && value(m, k) == target)
        .or_else(|| MODES.iter().find(|(k, _, _)| value(m, k) == target));
    match hit {
        Some((_, e, n)) => format!("{e} {n} {}", temp_label(target)),
        None => format!("🎛️ Custom {}", temp_label(target)),
    }
}

pub fn describe(running: bool, pending: bool, cfg_text: &str, mode_file: &str, gpu_ok: bool) -> String {
    let m = config::parse(cfg_text);
    let daemon = if running { "🟢 Daemon running" } else { "🔴 Daemon stopped" };
    let mut s = format!("{daemon} · {}", mode_label(&m, mode_file));
    // No supported GPU: nothing about the GPU is shown.
    if gpu_ok {
        let g = crate::gpu::GpuCfg::from_map(&m);
        if g.enabled {
            s.push_str(&format!(" · 🎮 GPU {}", temp_label(g.target)));
        } else {
            s.push_str(" · 🎮 GPU off");
        }
    }
    if pending {
        s.push_str(" · ⏱️ Restart to apply");
    }
    s.push_str(" — ");
    s.push_str(BASE);
    s
}

/// `version=` of the installed module.prop (falls back to the crate version).
pub fn module_version() -> String {
    default_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|t| t.lines().find_map(|l| l.strip_prefix("version=").map(|v| v.trim().to_string())))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}

/// module.prop next to the binary (override with CLMT_MODPROP for tests).
pub fn default_path() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("CLMT_MODPROP") {
        return Some(PathBuf::from(v));
    }
    Some(std::env::current_exe().ok()?.parent()?.join("module.prop"))
}

/// Replace (or add) the description line, keeping everything else byte for byte.
pub fn with_description(prop: &str, desc: &str) -> String {
    let mut out = String::with_capacity(prop.len() + desc.len() + 16);
    let mut done = false;
    for line in prop.lines() {
        if !done && line.starts_with("description=") {
            out.push_str(&format!("description={desc}\n"));
            done = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !done {
        out.push_str(&format!("description={desc}\n"));
    }
    out
}

/// Write the description into each file; skips missing files and unchanged content.
/// Written in place (not renamed) so the file keeps its SELinux label and inode.
pub fn update(paths: &[PathBuf], running: bool, pending: bool) {
    let cfg = fs::read_to_string(p(CFG_PATH)).unwrap_or_default();
    let mode = read_trim(&p(MODE_PATH)).unwrap_or_default();
    let desc = describe(running, pending, &cfg, &mode, crate::gpu::find().is_some());
    for path in paths {
        let Ok(old) = fs::read_to_string(path) else { continue };
        let new = with_description(&old, &desc);
        if new != old {
            let _ = fs::write(path, new);
        }
    }
}

/// Refresh the default module.prop.
pub fn refresh(running: bool) {
    if let Some(path) = default_path() {
        update(&[path], running, false);
    }
}

/// `cpu_limiter prop [--pending] [file...]`
pub fn run(args: &[String]) -> i32 {
    let pending = args.iter().any(|a| a == "--pending");
    let mut paths: Vec<PathBuf> = args.iter().filter(|a| !a.starts_with("--")).map(PathBuf::from).collect();
    if paths.is_empty() {
        paths.extend(default_path());
    }
    let running = !crate::status::daemon_pids().is_empty();
    update(&paths, running, pending);
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    const CFG: &str = "targetTemp=75000\npowersave=60000\nbalance=75000\nperformance=95000\nfast=85000\n";

    #[test]
    fn describes_modes() {
        let d = describe(true, false, CFG, "balance", true);
        assert!(d.starts_with("🟢 Daemon running · ⚖️ Balanced 75°C · 🎮 GPU off — "), "{d}");
        let d = describe(false, false, &CFG.replace("targetTemp=75000", "targetTemp=95000"), "performance", true);
        assert!(d.starts_with("🔴 Daemon stopped · 🚀 Performance 95°C"), "{d}");
        // mode file stale, but another mode has this temperature -> that mode
        let d = describe(true, false, &CFG.replace("targetTemp=75000", "targetTemp=60000"), "fast", true);
        assert!(d.contains("🍃 Power save 60°C"), "{d}");
    }

    #[test]
    fn custom_when_no_mode_matches() {
        let d = describe(true, false, &CFG.replace("targetTemp=75000", "targetTemp=74500"), "balance", true);
        assert!(d.contains("🎛️ Custom 74.5°C"), "{d}");
        let d = describe(true, false, "", "", true);
        assert!(d.contains("Balanced") || d.contains("Performance") || d.contains("Custom"), "{d}");
    }

    #[test]
    fn no_gpu_part_without_supported_gpu() {
        let d = describe(true, false, CFG, "balance", false);
        assert!(!d.contains("🎮"), "{d}");
        assert!(describe(true, false, &format!("{CFG}gpu=on\n"), "balance", false).find("🎮").is_none());
    }

    #[test]
    fn shows_gpu_target() {
        let d = describe(true, false, &format!("{CFG}gpu=on\ngpuTargetTemp=70000\n"), "balance", true);
        assert!(d.contains("🎮 GPU 70°C"), "{d}");
        assert!(describe(true, false, CFG, "balance", true).contains("🎮 GPU off"));
    }

    #[test]
    fn pending_marker() {
        let d = describe(true, true, CFG, "balance", true);
        assert!(d.contains("⏱️ Restart to apply"), "{d}");
        assert!(!describe(true, false, CFG, "balance", true).contains("⏱️"));
    }

    #[test]
    fn description_replaced_only() {
        let src = "id=x\nname=Y\ndescription=lama\nversion=1\n";
        assert_eq!(with_description(src, "baru"), "id=x\nname=Y\ndescription=baru\nversion=1\n");
        assert_eq!(with_description("id=x\n", "baru"), "id=x\ndescription=baru\n");
    }
}
