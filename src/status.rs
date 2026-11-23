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

//! Helper sub-commands used by the WebUI: `status` (JSON), `log [n]`, `restart`.

use crate::config;
use crate::sensor;
use crate::util::*;
use std::fs;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn num(v: Option<u32>) -> String {
    v.map(|n| n.to_string()).unwrap_or_else(|| "null".into())
}

fn read_u32(path: &std::path::Path) -> Option<u32> {
    read_trim(path)?.parse().ok()
}

/// PIDs of other running `cpu_limiter` daemons (not this process).
pub fn daemon_pids() -> Vec<u32> {
    let me = std::process::id();
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir("/proc") {
        for e in rd.flatten() {
            let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else { continue };
            if pid == me {
                continue;
            }
            let Ok(raw) = fs::read(e.path().join("cmdline")) else { continue };
            let args: Vec<&[u8]> = raw.split(|b| *b == 0).filter(|a| !a.is_empty()).collect();
            let Some(exe) = args.first() else { continue };
            let base = exe.rsplit(|b| *b == b'/').next().unwrap_or(exe);
            let is_bin = base == b"cpu_limiter";
            let is_daemon = args.get(1).map_or(true, |a| *a == b"daemon");
            if is_bin && is_daemon {
                out.push(pid);
            }
        }
    }
    out
}

/// Highest sensor reading right now (milli-degrees), plus a human readable sensor name.
fn sensor_now(filter: Option<&str>) -> (Option<u32>, String) {
    let paths = sensor::discover(filter);
    let mut max: Option<u32> = None;
    let mut names = Vec::new();
    for path in &paths {
        let zone = path.parent().and_then(|d| d.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let ty = path.parent().and_then(|d| read_trim(&d.join("type"))).unwrap_or_default();
        names.push(format!("{zone} ({ty})"));
        if let Some(v) = read_u32(path) {
            max = Some(max.map_or(v, |m| m.max(v)));
        }
    }
    (max, names.join(", "))
}

/// Every thermal zone that could be used as `sensor=`: "type" values with live temperature.
fn zones() -> Vec<(String, String, Option<u32>)> {
    let dir = p("/sys/class/thermal");
    let mut v = Vec::new();
    if let Ok(rd) = fs::read_dir(&dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with("thermal_zone") {
                continue;
            }
            let ty = read_trim(&e.path().join("type")).unwrap_or_default();
            let t = read_u32(&e.path().join("temp"));
            v.push((name, ty, t));
        }
    }
    v.sort_by_key(|(n, _, _)| n.trim_start_matches("thermal_zone").parse::<u32>().unwrap_or(u32::MAX));
    v
}

