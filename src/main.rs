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

//! CPU Max Frequency Limiter - Rust reimplementation of nakixii's Magisk_CPULimiter A.04.
//!
//! Sub-commands (single multi-call binary):
//!   cpu_limiter [daemon]        run the limiter (what the old `cpu_limiter` binary did)
//!   cpu_limiter mode <name>     switch mode (powersave|balance|performance|fast|init|pedestal)
//!   cpu_limiter init-config     create / migrate /sdcard/Android/naki/clmt/clmt.conf
//!   cpu_limiter boot-tweaks     disable hotplug / userspace thermal once at boot
//!   cpu_limiter set k=v ...     validate + write settings (used by the WebUI)
//!   cpu_limiter status          JSON snapshot (daemon, temperature, limits, config)
//!   cpu_limiter log [n]         last n log lines
//!   cpu_limiter start|stop|restart  control the daemon (stop also lifts the frequency limits)

mod config;
mod dump;
mod gpu;
mod info;
mod limiter;
mod prop;
mod sensor;
mod status;
mod tweaks;
mod util;

use std::process::exit;
use std::thread;
use std::time::Duration;
use util::*;

fn pin_to_little_cores() {
    let cpus = limiter::little_cpus();
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_ZERO(&mut set);
        for c in cpus {
            libc::CPU_SET(c, &mut set);
        }
        libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
        libc::setpriority(libc::PRIO_PROCESS, 0, -20);
    }
}

fn spawn(name: &str, f: impl FnOnce() + Send + 'static) {
    thread::Builder::new()
        .name(name.to_string())
        .spawn(f)
        .unwrap_or_else(|e| {
            log('E', &format!("cannot start {name}: {e}"));
            exit(1);
        });
}

fn daemon() -> ! {
    log_init();
    log('I', &format!("CPU & GPU Temperature Limiter {}", prop::module_version()));

    let clusters = limiter::discover();
    if clusters.is_empty() {
        log('E', "The cpufreq driver is unavailable");
        exit(1);
    }
    let sensors = sensor::discover(config::sensor_override().as_deref());
    if sensors.is_empty() {
        log('E', "No usable thermal sensor found (set sensor= in clmt.conf)");
        exit(1);
    }
    for s in &sensors {
        log('I', &format!("sensor: {}", s.display()));
    }
    for c in &clusters {
        log('I', &format!("policy{}: {} freqs, max {} kHz", c.id, c.freqs.len(), c.freqs[0]));
    }

    pin_to_little_cores();
    spawn("Limiter", move || limiter::run(clusters, sensor::Reader::new(sensors), config::Watcher::new(), gpu::GpuLimiter::new()));
    log('I', "CPU Limiter is running");
    loop {
        thread::sleep(Duration::from_secs(3600));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        None | Some("daemon") => daemon(),
        Some("mode") => exit(config::set_mode(args.get(2).map(String::as_str).unwrap_or(""))),
        Some("init-config") => {
            if let Err(e) = config::init_config() {
                eprintln!("init-config failed: {e}");
                exit(1);
            }
        }
        Some("boot-tweaks") => tweaks::run(),
        Some("revert-tweaks") => tweaks::revert(),
        Some("info") => exit(info::run()),
        Some("dump-gpu") => exit(dump::run()),
        Some("detect-gpu") => exit(gpu::detect(args[2..].iter().any(|a| a == "--force"), args[2..].iter().any(|a| a == "--first"))),
        Some("set") => exit(config::set_values(&args[2..])),
        Some("status") => println!("{}", status::status_json(true)),
        Some("watch") => exit(status::watch(args.get(2).and_then(|n| n.parse().ok()).unwrap_or(60))),
        Some("log") => exit(status::log_tail(args.get(2).and_then(|n| n.parse().ok()).unwrap_or(60))),
        Some("restart") => exit(status::restart()),
        Some("start") => exit(status::start()),
        Some("stop") => exit(status::stop()),
        Some("prop") => exit(prop::run(&args[2..])),
        Some("-V") | Some("--version") | Some("version") => {
            println!("cpu_limiter {}", env!("CARGO_PKG_VERSION"))
        }
        Some(_) => {
            eprintln!("usage: cpu_limiter [daemon | mode <name> | init-config | boot-tweaks | set k=v.. | status | watch [s] | log [n] | start | stop | restart | prop | detect-gpu [--force] | dump-gpu | info | version]");
            exit(2);
        }
    }
}
