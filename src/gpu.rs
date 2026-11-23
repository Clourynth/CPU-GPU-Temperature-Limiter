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

//! GPU limiter: a fixed frequency ceiling plus a temperature driven step-down.
//!
//! Two kinds of GPU are driven, found by plain path checks (no SoC vendor logic):
//!  * Mali: `/sys/class/devfreq/*mali*`, knob `max_freq` (Hz), table `available_frequencies`;
//!  * Adreno: `/sys/class/kgsl/kgsl-3d0`, knob `max_clock_mhz` (MHz), table `freq_table_mhz`.
//!
//! Mali only: the system's own GPU throttling is a thermal cooling device whose `type` is
//! `devfreq-<devfreq node name>` (e.g. `devfreq-13000000.mali`); `gpuTakeover=on` (HyperOS) keeps its
//! `cur_state` at 0 so only this limiter decides. If it cannot be identified without guessing,
//! the takeover is not done.
//!
//! Algorithm (same idea as the CPU limiter, with its own sensor and target):
//!  * `ceiling` = largest OPP <= `gpuMaxFreq` (0 = top of the table). The GPU is never
//!    allowed above it, however cold the SoC is;
//!  * every 100 ms: temp <= target - 1500 -> one OPP up (never above the ceiling),
//!    otherwise one OPP down (down to the lowest OPP).

use crate::config;
use crate::sensor;
use crate::util::*;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub const DEFAULT_SENSOR: &str = "auto";
pub const DEFAULT_TARGET: u32 = 53_000;
const KGSL: &str = "/sys/class/kgsl/kgsl-3d0";
const PERIOD: Duration = Duration::from_millis(100);
const REASSERT: Duration = Duration::from_secs(1);
const CFG_EVERY: Duration = Duration::from_secs(1);
pub const HYSTERESIS: u32 = 1500;

#[derive(Debug, Clone, PartialEq)]
pub struct GpuCfg {
    pub enabled: bool,
    pub takeover: bool,
    pub target: u32,
    /// Hz, 0 = no ceiling.
    pub max_freq: u32,
    pub sensor: String,
    /// Hz, GPU max frequency the system itself runs with (0 = unknown). Detected at install/boot.
    pub safe_max: u32,
}

impl Default for GpuCfg {
    fn default() -> Self {
        GpuCfg { enabled: false, takeover: false, target: DEFAULT_TARGET, max_freq: 0, sensor: DEFAULT_SENSOR.into(), safe_max: 0 }
    }
}

fn onoff(m: &HashMap<String, String>, k: &str) -> Option<bool> {
    match m.get(k)?.to_ascii_lowercase().as_str() {
        "on" | "1" | "true" => Some(true),
        "off" | "0" | "false" => Some(false),
        _ => None,
    }
}

impl GpuCfg {
    pub fn from_map(m: &HashMap<String, String>) -> Self {
        let d = GpuCfg::default();
        GpuCfg {
            enabled: onoff(m, "gpu").unwrap_or(d.enabled),
            takeover: onoff(m, "gpuTakeover").unwrap_or(d.takeover),
            target: config::get_u32(m, "gpuTargetTemp")
                .filter(|t| (config::TEMP_MIN..=config::TEMP_MAX).contains(t))
                .unwrap_or(d.target),
            max_freq: config::get_u32(m, "gpuMaxFreq").unwrap_or(d.max_freq),
            sensor: m.get("gpuSensor").filter(|s| !s.is_empty()).cloned().unwrap_or(d.sensor),
            safe_max: config::get_u32(m, "gpuSafeMax").unwrap_or(0),
        }
    }

    pub fn load() -> Self {
        let text = fs::read_to_string(p(CFG_PATH)).unwrap_or_default();
        Self::from_map(&config::parse(&text))
    }
}

/// Index of the ceiling in a table sorted high -> low.
pub fn ceiling_idx(freqs: &[u32], max: u32) -> usize {
    if freqs.is_empty() || max == 0 {
        return 0;
    }
    freqs.iter().position(|f| *f <= max).unwrap_or(freqs.len() - 1)
}