pub fn status_json(with_zones: bool) -> String {
    let text = fs::read_to_string(p(CFG_PATH)).unwrap_or_default();
    let m = config::parse(&text);
    let pids = daemon_pids();
    let sensor_cfg = m.get("sensor").cloned().filter(|s| !s.is_empty() && s != "auto");
    let (temp, sensor_name) = sensor_now(sensor_cfg.as_deref());
    let mode = read_trim(&p(MODE_PATH)).unwrap_or_default();

    let mut j = String::from("{");
    j.push_str(&format!("\"version\":{},", esc(&crate::prop::module_version())));
    j.push_str(&format!("\"running\":{},", !pids.is_empty()));
    j.push_str(&format!("\"pid\":{},", num(pids.first().copied())));
    j.push_str(&format!("\"configFound\":{},", !text.is_empty()));
    j.push_str(&format!("\"temp\":{},", num(temp)));
    j.push_str(&format!("\"sensorUsed\":{},", esc(&sensor_name)));
    j.push_str(&format!("\"mode\":{},", esc(&mode)));

    j.push_str("\"config\":{");
    let mut first = true;
    for (k, d) in config::DEFAULTS {
        if !first {
            j.push(',');
        }
        first = false;
        j.push_str(&format!("\"{k}\":{}", config::get_u32(&m, k).unwrap_or(d)));
    }
    j.push_str(&format!(",\"tuning\":{}", esc(if config::parse_smooth(&m).unwrap_or(false) { "smooth" } else { "raw" })));
    j.push_str(&format!(",\"cpuTakeover\":{}", esc(if config::parse_cpu_takeover(&m) { "on" } else { "off" })));
    j.push_str(&format!(",\"cpuTakeoverSupported\":{}", crate::tweaks::supported()));
    { let (sw, ss) = config::parse_smooth_params(&m); j.push_str(&format!(",\"smoothWindow\":{sw},\"smoothSpike\":{ss}")); }
    j.push_str(&format!(",\"lowlatency\":{}", esc(if config::parse_lowlat(&m).unwrap_or(false) { "on" } else { "off" })));
    j.push_str(&format!(",\"sensor\":{}", esc(sensor_cfg.as_deref().unwrap_or(""))));
    let g = crate::gpu::GpuCfg::from_map(&m);
    j.push_str(&format!(",\"gpu\":{}", esc(if g.enabled { "on" } else { "off" })));
    j.push_str(&format!(",\"gpuTakeover\":{}", esc(if g.takeover { "on" } else { "off" })));
    j.push_str(&format!(",\"gpuTargetTemp\":{}", g.target));
    j.push_str(&format!(",\"gpuMaxFreq\":{}", g.max_freq));
    j.push_str(&format!(",\"gpuSensor\":{}", esc(&g.sensor)));
    j.push_str(&format!(",\"gpuSafeMax\":{}", g.safe_max));
    j.push_str("},");
    j.push_str(&gpu_json(&g, with_zones));
    j.push(',');

    j.push_str("\"policies\":[");
    let mut pol: Vec<(u32, std::path::PathBuf)> = Vec::new();
    if let Ok(rd) = fs::read_dir(p(CPUFREQ_DIR)) {
        for e in rd.flatten() {
            if let Some(id) = e.file_name().to_string_lossy().strip_prefix("policy").and_then(|x| x.parse::<u32>().ok()) {
                pol.push((id, e.path()));
            }
        }
    }
    pol.sort();
    for (i, (id, d)) in pol.iter().enumerate() {
        if i > 0 {
            j.push(',');
        }
        let cpus = read_trim(&d.join("related_cpus")).unwrap_or_default();
        j.push_str(&format!(
            "{{\"id\":{id},\"cpus\":{},\"limit\":{},\"cur\":{},\"hw\":{}}}",
            esc(&cpus),
            num(read_u32(&d.join("scaling_max_freq"))),
            num(read_u32(&d.join("scaling_cur_freq"))),
            num(read_u32(&d.join("cpuinfo_max_freq")))
        ));
    }
    j.push(']');
    if with_zones {
        j.push_str(",\"zones\":[");
        for (i, (n, ty, t)) in zones().iter().enumerate() {
            if i > 0 {
                j.push(',');
            }
            j.push_str(&format!("{{\"zone\":{},\"type\":{},\"temp\":{}}}", esc(n), esc(ty), num(*t)));
        }
        j.push(']');
    }
    j.push('}');
    j
}

