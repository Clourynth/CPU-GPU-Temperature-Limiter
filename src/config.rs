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

//! clmt.conf parsing / rendering and the `mode` + `init-config` sub-commands.

use crate::gpu::GpuCfg;
use crate::util::*;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;

pub const DEFAULTS: [(&str, u32); 5] = [
    ("targetTemp", 65000),
    ("powersave", 55000),
    ("balance", 65000),
    ("performance", 75000),
    ("fast", 95000),
];

pub fn parse(text: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = l.split_once('=') {
            m.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    m
}

pub fn get_u32(m: &HashMap<String, String>, key: &str) -> Option<u32> {
    m.get(key).and_then(|v| v.parse::<u32>().ok())
}

/// `tuning=` (or legacy `filter=`): Some(true)=smooth, Some(false)=raw, None=unset/invalid.
pub fn parse_smooth(m: &HashMap<String, String>) -> Option<bool> {
    let v = m.get("tuning").or_else(|| m.get("filter"))?;
    match v.to_ascii_lowercase().as_str() {
        "smooth" | "on" | "1" | "true" => Some(true),
        "raw" | "aggressive" | "off" | "0" | "false" => Some(false),
        _ => None,
    }
}

/// `smoothWindow` (ms) and `smoothSpike` (milli-degrees), defaults when missing/out of range.
pub fn parse_smooth_params(m: &HashMap<String, String>) -> (u32, u32) {
    let w = get_u32(m, "smoothWindow").filter(|v| (SMOOTH_WINDOW_RANGE.0..=SMOOTH_WINDOW_RANGE.1).contains(v)).unwrap_or(SMOOTH_WINDOW_DEFAULT);
    let sp = get_u32(m, "smoothSpike").filter(|v| (SMOOTH_SPIKE_RANGE.0..=SMOOTH_SPIKE_RANGE.1).contains(v)).unwrap_or(SMOOTH_SPIKE_DEFAULT);
    (w, sp)
}

/// `cpuTakeover=on|off` (default on, like the original module).
pub fn parse_cpu_takeover(m: &HashMap<String, String>) -> bool {
    m.get("cpuTakeover").and_then(|v| parse_onoff("cpuTakeover", v).ok()).unwrap_or(true)
}

/// `lowlatency=on|off`.
pub fn parse_lowlat(m: &HashMap<String, String>) -> Option<bool> {
    match m.get("lowlatency")?.to_ascii_lowercase().as_str() {
        "on" | "1" | "true" => Some(true),
        "off" | "0" | "false" => Some(false),
        _ => None,
    }
}

/// Replace the `targetTemp=` line (or append one), keeping everything else intact.
pub fn with_target(text: &str, value: u32) -> String {
    let mut out = String::new();
    let mut done = false;
    for line in text.lines() {
        if !done && line.trim_start().starts_with("targetTemp=") {
            out.push_str(&format!("targetTemp={value}\n"));
            done = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !done {
        out.push_str(&format!("targetTemp={value}\n"));
    }
    out
}

const NOTE: &str = "\
# The system's built-in thermal limits make the processor less efficient.
# The temperature limit set by this module, at a slightly lower core
# temperature and the same performance, uses less power than the default.
# If you only want maximum performance, keep the default values.
# This module is not compatible with third-party schedulers such as Uperf.

# WARNING! This module does NOT change the system's thermal wall!
# The values you set must not be higher than the system thermal limit,
# otherwise they have no effect! Some devices (Xiaomi) have different limits,
# test the values yourself to get maximum performance.

# targetTemp = target core temperature (changed automatically when switching mode, e.g. via Scene)
# powersave / balance / performance / fast = temperature limit of each mode
# Values must be multiplied by 1000, e.g. 95 degrees C = 95000

# (optional) sensor = text fragment of the thermal zone 'type' file used as
# the temperature sensor. Leave empty / remove for automatic detection
# (soc_max, mtktscpu, or all cpu-1-* zones). 'auto' also means automatic.

# tuning = raw | smooth   (default: raw)
#   raw    : the clock follows the temperature directly (like the original module)
#            (aliases: aggressive, off)
#   smooth : EXPERIMENTAL. Polling and writing are the same as raw, but once the clock
#            changes, it moves one step toward the newest value and is held for
#            smoothWindow ms before the next step, so the frequency does not jitter. A sudden temperature jump
#            (smoothSpike or more) cancels the hold for a moment.
#            (aliases: on, old name: filter=on/off)
# smoothWindow = hold time in ms, 10..5000 (default 310)
# smoothSpike  = temperature jump that bypasses the filter, in milli-degrees,
#                0..50000 (default 5000 = 5 C). 0 = always bypass (same as raw).
# cpuTakeover = on | off   (default: on)
#   on : the system's userspace thermal (mi_thermald), CPU hotplug and core control are
#        turned off so only this module limits the CPU (what the original module did at boot).
#   off: everything is restored to the original state and mi_thermald is started again.
# lowlatency = off | on   (default: off)
#   on : temperature is read and frequency updated every ~8 ms (instead of 33 ms).
#        Much more responsive, also when the frequency rises again after cooling.
#        The cost is that the CPU wakes up ~125x/second even when idle (slightly
#        more power).
# Changes to tuning/smooth*/lowlatency apply automatically within ~1 second, no reboot needed.
";

pub fn render(values: &HashMap<&str, u32>, sensor: Option<&str>, smooth: bool, sp: (u32, u32), lowlat: bool, cpu_take: bool, gpu: &GpuCfg) -> String {
    let mut s = String::from(NOTE);
    s.push('\n');
    for (k, _) in DEFAULTS {
        s.push_str(&format!("{k}={}\n", values[k]));
    }
    s.push_str(&format!("tuning={}\n", if smooth { "smooth" } else { "raw" }));
    s.push_str(&format!("cpuTakeover={}\n", onoff(cpu_take)));
    s.push_str(&format!("smoothWindow={}\n", sp.0));
    s.push_str(&format!("smoothSpike={}\n", sp.1));
    s.push_str(&format!("lowlatency={}\n", if lowlat { "on" } else { "off" }));
    let sn = sensor.filter(|v| !v.is_empty()).unwrap_or("auto");
    s.push_str(&format!("sensor={sn}\n"));
    s.push_str(GPU_NOTE);
    s.push_str(&format!("gpu={}\n", onoff(gpu.enabled)));
    s.push_str(&format!("gpuTakeover={}\n", onoff(gpu.takeover)));
    s.push_str(&format!("gpuTargetTemp={}\n", gpu.target));
    s.push_str(&format!("gpuMaxFreq={}\n", gpu.max_freq));
    s.push_str(&format!("gpuSensor={}\n", gpu.sensor));
    s.push_str(&format!("gpuSafeMax={}\n", gpu.safe_max));
    s
}

fn onoff(b: bool) -> &'static str {
    if b {
        "on"
    } else {
        "off"
    }
}

const GPU_NOTE: &str = "
# --- GPU ---
# gpu = off | on   (default: off)
#   on : the daemon limits the maximum GPU frequency (devfreq max_freq)
# gpuMaxFreq = maximum GPU frequency limit in Hz, e.g. 601000000. 0 = no limit.
#   This is a fixed ceiling, applied even when the temperature is low. If it is not
#   in the OPP table, the nearest frequency below it is used.
# gpuTargetTemp = target GPU temperature (x1000). Above it the frequency is lowered
#   one OPP step from the ceiling, below it it rises again up to the ceiling.
# gpuSensor = auto | comma-separated list of thermal zone types for the GPU temperature,
#   separate from the CPU sensor. auto = all zones of type gpu*. The highest value is used.
# gpuSafeMax = the system's default maximum GPU frequency (Hz) detected at install/boot,
#   used by the WebUI to mark the danger zone. 0 = not detected.
#   Re-detect: cpu_limiter detect-gpu --force
# gpuTakeover = off | on   (default: off)
#   on : the system's GPU cooling device is forced to state 0 every ~1 second (only
#        applies while gpu=on), so GPU thermal limiting is handled by this module.
#   off: the system's own GPU thermal limiting keeps running.
";

/// `cpu_limiter init-config`: create/migrate the config, like the original customize.sh.
pub fn init_config() -> std::io::Result<()> {
    let _ = fs::create_dir_all(p(CFG_DIR));
    let cur = fs::read_to_string(p(CFG_PATH)).map(|t| parse(&t)).unwrap_or_default();
    let legacy = fs::read_to_string(p(LEGACY_CFG)).map(|t| parse(&t)).unwrap_or_default();
    // Like the original: if the new config has no targetTemp, fall back to the legacy file.
    let src = if get_u32(&cur, "targetTemp").is_some() { &cur } else { &legacy };

    let mut values: HashMap<&str, u32> = HashMap::new();
    for (k, d) in DEFAULTS {
        values.insert(k, get_u32(src, k).or_else(|| get_u32(&cur, k)).unwrap_or(d));
    }
    let sensor = cur.get("sensor").or_else(|| legacy.get("sensor")).cloned();
    let smooth = parse_smooth(&cur).or_else(|| parse_smooth(&legacy)).unwrap_or(false);
    let lowlat = parse_lowlat(&cur).or_else(|| parse_lowlat(&legacy)).unwrap_or(false);
    let gpu = GpuCfg::from_map(&cur);
    fs::write(p(CFG_PATH), render(&values, sensor.as_deref(), smooth, parse_smooth_params(&cur), lowlat, parse_cpu_takeover(&cur), &gpu))?;
    let _ = fs::remove_file(p(LEGACY_CFG));
    Ok(())
}

/// `cpu_limiter mode <name>`: replacement for powercfg_main.sh (called by Scene).
pub fn set_mode(mode: &str) -> i32 {
    let key = match mode {
        "powersave" => "powersave",
        "balance" | "init" => "balance",
        "performance" | "pedestal" => "performance",
        "fast" => "fast",
        _ => {
            println!("Failed to apply unknown action '{mode}'.");
            return 1;
        }
    };
    let text = fs::read_to_string(p(CFG_PATH)).unwrap_or_default();
    let m = parse(&text);
    let default = DEFAULTS.iter().find(|(k, _)| *k == key).map(|(_, v)| *v).unwrap_or(95000);
    let value = get_u32(&m, key).unwrap_or(default);
    if let Err(e) = fs::write(p(CFG_PATH), with_target(&text, value)) {
        println!("Failed to write config: {e}");
        return 1;
    }
    let _ = fs::write(p(MODE_PATH), format!("{mode}\n"));
    let _ = fs::set_permissions(p(MODE_PATH), fs::Permissions::from_mode(0o666));
    0
}

/// Allowed range for temperature settings (milli-degrees): 30 C .. 120 C.
pub const TEMP_MIN: u32 = 30_000;
pub const TEMP_MAX: u32 = 120_000;

/// Everything `set` can change, already validated.
#[derive(Debug, Default, PartialEq)]
pub struct Update {
    pub temps: Vec<(&'static str, u32)>,
    pub smooth: Option<bool>,
    pub lowlat: Option<bool>,
    pub cpu_takeover: Option<bool>,
    pub smooth_window: Option<u32>,
    pub smooth_spike: Option<u32>,
    pub sensor: Option<String>, // Some("") = remove
    pub mode: Option<&'static str>,
    pub gpu: Option<bool>,
    pub gpu_takeover: Option<bool>,
    pub gpu_target: Option<u32>,
    pub gpu_max: Option<u32>,
    pub gpu_sensor: Option<String>,
    pub gpu_safe: Option<u32>,
}

fn parse_onoff(k: &str, v: &str) -> Result<bool, String> {
    match v.to_ascii_lowercase().as_str() {
        "on" | "1" | "true" => Ok(true),
        "off" | "0" | "false" => Ok(false),
        _ => Err(format!("{k}: '{v}' must be on or off")),
    }
}

fn temp_key(k: &str) -> Option<&'static str> {
    ["targetTemp", "powersave", "balance", "performance", "fast"].into_iter().find(|x| *x == k)
}

/// Parse `key=value` arguments. Nothing is applied if any argument is invalid.
pub fn parse_update(args: &[String]) -> Result<Update, String> {
    let mut u = Update::default();
    if args.is_empty() {
        return Err("no settings given (example: set tuning=raw lowlatency=off balance=75000)".into());
    }
    for a in args {
        let (k, v) = a.split_once('=').ok_or_else(|| format!("'{a}' is not key=value"))?;
        let (k, v) = (k.trim(), v.trim());
        if let Some(key) = temp_key(k) {
            let n: u32 = v.parse().map_err(|_| format!("{k}: '{v}' is not a number (milli-degrees, e.g. 85000)"))?;
            if !(TEMP_MIN..=TEMP_MAX).contains(&n) {
                return Err(format!("{k}: {n} is outside {TEMP_MIN}..{TEMP_MAX} (30-120 C, value x1000)"));
            }
            u.temps.retain(|(x, _)| *x != key);
            u.temps.push((key, n));
            continue;
        }
        match k {
            "tuning" => {
                u.smooth = Some(parse_smooth(&parse(&format!("tuning={v}"))).ok_or_else(|| format!("tuning: '{v}' must be raw or smooth"))?)
            }
            "lowlatency" => {
                u.lowlat = Some(parse_lowlat(&parse(&format!("lowlatency={v}"))).ok_or_else(|| format!("lowlatency: '{v}' must be on or off"))?)
            }
            "smoothWindow" | "smoothSpike" => {
                let (lo, hi) = if k == "smoothWindow" { SMOOTH_WINDOW_RANGE } else { SMOOTH_SPIKE_RANGE };
                let n: u32 = v.parse().map_err(|_| format!("{k}: '{v}' is not a number"))?;
                if !(lo..=hi).contains(&n) {
                    return Err(format!("{k}: {n} is outside {lo}..{hi}"));
                }
                if k == "smoothWindow" { u.smooth_window = Some(n) } else { u.smooth_spike = Some(n) }
            }
            "sensor" => {
                if v.len() > 48 || !v.chars().all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c)) {
                    return Err("sensor: only letters, digits, _ . - (max 48) are allowed".into());
                }
                u.sensor = Some(v.to_string());
            }
            "gpu" => u.gpu = Some(parse_onoff(k, v)?),
            "cpuTakeover" => u.cpu_takeover = Some(parse_onoff(k, v)?),
            "gpuTakeover" => u.gpu_takeover = Some(parse_onoff(k, v)?),
            "gpuTargetTemp" => {
                let n: u32 = v.parse().map_err(|_| format!("{k}: '{v}' is not a number (milli-degrees)"))?;
                if !(TEMP_MIN..=TEMP_MAX).contains(&n) {
                    return Err(format!("{k}: {n} is outside {TEMP_MIN}..{TEMP_MAX}"));
                }
                u.gpu_target = Some(n);
            }
            "gpuMaxFreq" => {
                let n: u32 = v.parse().map_err(|_| format!("{k}: '{v}' is not a number (Hz, 0 = no limit)"))?;
                if n != 0 && !(50_000_000..=3_000_000_000).contains(&n) {
                    return Err(format!("{k}: {n} Hz is not plausible (0 or 50000000..3000000000)"));
                }
                u.gpu_max = Some(n);
            }
            "gpuSafeMax" => {
                let n: u32 = v.parse().map_err(|_| format!("{k}: '{v}' is not a number (Hz, 0 = unknown)"))?;
                if n != 0 && !(50_000_000..=3_000_000_000).contains(&n) {
                    return Err(format!("{k}: {n} Hz is not plausible"));
                }
                u.gpu_safe = Some(n);
            }
            "gpuSensor" => {
                if v.is_empty() || v.len() > 96 || !v.chars().all(|c| c.is_ascii_alphanumeric() || "_.-,".contains(c)) {
                    return Err("gpuSensor: comma separated zone types, only letters, digits, _ . - (max 96)".into());
                }
                u.gpu_sensor = Some(v.to_string());
            }
            "mode" => {
                u.mode = Some(match v {
                    "powersave" => "powersave",
                    "balance" | "init" => "balance",
                    "performance" | "pedestal" => "performance",
                    "fast" => "fast",
                    _ => return Err(format!("mode: unknown mode '{v}'")),
                })
            }
            _ => return Err(format!("unknown setting '{k}'")),
        }
    }
    Ok(u)
}