/// One temperature step. `ceil` is the index of the highest frequency that is allowed.
pub fn step(idx: usize, ceil: usize, len: usize, cur: u32, target: u32) -> usize {
    let n = if cur <= target.saturating_sub(HYSTERESIS) {
        idx.saturating_sub(1)
    } else {
        (idx + 1).min(len - 1)
    };
    n.max(ceil)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Mali,
    Adreno,
}

/// The GPU this module can drive.
#[derive(Debug, Clone)]
pub struct Gpu {
    pub kind: Kind,
    pub dir: PathBuf,
}

impl Gpu {
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            Kind::Mali => "mali",
            Kind::Adreno => "adreno",
        }
    }
    /// File holding the frequency ceiling.
    pub fn knob(&self) -> PathBuf {
        self.dir.join(if self.kind == Kind::Mali { "max_freq" } else { "max_clock_mhz" })
    }
    /// Unit of the knob and of the table, in Hz.
    fn scale(&self) -> u32 {
        if self.kind == Kind::Mali { 1 } else { 1_000_000 }
    }
    /// Table of allowed frequencies (Hz), high -> low.
    pub fn freqs(&self) -> Vec<u32> {
        let file = if self.kind == Kind::Mali { "available_frequencies" } else { "freq_table_mhz" };
        let sc = self.scale() as f64;
        let mut v: Vec<u32> = fs::read_to_string(self.dir.join(file))
            .unwrap_or_default()
            .split_whitespace()
            .filter_map(|s| s.parse::<f64>().ok())
            .filter(|f| *f > 0.0)
            .map(|f| (f * sc).round() as u32)
            .collect();
        v.sort_unstable_by(|a, b| b.cmp(a));
        v.dedup();
        v
    }
    /// Current ceiling (Hz).
    pub fn limit(&self) -> Option<u32> {
        let v: f64 = read_trim(&self.knob())?.parse().ok()?;
        Some((v * self.scale() as f64).round() as u32)
    }
    pub fn set_limit(&self, hz: u32) -> std::io::Result<()> {
        fs::write(self.knob(), (hz / self.scale()).to_string())
    }
    /// Current clock (Hz), for display.
    pub fn cur(&self) -> Option<u32> {
        let (file, sc) = if self.kind == Kind::Mali { ("cur_freq", 1.0) } else { ("gpuclk", 1.0) };
        let v: f64 = read_trim(&self.dir.join(file))?.parse().ok()?;
        Some((v * sc).round() as u32)
    }
    /// Name of the devfreq node (Mali) used to find the matching cooling device.
    fn node(&self) -> String {
        self.dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
    }
}

/// Every `/sys/class/devfreq/*mali*` node that has a ceiling knob and a frequency table, sorted.
pub fn mali_nodes() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(p("/sys/class/devfreq"))
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    found.retain(|d| {
        let name = d.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        name.contains("mali") && d.join("max_freq").exists() && d.join("available_frequencies").exists()
    });
    found.sort();
    found
}

/// Mali first, then Adreno. `None` = this GPU is not supported.
pub fn find() -> Option<Gpu> {
    if let Some(dir) = mali_nodes().into_iter().next() {
        return Some(Gpu { kind: Kind::Mali, dir });
    }
    let k = p(KGSL);
    (k.join("max_clock_mhz").exists() && k.join("freq_table_mhz").exists()).then(|| Gpu { kind: Kind::Adreno, dir: k })
}

#[derive(Debug, PartialEq)]
pub enum Cooling {
    Found(PathBuf),
    NotFound,
    /// Several candidates and none is the exact pair of the devfreq node: not guessed.
    Ambiguous(Vec<String>),
}

