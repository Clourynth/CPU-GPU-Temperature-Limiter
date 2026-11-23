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

//! CPU thermal takeover (config `cpuTakeover`, default on): what the original service.sh did once at
//! boot (disable CPU hotplug / userspace thermal that would fight the limiter). Everything that is
//! changed is recorded first in a state file, so `revert()` can put the system back as it was.

use crate::util::*;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{chown, MetadataExt};

const STATE: &str = "/data/adb/cpu_limiter_rs_tweaks.state";

struct Saved {
    path: String,
    value: Option<String>,
    mode: u32,
    uid: u32,
    gid: u32,
}

struct Ctx {
    capture: bool,
    saved: Vec<Saved>,
    thermald: bool,
}

impl Ctx {
    fn save(&mut self, path: &str) {
        if !self.capture || self.saved.iter().any(|s| s.path == path) {
            return;
        }
        let pp = p(path);
        if let Ok(md) = fs::metadata(&pp) {
            let value = read_trim(&pp).filter(|v| !v.is_empty() && !v.contains('\n') && !v.contains('\t'));
            self.saved.push(Saved { path: path.to_string(), value, mode: md.mode() & 0o7777, uid: md.uid(), gid: md.gid() });
        }
    }

    /// echo $val > $path, but leave the node root-owned and read-only afterwards.
    fn lock_val(&mut self, val: &str, path: &str) {
        let pp = p(path);
        if pp.is_file() {
            self.save(path);
            let _ = chown(&pp, Some(0), Some(0));
            chmod(&pp, 0o666);
            write_val(&pp, val);
            chmod(&pp, 0o444);
        }
    }

    /// echo $val > $path, leave permissions open.
    fn mutate(&mut self, val: &str, path: &str) {
        let pp = p(path);
        if pp.is_file() {
            self.save(path);
            chmod(&pp, 0o666);
            write_val(&pp, val);
        }
    }
}

fn write_val(path: &std::path::Path, val: &str) {
    if let Ok(mut f) = OpenOptions::new().write(true).open(path) {
        let _ = f.write_all(val.as_bytes());
    }
}

/// `killall <name>` (SIGTERM) by scanning /proc/*/comm. Returns whether something was signalled.
fn killall(name: &str) -> bool {
    let mut any = false;
    if let Ok(rd) = fs::read_dir(p("/proc")) {
        for e in rd.flatten() {
            if let Ok(pid) = e.file_name().to_string_lossy().parse::<i32>() {
                if read_trim(&e.path().join("comm")).as_deref() == Some(name) {
                    unsafe { libc::kill(pid, libc::SIGTERM) };
                    any = true;
                }
            }
        }
    }
    any
}

fn disable_hotplug(c: &mut Ctx) {
    c.mutate("0", "/sys/power/cpuhotplug/enabled");
    c.mutate("0", "/sys/devices/system/cpu/cpuhotplug/enabled");
    c.lock_val("0", "/sys/module/msm_thermal/core_control/enabled");
    c.lock_val("N", "/sys/module/msm_thermal/parameters/enabled");
    c.lock_val("0", "/sys/kernel/intelli_plug/intelli_plug_active");
    c.lock_val("0", "/sys/module/blu_plug/parameters/enabled");
    c.lock_val("0", "/sys/devices/virtual/misc/mako_hotplug_control/enabled");
    c.lock_val("0", "/sys/module/autosmp/parameters/enabled");
    c.lock_val("0", "/sys/kernel/zen_decision/enabled");
    for i in 0..16 {
        c.lock_val("0", &format!("/sys/devices/system/cpu/cpu{i}/core_ctl/enable"));
    }
    for i in 0..10 {
        c.lock_val("1", &format!("/sys/devices/system/cpu/cpu{i}/online"));
    }
}