/// Apply `u` on top of the current config text and return the new file contents.
/// If the active mode's temperature is edited while `targetTemp` still equals it,
/// `targetTemp` follows (unless it was given explicitly).
pub fn apply_update(text: &str, u: &Update) -> String {
    let m = parse(text);
    let mut values: HashMap<&str, u32> = HashMap::new();
    for (k, d) in DEFAULTS {
        values.insert(k, get_u32(&m, k).unwrap_or(d));
    }
    let old = values.clone();
    for (k, v) in &u.temps {
        values.insert(k, *v);
    }
    let explicit_target = u.temps.iter().any(|(k, _)| *k == "targetTemp");
    if !explicit_target {
        for (k, v) in &u.temps {
            if *k != "targetTemp" && old[k] == old["targetTemp"] {
                values.insert("targetTemp", *v);
            }
        }
    }
    if let Some(mode) = u.mode {
        if !explicit_target {
            values.insert("targetTemp", values[mode]);
        }
    }
    let smooth = u.smooth.or_else(|| parse_smooth(&m)).unwrap_or(false);
    let lowlat = u.lowlat.or_else(|| parse_lowlat(&m)).unwrap_or(false);
    let cpu_take = u.cpu_takeover.unwrap_or_else(|| parse_cpu_takeover(&m));
    let mut sp = parse_smooth_params(&m);
    if let Some(v) = u.smooth_window { sp.0 = v; }
    if let Some(v) = u.smooth_spike { sp.1 = v; }
    let sensor = match &u.sensor {
        Some(s) => Some(s.clone()),
        None => m.get("sensor").cloned(),
    };
    let mut gpu = GpuCfg::from_map(&m);
    if let Some(v) = u.gpu {
        gpu.enabled = v;
    }
    if let Some(v) = u.gpu_takeover {
        gpu.takeover = v;
    }
    if let Some(v) = u.gpu_target {
        gpu.target = v;
    }
    if let Some(v) = u.gpu_max {
        gpu.max_freq = v;
    }
    if let Some(v) = &u.gpu_sensor {
        gpu.sensor = v.clone();
    }
    if let Some(v) = u.gpu_safe {
        gpu.safe_max = v;
    }
    render(&values, sensor.as_deref(), smooth, sp, lowlat, cpu_take, &gpu)
}