/// `devs` = (path, type) of every cooling device. The exact pair `devfreq-<node>` wins; otherwise a
/// single device whose type mentions "mali" or "gpu"; more than one is ambiguous.
pub fn pick_cooling(node: &str, devs: &[(PathBuf, String)]) -> Cooling {
    let exact = format!("devfreq-{}", node.to_ascii_lowercase());
    if let Some((path, _)) = devs.iter().find(|(_, t)| t.to_ascii_lowercase() == exact) {
        return Cooling::Found(path.clone());
    }
    let cands: Vec<&(PathBuf, String)> = devs
        .iter()
        .filter(|(_, t)| {
            let t = t.to_ascii_lowercase();
            t.contains("mali") || t.contains("gpu")
        })
        .collect();
    match cands.len() {
        0 => Cooling::NotFound,
        1 => Cooling::Found(cands[0].0.clone()),
        _ => Cooling::Ambiguous(cands.iter().map(|(p, t)| format!("{}={}", p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), t)).collect()),
    }
}

/// The cooling device of a Mali GPU (Adreno has none to take over).
pub fn cooling_dev(g: &Gpu) -> Cooling {
    if g.kind != Kind::Mali {
        return Cooling::NotFound;
    }
    let mut dirs: Vec<PathBuf> = fs::read_dir(p("/sys/class/thermal"))
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    dirs.retain(|d| d.file_name().map_or(false, |n| n.to_string_lossy().starts_with("cooling_device")) && d.join("cur_state").exists());
    dirs.sort();
    let devs: Vec<(PathBuf, String)> = dirs.into_iter().filter_map(|d| read_trim(&d.join("type")).map(|t| (d, t))).collect();
    pick_cooling(&g.node(), &devs)
}

pub fn sensor_types(list: &str) -> Vec<&str> {
    list.split(',').map(str::trim).filter(|s| !s.is_empty()).collect()
}

/// `auto` (or empty) = every `gpu*` zone. Otherwise zones whose type equals one of the listed
/// types; if none of them exist on this device, fall back to every `gpu*` zone.
pub fn discover_sensors(list: &str) -> Vec<PathBuf> {
    let zones = sensor::zone_list();
    let dir = p("/sys/class/thermal");
    let want = sensor_types(list);
    let pick = |f: &dyn Fn(&str) -> bool| -> Vec<PathBuf> {
        zones.iter().filter(|(_, t)| f(t)).map(|(n, _)| dir.join(n).join("temp")).collect()
    };
    let auto = want.is_empty() || want.contains(&"auto");
    let v = if auto { Vec::new() } else { pick(&|t| want.contains(&t)) };
    if v.is_empty() {
        pick(&|t| t.to_ascii_lowercase().starts_with("gpu"))
    } else {
        v
    }
}

/// Highest reading (milli-degrees) of `paths`.
pub fn read_max(paths: &[PathBuf]) -> Option<u32> {
    paths.iter().filter_map(|p| read_trim(p)?.parse::<i64>().ok()).filter(|v| *v > 0 && *v < 200_000).map(|v| v as u32).max()
}

/// Sensor files kept open between ticks (pread, like the CPU TempMonitor): no open/close per tick.
pub struct Sensors(Vec<(PathBuf, Option<fs::File>)>);

impl Sensors {
    pub fn new(paths: Vec<PathBuf>) -> Self {
        Sensors(paths.into_iter().map(|p| (p, None)).collect())
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn read_max(&mut self) -> Option<u32> {
        let mut max: Option<u32> = None;
        for (path, slot) in self.0.iter_mut() {
            if slot.is_none() {
                *slot = fs::File::open(&*path).ok();
            }
            if let Some(f) = slot {
                match sensor::read_temp(f) {
                    Some(v) => max = Some(max.map_or(v, |m| m.max(v))),
                    None => *slot = None, // reopen next tick
                }
            }
        }
        max
    }
}

/// HyperOS (`ro.mi.os.version.name`). Only there is the GPU cooling device the system's throttle.
pub fn is_hyperos() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| getprop("ro.mi.os.version.name").is_some())
}

/// The GPU thermal takeover exists for a Mali GPU on HyperOS only, whatever the other ROMs do.
pub fn takeover_possible(g: &Gpu) -> bool {
    g.kind == Kind::Mali && is_hyperos()
}

