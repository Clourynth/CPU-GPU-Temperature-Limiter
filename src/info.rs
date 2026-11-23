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

//! `cpu_limiter info`: what the installer prints about the device. Everything is detected at
//! run time (cpufreq policies, MIDR part numbers, devfreq node, device tree), nothing is tied to one SoC.

use crate::gpu::{self, GpuCfg};
use crate::util::*;
use std::fs;

/// Arm core names by MIDR part number (implementer 0x41).
fn arm_core(part: u32) -> Option<&'static str> {
    Some(match part {
        0xd03 => "Cortex-A53",
        0xd04 => "Cortex-A35",
        0xd05 => "Cortex-A55",
        0xd07 => "Cortex-A57",
        0xd08 => "Cortex-A72",
        0xd09 => "Cortex-A73",
        0xd0a => "Cortex-A75",
        0xd0b => "Cortex-A76",
        0xd0d => "Cortex-A77",
        0xd41 => "Cortex-A78",
        0xd44 => "Cortex-X1",
        0xd46 => "Cortex-A510",
        0xd47 => "Cortex-A710",
        0xd48 => "Cortex-X2",
        0xd4d => "Cortex-A715",
        0xd4e => "Cortex-X3",
        0xd80 => "Cortex-A520",
        0xd81 => "Cortex-A720",
        0xd82 => "Cortex-X4",
        0xd85 => "Cortex-X925",
        0xd87 => "Cortex-A725",
        _ => return None,
    })
}

/// MIDR_EL1 of a core -> Arm part number.
fn midr_part(cpu: u32) -> Option<(u32, u32)> {
    let t = read_trim(&p(&format!("/sys/devices/system/cpu/cpu{cpu}/regs/identification/midr_el1")))?;
    let v = u64::from_str_radix(t.trim_start_matches("0x"), 16).ok()?;
    Some((((v >> 24) & 0xff) as u32, ((v >> 4) & 0xfff) as u32))
}

/// Fallback: "CPU implementer" / "CPU part" of that processor in /proc/cpuinfo.
fn cpuinfo_part(cpu: u32) -> Option<(u32, u32)> {
    let text = fs::read_to_string(p("/proc/cpuinfo")).ok()?;
    let (mut cur, mut imp, mut part) = (None::<u32>, None::<u32>, None::<u32>);
    let hex = |s: &str| u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok();
    for l in text.lines().chain(std::iter::once("processor : 9999")) {
        let Some((k, v)) = l.split_once(':') else { continue };
        match k.trim() {
            "processor" => {
                if cur == Some(cpu) {
                    return imp.zip(part);
                }
                cur = v.trim().parse().ok();
                imp = None;
                part = None;
            }
            "CPU implementer" => imp = hex(v),
            "CPU part" => part = hex(v),
            _ => {}
        }
    }
    None
}

fn core_name(cpu: u32) -> Option<String> {
    let (imp, part) = midr_part(cpu).or_else(|| cpuinfo_part(cpu))?;
    if imp == 0x41 {
        arm_core(part).map(String::from)
    } else {
        None
    }
}

/// "0 1 2 3" -> "0-3", "0 1 4" -> "0-1,4"
pub fn cpu_ranges(cpus: &[u32]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < cpus.len() {
        let mut j = i;
        while j + 1 < cpus.len() && cpus[j + 1] == cpus[j] + 1 {
            j += 1;
        }
        out.push(if j > i { format!("{}-{}", cpus[i], cpus[j]) } else { cpus[i].to_string() });
        i = j + 1;
    }
    out.join(",")
}

fn mhz(khz: u32) -> u32 {
    (khz + 500) / 1000
}

fn soc_name() -> Option<String> {
    let maker = getprop("ro.soc.manufacturer");
    let model = getprop("ro.soc.model").or_else(|| getprop("ro.board.platform"));
    match (maker, model) {
        (Some(a), Some(b)) => Some(format!("{a} {b}")),
        (None, Some(b)) => Some(b),
        _ => fs::read_to_string(p("/proc/cpuinfo")).ok().and_then(|t| {
            t.lines().find_map(|l| l.strip_prefix("Hardware").and_then(|r| r.split_once(':')).map(|(_, v)| v.trim().to_string()))
        }),
    }
}