/// `cpu_limiter set key=value ...`: validate, then replace the config atomically.
pub fn set_values(args: &[String]) -> i32 {
    let rc = set_values_quiet(args);
    if rc == 0 {
        println!("ok");
    }
    rc
}

pub fn set_values_quiet(args: &[String]) -> i32 {
    let u = match parse_update(args) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let _ = fs::create_dir_all(p(CFG_DIR));
    let text = fs::read_to_string(p(CFG_PATH)).unwrap_or_default();
    let new = apply_update(&text, &u);
    let tmp = p(&format!("{CFG_PATH}.tmp"));
    let ok = fs::write(&tmp, &new).and_then(|_| fs::rename(&tmp, p(CFG_PATH))).or_else(|_| fs::write(p(CFG_PATH), &new));
    if let Err(e) = ok {
        let _ = fs::remove_file(&tmp);
        eprintln!("error: cannot write config: {e}");
        return 1;
    }
    if let Some(mode) = u.mode {
        let _ = fs::write(p(MODE_PATH), format!("{mode}\n"));
        let _ = fs::set_permissions(p(MODE_PATH), fs::Permissions::from_mode(0o666));
    }
    0
}

/// Re-reads the config once per second (called from the limiter loop) and publishes
/// targetTemp / tuning / lowlatency / cpuTakeover.
pub struct Watcher {
    last: Option<u32>,
    last_smooth: Option<bool>,
    last_lowlat: Option<bool>,
    last_take: Option<bool>,
    last_label: Option<(u32, String)>,
    next: std::time::Instant,
}