/// Takeover only does anything while the limiter is on.
pub fn effective_takeover(c: &GpuCfg, possible: bool) -> bool {
    c.enabled && c.takeover && possible
}

/// Ceiling actually enforced (Hz, 0 = none). On HyperOS without takeover the system's own thermal
/// logic is in charge, so the ceiling is held at the system's safe maximum.
pub fn clamp_max(max: u32, safe: u32, hyperos: bool, takeover: bool) -> u32 {
    if hyperos && !takeover && safe != 0 {
        if max == 0 { safe } else { max.min(safe) }
    } else {
        max
    }
}

pub fn effective_max(c: &GpuCfg, possible: bool) -> u32 {
    clamp_max(c.max_freq, c.safe_max, possible, c.takeover)
}

/// Hand the GPU back to the system: write the system's own maximum (the detected safe max), or the
/// top OPP when that was never detected. devfreq keeps what was written, so this is not optional.
pub fn release() {
    if let Some(g) = find() {
        let f = g.freqs();
        let safe = GpuCfg::load().safe_max;
        let target = if safe != 0 { f.iter().copied().find(|x| *x <= safe).or(f.last().copied()) } else { f.first().copied() };
        if let Some(t) = target {
            let _ = g.set_limit(t);
        }
    }
}

/// `cpu_limiter detect-gpu [--force]`: remember the max frequency the system itself runs the GPU
/// with (the ceiling knob below the top OPP, e.g. 601 MHz). Run at install and before the daemon
/// starts at boot, i.e. before this module has written anything. Existing detections are kept.
pub fn detect(force: bool, first: bool) -> i32 {
    let Some(g) = find() else {
        println!("no supported gpu");
        return 0;
    };
    let cfg = GpuCfg::load();
    let mut safe = cfg.safe_max;
    let mut updates: Vec<String> = Vec::new();
    if cfg.safe_max != 0 && !force {
        println!("kept {}", cfg.safe_max);
    } else if cfg.enabled && !force {
        println!("skipped: limiter active, value may be ours");
    } else {
        let freqs = g.freqs();
        let cur: u32 = g.limit().unwrap_or(0);
        safe = match freqs.first() {
            Some(top) if cur > 0 && cur < *top => freqs.iter().copied().find(|f| *f <= cur).unwrap_or(0),
            _ => 0,
        };
        println!("detected {safe}");
        updates.push(format!("gpuSafeMax={safe}"));
    }
    // First install: default the GPU ceiling to the system's own safe clock (when one was found).
    if first && safe > 0 && cfg.max_freq == 0 {
        updates.push(format!("gpuMaxFreq={safe}"));
    }
    if updates.is_empty() {
        return 0;
    }
    config::set_values_quiet(&updates)
}

pub struct GpuLimiter {
    cfg: GpuCfg,
    gpu: Option<Gpu>,
    freqs: Vec<u32>,
    cooling: Option<PathBuf>,
    possible: bool,
    sensors: Sensors,
    sensor_key: String,
    idx: usize,
    active: bool,
    last_cfg: Instant,
    last_force: Instant,
    last_scan: Instant,
    last_tick: Instant,
    last_written: Option<u32>,
    warned: bool,
    announced: bool,
}

impl GpuLimiter {
    pub fn new() -> Self {
        GpuLimiter {
            cfg: GpuCfg::load(),
            gpu: None,
            freqs: Vec::new(),
            cooling: None,
            possible: false,
            sensors: Sensors::new(Vec::new()),
            sensor_key: String::new(),
            idx: 0,
            active: false,
            last_cfg: Instant::now() - CFG_EVERY,
            last_force: Instant::now(),
            last_scan: Instant::now(),
            last_tick: Instant::now(),
            last_written: None,
            warned: false,
            announced: false,
        }
    }