/// Mali kbase `gpuinfo`: "Mali-G615 10 cores r1p0 0x...". -> "Mali-G615 MC10".
fn gpu_model(dir: &std::path::Path) -> Option<String> {
    let t = read_trim(&dir.join("device/gpuinfo"))?;
    let w: Vec<&str> = t.split_whitespace().collect();
    let name = *w.first()?;
    if !name.to_ascii_lowercase().starts_with("mali") {
        return None;
    }
    match (w.get(1).and_then(|n| n.parse::<u32>().ok()), w.get(2)) {
        (Some(n), Some(c)) if c.starts_with("core") => Some(format!("{name} MC{n}")),
        _ => Some(name.to_string()),
    }
}

pub fn run() -> i32 {
    if let Some(s) = soc_name() {
        println!("SoC       : {s}");
    }
    let mut pol: Vec<(u32, String)> = Vec::new();
    if let Ok(rd) = fs::read_dir(p(CPUFREQ_DIR)) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if let Some(id) = n.strip_prefix("policy").and_then(|x| x.parse::<u32>().ok()) {
                pol.push((id, n));
            }
        }
    }
    pol.sort();
    for (_, n) in &pol {
        let d = p(CPUFREQ_DIR).join(n);
        let mut cpus: Vec<u32> = read_trim(&d.join("related_cpus")).unwrap_or_default().split_whitespace().filter_map(|x| x.parse().ok()).collect();
        cpus.sort_unstable();
        let lo: u32 = read_trim(&d.join("cpuinfo_min_freq")).and_then(|s| s.parse().ok()).unwrap_or(0);
        let hi: u32 = read_trim(&d.join("cpuinfo_max_freq")).and_then(|s| s.parse().ok()).unwrap_or(0);
        let name = cpus.first().and_then(|c| core_name(*c)).unwrap_or_else(|| "CPU core".into());
        println!("CPU {:<5} : {name}, {}-{} MHz", cpu_ranges(&cpus), mhz(lo), mhz(hi));
    }
    let cfg = GpuCfg::load();
    match gpu::find() {
        Some(g) => {
            let f = g.freqs();
            let kind = if g.kind == gpu::Kind::Mali { "Mali" } else { "Adreno" };
            let name = if g.kind == gpu::Kind::Mali { gpu_model(&g.dir).unwrap_or_else(|| "Mali".into()) } else { "Adreno".into() };
            match f.first().zip(f.last()) {
                Some((hi, lo)) => println!("GPU       : {name}, {}-{} MHz", lo / 1_000_000, hi / 1_000_000),
                None => println!("GPU       : {name}"),
            }
            if cfg.safe_max != 0 {
                println!("Safe clock : GPU {} MHz (system default limit)", cfg.safe_max / 1_000_000);
            } else {
                println!("Safe clock : GPU not detected (the system does not limit below the highest frequency)");
            }
            println!("GPU control: supported ({kind})");
            if g.kind != gpu::Kind::Mali {
                println!("GPU thermal takeover: not available (Mali GPUs only)");
            } else if !gpu::is_hyperos() {
                println!("GPU thermal takeover: not available (not HyperOS)");
            } else {
                match gpu::cooling_dev(&g) {
                    gpu::Cooling::Found(_) => println!("GPU thermal takeover: available (HyperOS)"),
                    gpu::Cooling::NotFound => println!("GPU thermal takeover: not available (GPU cooling device not found)"),
                    gpu::Cooling::Ambiguous(_) => println!("GPU thermal takeover: not available (GPU cooling device is ambiguous)"),
                }
            }
        }
        None => {
            println!("GPU       : not detected as Mali or Adreno");
            println!("GPU control: not supported (this GPU is not supported)");
        }
    }
    if crate::tweaks::supported() {
        println!("CPU thermal takeover: available (Xiaomi / Redmi / POCO)");
    } else {
        println!("CPU thermal takeover: not available (not a Xiaomi / Redmi / POCO device)");
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(cpu_ranges(&[0, 1, 2, 3]), "0-3");
        assert_eq!(cpu_ranges(&[7]), "7");
        assert_eq!(cpu_ranges(&[0, 1, 4, 6, 7]), "0-1,4,6-7");
    }

    #[test]
    fn core_table() {
        assert_eq!(arm_core(0xd81), Some("Cortex-A720"));
        assert_eq!(arm_core(0xd80), Some("Cortex-A520"));
        assert_eq!(arm_core(0xfff), None);
    }
}