impl Watcher {
    pub fn new() -> Self {
        Watcher { last: None, last_smooth: None, last_lowlat: None, last_take: None, last_label: None, next: std::time::Instant::now() }
    }

    pub fn tick(&mut self) {
        let now = std::time::Instant::now();
        if now < self.next {
            return;
        }
        self.next = now + std::time::Duration::from_secs(1);
    if let Ok(text) = fs::read_to_string(p(CFG_PATH)) {
        let m = parse(&text);
        if let Some(t) = get_u32(&m, "targetTemp") {
            if self.last != Some(t) {
                self.last = Some(t);
                store(&TARGET, t);
                log('I', &format!("targetTemp: {t}"));
            }
        }
        let smooth = parse_smooth(&m).unwrap_or(false);
        if self.last_smooth != Some(smooth) {
            self.last_smooth = Some(smooth);
            SMOOTH.store(smooth, std::sync::atomic::Ordering::Relaxed);
            log('I', &format!("tuning: {}", if smooth { "smooth" } else { "raw" }));
        }
        let (sw, ss) = parse_smooth_params(&m);
        if load(&SMOOTH_WINDOW) != sw || load(&SMOOTH_SPIKE) != ss {
            store(&SMOOTH_WINDOW, sw);
            store(&SMOOTH_SPIKE, ss);
            log('I', &format!("smooth: window {sw} ms, spike {ss} mC"));
        }
        let lowlat = parse_lowlat(&m).unwrap_or(false);
        if self.last_lowlat != Some(lowlat) {
            self.last_lowlat = Some(lowlat);
            LOWLAT.store(lowlat, std::sync::atomic::Ordering::Relaxed);
            log('I', &format!("lowlatency: {}", if lowlat { "on" } else { "off" }));
        }
        let take = parse_cpu_takeover(&m);
        if self.last_take != Some(take) {
            self.last_take = Some(take);
            crate::tweaks::reconcile(take);
        }
        // module.prop description follows target / mode (written only when they change)
        let label = (get_u32(&m, "targetTemp").unwrap_or(0), read_trim(&p(MODE_PATH)).unwrap_or_default());
        if self.last_label.as_ref() != Some(&label) {
            self.last_label = Some(label);
            crate::prop::refresh(true);
        }
    }
    }
}