    /// Called from the daemon loop on every wakeup; does the work every 100 ms while the limiter
    /// is active and only once per second (config check) while it is off.
    pub fn tick(&mut self) {
        let period = if self.active { PERIOD } else { CFG_EVERY };
        if self.last_tick.elapsed() < period {
            return;
        }
        self.last_tick = Instant::now();

    if self.last_cfg.elapsed() >= CFG_EVERY {
        self.last_cfg = Instant::now();
        let new = GpuCfg::load();
        if new != self.cfg || !self.announced {
            if self.announced {
                log('I', &format!(
                    "gpu: {} ceiling={} MHz target={} takeover={} sensor={}",
                    if new.enabled { "on" } else { "off" },
                    new.max_freq / 1_000_000,
                    new.target,
                    if new.takeover { "on" } else { "off" },
                    new.sensor
                ));
            }
            self.announced = true;
            self.cfg = new;
            self.last_written = None;
        }
        if self.gpu.is_none() {
            self.gpu = find();
            if let Some(g) = &self.gpu {
                self.freqs = g.freqs();
                self.possible = takeover_possible(g);
                let cool = cooling_dev(g);
                let cool_txt = match &cool {
                    Cooling::Found(c) => c.display().to_string(),
                    Cooling::NotFound => "none".into(),
                    Cooling::Ambiguous(l) => format!("ambiguous ({}), takeover not done", l.join(", ")),
                };
                self.cooling = if let Cooling::Found(c) = cool { Some(c) } else { None };
                log('I', &format!(
                    "gpu: {} {} ({} OPP{}), cooling {}",
                    g.kind_name(),
                    g.dir.display(),
                    self.freqs.len(),
                    self.freqs.first().zip(self.freqs.last()).map_or(String::new(), |(a, b)| format!(", {}-{} MHz", b / 1_000_000, a / 1_000_000)),
                    cool_txt
                ));
            }
        }
        if self.sensor_key != self.cfg.sensor || self.last_scan.elapsed() >= Duration::from_secs(10) {
            self.last_scan = Instant::now();
            let changed = self.sensor_key != self.cfg.sensor;
            self.sensor_key = self.cfg.sensor.clone();
            let before = self.sensors.len();
            self.sensors = Sensors::new(discover_sensors(&self.cfg.sensor));
            if changed || before != self.sensors.len() {
                log('I', &format!("gpu sensor: {} zone(s)", self.sensors.len()));
            }
        }
    }

    let ok = self.cfg.enabled && self.gpu.is_some() && !self.freqs.is_empty();
    if ok != self.active {
        self.active = ok;
        if ok {
            self.idx = ceiling_idx(&self.freqs, effective_max(&self.cfg, self.possible));
            self.last_written = None;
        } else if self.gpu.is_some() {
            release();
            log('I', "gpu: limit lifted");
        }
    }
    if !self.active {
        return;
    }
    let Some(gpu) = self.gpu.clone() else { return };

    let ceil = ceiling_idx(&self.freqs, effective_max(&self.cfg, self.possible));
    match self.sensors.read_max() {
        Some(t) => self.idx = step(self.idx, ceil, self.freqs.len(), t, self.cfg.target),
        None => self.idx = self.idx.max(ceil),
    }
    let want = self.freqs[self.idx];
    let force = self.last_force.elapsed() >= REASSERT;
    if force {
        self.last_force = Instant::now();
    }
    if force || self.last_written != Some(want) {
        match gpu.set_limit(want) {
            Ok(_) => {
                self.last_written = Some(want);
                self.warned = false;
            }
            Err(e) => {
                self.last_written = None;
                if !self.warned {
                    self.warned = true;
                    log('W', &format!("gpu: cannot write the frequency ceiling: {e}"));
                }
            }
        }
        if effective_takeover(&self.cfg, self.possible) {
            if let Some(c) = &self.cooling {
                let node = c.join("cur_state");
                if read_trim(&node).map_or(false, |s| s != "0") {
                    let _ = fs::write(&node, "0");
                }
            }
        }
    }

    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const F: [u32; 6] = [1_400_000_000, 900_000_000, 700_000_000, 601_000_000, 463_000_000, 265_000_000];

    #[test]
    fn ceiling_picks_largest_not_above() {
        assert_eq!(ceiling_idx(&F, 0), 0);
        assert_eq!(ceiling_idx(&F, 601_000_000), 3);
        assert_eq!(ceiling_idx(&F, 650_000_000), 3);
        assert_eq!(ceiling_idx(&F, 2_000_000_000), 0);
        assert_eq!(ceiling_idx(&F, 100_000_000), 5);
    }

    #[test]
    fn step_never_exceeds_ceiling() {
        // cold: climbs but stops at ceiling (idx 3)
        assert_eq!(step(5, 3, 6, 40_000, 75_000), 4);
        assert_eq!(step(3, 3, 6, 40_000, 75_000), 3);
        // ceiling lowered under us: clamp immediately
        assert_eq!(step(0, 3, 6, 40_000, 75_000), 3);
        // hot: descend to the table end
        assert_eq!(step(3, 3, 6, 80_000, 75_000), 4);
        assert_eq!(step(5, 3, 6, 80_000, 75_000), 5);
        // hysteresis edge
        assert_eq!(step(4, 3, 6, 73_500, 75_000), 3);
        assert_eq!(step(4, 3, 6, 73_501, 75_000), 5);
    }

    #[test]
    fn cfg_defaults_and_parse() {
        let d = GpuCfg::from_map(&HashMap::new());
        assert_eq!(d, GpuCfg::default());
        assert_eq!(d.sensor, "auto");
        let m = config::parse("gpu=on\ngpuTakeover=on\ngpuTargetTemp=65000\ngpuMaxFreq=601000000\ngpuSensor=gpu1\n");
        let c = GpuCfg::from_map(&m);
        assert!(c.enabled && c.takeover);
        assert_eq!((c.target, c.max_freq, c.sensor.as_str()), (65000, 601_000_000, "gpu1"));
        // out-of-range temperature falls back to the default
        assert_eq!(GpuCfg::from_map(&config::parse("gpuTargetTemp=5")).target, DEFAULT_TARGET);
    }

    #[test]
    fn clamp_to_system_max_only_on_hyperos_without_takeover() {
        let (z, s) = (0, 601_000_000);
        assert_eq!(clamp_max(z, s, true, false), s); // no ceiling -> system max
        assert_eq!(clamp_max(1_400_000_000, s, true, false), s);
        assert_eq!(clamp_max(463_000_000, s, true, false), 463_000_000);
        assert_eq!(clamp_max(1_400_000_000, s, true, true), 1_400_000_000); // takeover: free
        assert_eq!(clamp_max(1_400_000_000, s, false, false), 1_400_000_000); // other ROMs: free
        assert_eq!(clamp_max(1_400_000_000, 0, true, false), 1_400_000_000); // safe max unknown
    }

    fn dev(n: &str, t: &str) -> (PathBuf, String) {
        (PathBuf::from(n), t.to_string())
    }

    #[test]
    fn cooling_exact_pair_wins() {
        let d = vec![dev("cooling_device0", "gpu-extra"), dev("cooling_device2", "devfreq-13000000.mali")];
        assert_eq!(pick_cooling("13000000.mali", &d), Cooling::Found(PathBuf::from("cooling_device2")));
    }

    #[test]
    fn cooling_single_candidate_or_refuse() {
        let one = vec![dev("cooling_device1", "thermal-cpufreq-0"), dev("cooling_device5", "mtk-gpu-cooler")];
        assert_eq!(pick_cooling("13000000.mali", &one), Cooling::Found(PathBuf::from("cooling_device5")));
        assert_eq!(pick_cooling("x.mali", &[dev("cooling_device1", "thermal-cpufreq-0")]), Cooling::NotFound);
        let two = vec![dev("cooling_device3", "gpu-a"), dev("cooling_device4", "devfreq-other.mali")];
        assert!(matches!(pick_cooling("13000000.mali", &two), Cooling::Ambiguous(_)));
    }

    #[test]
    fn sensor_list_split() {
        assert_eq!(sensor_types(" gpu1, gpu2 ,"), vec!["gpu1", "gpu2"]);
    }
}