/// `"gpu":{...}`: live GPU state. `freqs` (static OPP table) only when `full`.
fn gpu_json(g: &crate::gpu::GpuCfg, full: bool) -> String {
    use crate::gpu;
    let found = gpu::find();
    let possible = found.as_ref().map_or(false, gpu::takeover_possible);
    let mut j = String::from("\"gpu\":{");
    j.push_str(&format!("\"available\":{}", found.is_some()));
    j.push_str(&format!(",\"supported\":{}", found.is_some()));
    j.push_str(&format!(",\"kind\":{}", found.as_ref().map_or("null".to_string(), |x| esc(x.kind_name()))));
    j.push_str(&format!(",\"hyperos\":{}", gpu::is_hyperos()));
    j.push_str(&format!(",\"takeoverPossible\":{possible}"));
    j.push_str(&format!(",\"ceiling\":{}", gpu::effective_max(g, possible)));
    let sensors = gpu::discover_sensors(&g.sensor);
    j.push_str(&format!(",\"temp\":{}", num(gpu::read_max(&sensors))));
    let names: Vec<String> = sensors
        .iter()
        .filter_map(|p| p.parent().and_then(|d| read_trim(&d.join("type"))))
        .collect();
    j.push_str(&format!(",\"sensorUsed\":{}", esc(&names.join(", "))));
    if let Some(d) = &found {
        j.push_str(&format!(",\"limit\":{}", num(d.limit())));
        j.push_str(&format!(",\"cur\":{}", num(d.cur())));
        if full {
            let f = d.freqs();
            j.push_str(&format!(",\"freqs\":[{}]", f.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")));
        }
    }
    match found.as_ref().map(gpu::cooling_dev) {
        Some(gpu::Cooling::Found(c)) => {
            let name = c.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            j.push_str(&format!(
                ",\"cooling\":{{\"name\":{},\"type\":{},\"state\":{}}}",
                esc(&name),
                esc(&read_trim(&c.join("type")).unwrap_or_default()),
                num(read_u32(&c.join("cur_state")))
            ));
        }
        _ => j.push_str(",\"cooling\":null"),
    }
    j.push('}');
    j
}

/// Write everything to fd 1, waiting at most `timeout_ms` whenever the pipe is full.
/// Returns false when the reader is gone or stuck, so a forgotten `watch` can never pile up.
fn write_stdout(buf: &[u8], timeout_ms: i32) -> bool {
    unsafe {
        let fl = libc::fcntl(1, libc::F_GETFL);
        if fl >= 0 {
            libc::fcntl(1, libc::F_SETFL, fl | libc::O_NONBLOCK);
        }
    }
    let mut off = 0;
    while off < buf.len() {
        let n = unsafe { libc::write(1, buf[off..].as_ptr() as *const libc::c_void, buf.len() - off) };
        if n > 0 {
            off += n as usize;
            continue;
        }
        let err = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        if n < 0 && (err == libc::EAGAIN || err == libc::EINTR) {
            let mut pfd = libc::pollfd { fd: 1, events: libc::POLLOUT, revents: 0 };
            let r = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
            if r <= 0 || pfd.revents & (libc::POLLERR | libc::POLLHUP) != 0 {
                return false;
            }
            continue;
        }
        return false; // EPIPE etc.
    }
    true
}

/// `cpu_limiter watch [secs]`: one JSON status line per second, for `ksu.spawn` in the WebUI.
/// Replaces polling with `exec`, which starts a new root shell every time. It stops by itself
/// after `secs` (default 60), when stdout closes, or when nobody reads for 3 s; the WebUI
/// simply starts a new one. Thermal zones are only included every 5th line.
pub fn watch(secs: u64) -> i32 {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(secs.clamp(1, 3600));
    let mut tick: u64 = 0;
    loop {
        let t0 = std::time::Instant::now();
        let mut line = status_json(tick % 5 == 0);
        line.push('\n');
        if !write_stdout(line.as_bytes(), 3000) {
            return 0;
        }
        tick += 1;
        if std::time::Instant::now() >= end {
            return 0;
        }
        let spent = t0.elapsed();
        if spent < std::time::Duration::from_millis(1000) {
            std::thread::sleep(std::time::Duration::from_millis(1000) - spent);
        }
    }
}

/// `cpu_limiter log [n]`: last n log lines (default 60).
pub fn log_tail(n: usize) -> i32 {
    let text = fs::read_to_string(p(LOG_PATH)).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    for l in &lines[lines.len().saturating_sub(n)..] {
        println!("{l}");
    }
    0
}

/// Stop every running daemon (SIGTERM, then SIGKILL after ~1 s). Returns true when none is left.
fn kill_daemons() -> bool {
    for pid in daemon_pids() {
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }
    for i in 0..40 {
        if daemon_pids().is_empty() {
            return true;
        }
        if i == 20 {
            for pid in daemon_pids() {
                unsafe {
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    daemon_pids().is_empty()
}

/// Remove the limit the daemon left behind: same state as right after boot-tweaks
/// (scaling_max_freq = 9999000, which the kernel clamps to the hardware maximum).
fn release_limits() {
    if let Ok(rd) = fs::read_dir(p(CPUFREQ_DIR)) {
        for e in rd.flatten() {
            if !e.file_name().to_string_lossy().starts_with("policy") {
                continue;
            }
            let node = e.path().join("scaling_max_freq");
            if node.exists() {
                chmod(&node, 0o666);
                let _ = fs::write(&node, "9999000");
                chmod(&node, 0o444);
            }
        }
    }
}

fn spawn_daemon() -> i32 {
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: cannot locate own binary: {e}");
            return 1;
        }
    };
    let mut cmd = Command::new(exe);
    cmd.arg("daemon").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    match cmd.spawn() {
        Ok(_) => {
            println!("ok");
            0
        }
        Err(e) => {
            eprintln!("error: cannot start daemon: {e}");
            1
        }
    }
}

/// `cpu_limiter start`: start the daemon unless one is already running.
pub fn start() -> i32 {
    if !daemon_pids().is_empty() {
        println!("ok");
        return 0;
    }
    spawn_daemon()
}

/// `cpu_limiter stop`: stop the daemon and lift the frequency limits it set.
pub fn stop() -> i32 {
    if !kill_daemons() {
        eprintln!("error: daemon did not exit");
        return 1;
    }
    release_limits();
    crate::tweaks::revert(); // no daemon = no limiter: give the system its own thermal control back
    if crate::gpu::GpuCfg::load().enabled {
        crate::gpu::release();
    }
    crate::prop::refresh(false);
    println!("ok");
    0
}

/// `cpu_limiter restart`: stop running daemon(s), start a fresh detached one.
pub fn restart() -> i32 {
    if !kill_daemons() {
        eprintln!("error: old daemon did not exit");
        return 1;
    }
    spawn_daemon()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_escape() {
        assert_eq!(esc("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
        assert_eq!(esc("\u{1}"), "\"\\u0001\"");
    }

    #[test]
    fn num_null() {
        assert_eq!(num(None), "null");
        assert_eq!(num(Some(5)), "5");
    }
}