fn disable_userspace_thermal(c: &mut Ctx) {
    let has_mi_thermald = p("/vendor/bin/mi_thermald").exists();
    if has_mi_thermald && killall("mi_thermald") {
        c.thermald = true;
    }
    // Make mi_thermald's cpu_limits interface a no-op: pin each cluster's limit to its max.
    let mut cpus: Vec<u32> = vec![0, 2, 4, 6, 7];
    cpus.extend(crate::limiter::discover().iter().map(|c| c.id));
    cpus.sort_unstable();
    cpus.dedup();
    for i in cpus {
        let max = read_trim(&p(&format!("/sys/devices/system/cpu/cpu{i}/cpufreq/cpuinfo_max_freq")))
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        if max > 0 {
            c.lock_val(&format!("cpu{i} {max}"), "/sys/devices/virtual/thermal/thermal_message/cpu_limits");
        }
    }
    if has_mi_thermald && killall("mi_thermald") {
        c.thermald = true;
    }
    // No msm_performance / scaling_max limit left over from the ROM.
    let all = (0..8).map(|i| format!("{i}:9999000")).collect::<Vec<_>>().join(" ");
    c.lock_val(&all, "/sys/module/msm_performance/parameters/cpu_max_freq");
    // scaling_max_freq is owned by the limiter afterwards, so it is not recorded / reverted here.
    let mut none = Ctx { capture: false, saved: Vec::new(), thermald: false };
    for i in 0..8 {
        none.lock_val("9999000", &format!("/sys/devices/system/cpu/cpu{i}/cpufreq/scaling_max_freq"));
    }
}

fn state_exists() -> bool {
    p(STATE).exists()
}

/// Apply the takeover (recording the original state first when nothing is recorded yet).
pub fn apply() {
    let mut c = Ctx { capture: !state_exists(), saved: Vec::new(), thermald: false };
    disable_hotplug(&mut c);
    disable_userspace_thermal(&mut c);
    if c.capture {
        let mut out = String::new();
        for s in &c.saved {
            out.push_str(&format!("N\t{}\t{}\t{:o}\t{}\t{}\n", s.path, s.value.as_deref().unwrap_or("-"), s.mode, s.uid, s.gid));
        }
        if c.thermald {
            out.push_str("T\tmi_thermald\n");
        }
        if out.is_empty() {
            out.push_str("# nothing to restore\n");
        }
        let sp = p(STATE);
        if let Some(dir) = sp.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(&sp, out);
        chmod(&sp, 0o600);
        log('I', &format!("cpu thermal takeover: on ({} nodes recorded{})", c.saved.len(), if c.thermald { ", mi_thermald stopped" } else { "" }));
    }
}

/// Put everything back as recorded, and restart mi_thermald if it was stopped.
pub fn revert() {
    let Ok(text) = fs::read_to_string(p(STATE)) else { return };
    let mut thermald = false;
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        match f.as_slice() {
            ["N", path, value, mode, uid, gid] => {
                let pp = p(path);
                if !pp.exists() {
                    continue;
                }
                chmod(&pp, 0o666);
                if *value != "-" {
                    write_val(&pp, value);
                }
                if let (Ok(m), Ok(u), Ok(g)) = (u32::from_str_radix(mode, 8), uid.parse::<u32>(), gid.parse::<u32>()) {
                    let _ = chown(&pp, Some(u), Some(g));
                    chmod(&pp, m);
                }
            }
            ["T", _] => thermald = true,
            _ => {}
        }
    }
    if thermald {
        let _ = std::process::Command::new("setprop").args(["ctl.start", "mi_thermald"]).status();
    }
    let _ = fs::remove_file(p(STATE));
    log('I', &format!("cpu thermal takeover: off, system state restored{}", if thermald { " (mi_thermald started)" } else { "" }));
}

/// The takeover only exists on Xiaomi / Redmi / POCO devices (mi_thermald and its cpu_limits interface).
pub fn supported() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| {
        ["ro.product.manufacturer", "ro.product.brand", "ro.product.vendor.manufacturer", "ro.product.vendor.brand"]
            .iter()
            .filter_map(|k| getprop(k))
            .any(|v| {
                let v = v.to_ascii_lowercase();
                ["xiaomi", "redmi", "poco"].iter().any(|b| v.contains(b))
            })
    })
}

/// Make the system match the config switch (used by the daemon at start and on every change).
pub fn reconcile(on: bool) {
    let on = on && supported();
    if on {
        if !state_exists() {
            apply();
        }
    } else if state_exists() {
        revert();
    }
}

/// `cpu_limiter boot-tweaks`: the nodes are pristine after a boot, so any old record is stale.
pub fn run() {
    let _ = fs::remove_file(p(STATE));
    let m = fs::read_to_string(p(CFG_PATH)).map(|t| crate::config::parse(&t)).unwrap_or_default();
    if crate::config::parse_cpu_takeover(&m) && supported() {
        apply();
    }
}