/// Optional `sensor=` override, read once at start.
pub fn sensor_override() -> Option<String> {
    let text = fs::read_to_string(p(CFG_PATH)).ok()?;
    parse(&text).get("sensor").cloned().filter(|s| !s.is_empty() && s != "auto")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_skips_comments() {
        let m = parse("# targetTemp=1\n\ntargetTemp = 85000\nfast=1\n");
        assert_eq!(get_u32(&m, "targetTemp"), Some(85000));
        assert_eq!(get_u32(&m, "fast"), Some(1));
    }

    #[test]
    fn with_target_replaces_first_line_only() {
        let t = with_target("# c\ntargetTemp=1\nbalance=2\n", 9);
        assert_eq!(t, "# c\ntargetTemp=9\nbalance=2\n");
        assert_eq!(with_target("balance=2\n", 7), "balance=2\ntargetTemp=7\n");
    }

    #[test]
    fn render_roundtrip() {
        let mut v = HashMap::new();
        for (k, d) in DEFAULTS {
            v.insert(k, d);
        }
        let g = GpuCfg { enabled: true, takeover: true, target: 70000, max_freq: 601000000, sensor: "gpu1,gpu2".into(), safe_max: 601000000 };
        let m = parse(&render(&v, Some("soc_max"), true, (500, 8000), true, false, &g));
        assert_eq!(GpuCfg::from_map(&m), g);
        assert_eq!(get_u32(&m, "balance"), Some(65000));
        assert_eq!(m.get("sensor").map(String::as_str), Some("soc_max"));
        assert_eq!(parse_smooth(&m), Some(true));
        assert_eq!(parse_lowlat(&m), Some(true));
        let m = parse(&render(&v, None, false, (500, 8000), false, true, &GpuCfg::default()));
        assert_eq!(GpuCfg::from_map(&m), GpuCfg::default());
        assert_eq!(parse_smooth(&m), Some(false));
        assert_eq!(parse_lowlat(&m), Some(false));
    }

    #[test]
    fn tuning_aliases() {
        let g = |s: &str| parse_smooth(&parse(s));
        assert_eq!(g("tuning=smooth"), Some(true));
        assert_eq!(g("tuning=SMOOTH"), Some(true));
        assert_eq!(g("tuning=aggressive"), Some(false));
        assert_eq!(g("tuning=raw"), Some(false));
        assert_eq!(g("filter=on"), Some(true));
        assert_eq!(g("filter=off"), Some(false));
        assert_eq!(g("tuning=raw\nfilter=on"), Some(false)); // tuning wins
        assert_eq!(g("tuning=banana"), None);
        assert_eq!(g(""), None);
        assert_eq!(parse_lowlat(&parse("lowlatency=on")), Some(true));
        assert_eq!(parse_lowlat(&parse("lowlatency=off")), Some(false));
        assert_eq!(parse_lowlat(&parse("lowlatency=maybe")), None);
    }

    fn sv(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn update_validation() {
        assert!(parse_update(&sv(&[])).is_err());
        assert!(parse_update(&sv(&["balance=abc"])).is_err());
        assert!(parse_update(&sv(&["balance=500"])).is_err()); // 0.5 C
        assert!(parse_update(&sv(&["balance=999999"])).is_err());
        assert!(parse_update(&sv(&["tuning=banana"])).is_err());
        assert!(parse_update(&sv(&["lowlatency=maybe"])).is_err());
        assert!(parse_update(&sv(&["sensor=a b"])).is_err());
        assert!(parse_update(&sv(&["sensor=$(reboot)"])).is_err());
        assert!(parse_update(&sv(&["bogus=1"])).is_err());
        assert!(parse_update(&sv(&["mode=turbo"])).is_err());
        assert!(parse_update(&sv(&["noequals"])).is_err());
        // all-or-nothing: a bad argument rejects the whole call
        assert!(parse_update(&sv(&["balance=70000", "tuning=banana"])).is_err());
        let u = parse_update(&sv(&["balance=70000", "tuning=smooth", "lowlatency=on", "sensor=soc_max"])).unwrap();
        assert_eq!(u.temps, vec![("balance", 70000)]);
        assert_eq!(u.smooth, Some(true));
        assert_eq!(u.lowlat, Some(true));
        assert_eq!(u.sensor.as_deref(), Some("soc_max"));
        assert_eq!(parse_update(&sv(&["sensor="])).unwrap().sensor.as_deref(), Some(""));
    }

    #[test]
    fn update_applies_and_target_follows_active_mode() {
        let base = apply_update("", &parse_update(&sv(&["mode=balance"])).unwrap());
        let m = parse(&base);
        assert_eq!(get_u32(&m, "targetTemp"), Some(65000));
        // edit the active mode's temperature -> targetTemp follows
        let t = apply_update(&base, &parse_update(&sv(&["balance=70000"])).unwrap());
        assert_eq!(get_u32(&parse(&t), "targetTemp"), Some(70000));
        // edit a different mode -> targetTemp untouched
        let t2 = apply_update(&t, &parse_update(&sv(&["fast=88000"])).unwrap());
        let m2 = parse(&t2);
        assert_eq!(get_u32(&m2, "fast"), Some(88000));
        assert_eq!(get_u32(&m2, "targetTemp"), Some(70000));
        // explicit targetTemp wins and is not overridden
        let t3 = apply_update(&t2, &parse_update(&sv(&["balance=71000", "targetTemp=64000"])).unwrap());
        assert_eq!(get_u32(&parse(&t3), "targetTemp"), Some(64000));
        // switching mode loads that mode's temperature
        let t4 = apply_update(&t3, &parse_update(&sv(&["mode=fast"])).unwrap());
        assert_eq!(get_u32(&parse(&t4), "targetTemp"), Some(88000));
        // other keys preserved
        let t5 = apply_update(&t4, &parse_update(&sv(&["tuning=smooth", "sensor=soc_max"])).unwrap());
        let m5 = parse(&t5);
        assert_eq!(parse_smooth(&m5), Some(true));
        assert_eq!(parse_lowlat(&m5), Some(false));
        assert_eq!(m5.get("sensor").map(String::as_str), Some("soc_max"));
        assert_eq!(get_u32(&m5, "targetTemp"), Some(88000));
        let t6 = apply_update(&t5, &parse_update(&sv(&["sensor="])).unwrap());
        assert_eq!(parse(&t6).get("sensor").map(String::as_str), Some("auto"));
    }
}
