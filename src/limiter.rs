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

//! The actual limiter (FreqLimiter in the original): throttles scaling_max_freq.
//!
//! Behaviour reconstructed from the original aarch64 binary (A.04):
//!  * every 33.333 ms, once both target and current temperature are known;
//!  * the *highest* cpufreq policy is the one driven by temperature: if
//!    `temp <= target - 1500` it steps one entry up the frequency table,
//!    otherwise one entry down (frequency tables are ordered high -> low);
//!  * every other policy is pulled to the smallest frequency in its own table
//!    that is >= the current limit of the highest policy;
//!  * the chosen value is written to `policyN/scaling_max_freq`, which is
//!    chmod 0444 so that other (non-root-bypassing) writers are locked out.

use crate::util::*;
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::FileExt;
use std::path::PathBuf;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub const HYSTERESIS: u32 = 1500;
/// Re-assert the value even if unchanged, roughly once per second.
const REASSERT: Duration = Duration::from_secs(1);

pub struct Cluster {
    pub id: u32,
    /// Available frequencies in kHz, sorted high -> low (index 0 = max).
    pub freqs: Vec<u32>,
    pub idx: usize,
    path: PathBuf,
    file: Option<File>,
    last: Option<u32>,
}

impl Cluster {
    fn open(&mut self) {
        chmod(&self.path, 0o666);
        self.file = OpenOptions::new().write(true).open(&self.path).ok();
        chmod(&self.path, 0o444);
    }

    fn freq(&self) -> u32 {
        self.freqs[self.idx]
    }

    fn apply(&mut self, force: bool) {
        let v = self.freq();
        if !force && self.last == Some(v) {
            return;
        }
        if self.file.is_none() {
            self.open();
        }
        if let Some(f) = &self.file {
            match f.write_at(v.to_string().as_bytes(), 0) {
                Ok(_) => self.last = Some(v),
                Err(_) => {
                    self.file = None; // policy may have been re-created; reopen next tick
                    self.last = None;
                }
            }
        }
    }
}

/// Temperature driven step for the top cluster.
pub fn step_top(idx: usize, len: usize, cur: u32, target: u32) -> usize {
    if cur <= target.saturating_sub(HYSTERESIS) {
        idx.saturating_sub(1)
    } else {
        (idx + 1).min(len - 1)
    }
}

/// Where a lower cluster lands given the top cluster's current limit (kHz).
pub fn follow(idx: usize, freqs: &[u32], limit: u32) -> usize {
    let mut i = idx.min(freqs.len() - 1);
    while freqs[i] > limit && i + 1 < freqs.len() {
        i += 1;
    }
    while freqs[i] < limit && i > 0 {
        i -= 1;
    }
    i
}

/// Smooth tuning. `raw` walks the frequency table exactly like aggressive mode. When it differs
/// from the applied index, the applied index moves ONE step toward it and is then *held* for `window`
/// (changes during the hold are ignored; the next step comes after the hold). A temperature jump of `spike` or more within SPIKE_SPAN cancels the hold and
/// follows `raw` like aggressive for BYPASS_HOLD, after which the newest value is held again.
pub struct Smoother {
    raw: usize,
    applied: usize,
    temps: VecDeque<(Instant, u32)>,
    hold_until: Option<Instant>,
    bypass_until: Option<Instant>,
}

const SPIKE_SPAN: Duration = Duration::from_millis(100);
const BYPASS_HOLD: Duration = Duration::from_millis(200);

impl Smoother {
    pub fn new() -> Self {
        Smoother { raw: 0, applied: 0, temps: VecDeque::new(), hold_until: None, bypass_until: None }
    }

    pub fn reset(&mut self, idx: usize) {
        self.raw = idx;
        self.applied = idx;
        self.temps.clear();
        self.hold_until = None;
        self.bypass_until = None;
    }

    pub fn pick(&mut self, now: Instant, len: usize, cur: u32, target: u32, window: Duration, spike: u32) -> usize {
        self.raw = step_top(self.raw.min(len - 1), len, cur, target);
        self.applied = self.applied.min(len - 1);
        self.temps.push_back((now, cur));
        while self.temps.len() > 1 && self.temps.front().map_or(false, |&(t, _)| now.duration_since(t) > SPIKE_SPAN) {
            self.temps.pop_front();
        }
        let reference = self.temps.front().map_or(cur, |&(_, v)| v);
        if cur.abs_diff(reference) >= spike {
            self.bypass_until = Some(now + BYPASS_HOLD);
        }
        if self.bypass_until.map_or(false, |b| now < b) {
            self.applied = self.raw;
            self.hold_until = None;
            return self.applied;
        }
        if self.hold_until.map_or(false, |h| now < h) {
            return self.applied;
        }
        if self.raw != self.applied {
            // move one step toward the newest value, then hold
            self.applied = if self.raw > self.applied { self.applied + 1 } else { self.applied - 1 };
            self.hold_until = Some(now + window);
        } else {
            self.hold_until = None;
        }
        self.applied
    }
}

pub fn tick(clusters: &mut [Cluster], target: u32, cur: u32, force: bool, sm: Option<&mut Smoother>) {
    let n = clusters.len();
    let mut sm = sm;
    for i in 0..n {
        if i + 1 < n {
            let limit = clusters[n - 1].freq();
            let c = &mut clusters[i];
            c.idx = follow(c.idx, &c.freqs, limit);
        } else {
            let c = &mut clusters[i];
            c.idx = match sm.as_deref_mut() {
                Some(s) => s.pick(
                    Instant::now(),
                    c.freqs.len(),
                    cur,
                    target,
                    Duration::from_millis(load(&SMOOTH_WINDOW) as u64),
                    load(&SMOOTH_SPIKE),
                ),
                None => step_top(c.idx, c.freqs.len(), cur, target),
            };
        }
        clusters[i].apply(force);
    }
}

pub fn discover() -> Vec<Cluster> {
    let base = p(CPUFREQ_DIR);
    let mut ids: Vec<u32> = Vec::new();
    if let Ok(rd) = fs::read_dir(&base) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if let Some(n) = name.strip_prefix("policy").and_then(|s| s.parse::<u32>().ok()) {
                ids.push(n);
            }
        }
    }
    ids.sort_unstable();

    let mut out = Vec::new();
    for id in ids {
        let dir = base.join(format!("policy{id}"));
        let mut freqs: Vec<u32> = fs::read_to_string(dir.join("scaling_available_frequencies"))
            .unwrap_or_default()
            .split_whitespace()
            .filter_map(|s| s.parse().ok())
            .collect();
        freqs.sort_unstable_by(|a, b| b.cmp(a));
        freqs.dedup();
        if freqs.is_empty() {
            log('E', &format!("policy{id}: no scaling_available_frequencies, skipped"));
            continue;
        }
        out.push(Cluster {
            id,
            freqs,
            idx: 0,
            path: dir.join("scaling_max_freq"),
            file: None,
            last: None,
        });
    }
    out
}

/// CPUs of the lowest policy (the original pins itself to the little cluster).
pub fn little_cpus() -> Vec<usize> {
    let base = p(CPUFREQ_DIR);
    let mut best: Option<(u32, Vec<usize>)> = None;
    if let Ok(rd) = fs::read_dir(&base) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if let Some(id) = name.strip_prefix("policy").and_then(|s| s.parse::<u32>().ok()) {
                let cpus: Vec<usize> = read_trim(&e.path().join("related_cpus"))
                    .unwrap_or_default()
                    .split_whitespace()
                    .filter_map(|s| s.parse().ok())
                    .collect();
                if !cpus.is_empty() && best.as_ref().map_or(true, |(b, _)| id < *b) {
                    best = Some((id, cpus));
                }
            }
        }
    }
    best.map(|(_, c)| c).unwrap_or_else(|| vec![0, 1, 2, 3])
}

/// The single daemon loop: sensor read + CPU limiter every 33 ms (8 ms with lowlatency), the GPU
/// limiter every 100 ms and the config watcher every second, all on the same wakeup, so the
/// daemon needs one timer instead of four threads.
pub fn run(mut clusters: Vec<Cluster>, mut sensors: crate::sensor::Reader, mut watcher: crate::config::Watcher, mut gpu: crate::gpu::GpuLimiter) -> ! {
    let mut last_force = Instant::now();
    let mut sm = Smoother::new();
    let mut was_smooth = false;
    let mut slack_fast: Option<bool> = None;
    watcher.tick(); // TARGET and the tuning flags must be known before the first decision
    loop {
        // lowlatency=on: act every 8 ms (else 33 ms like the original), in both tunings.
        let fast = flag(&LOWLAT);
        if slack_fast != Some(fast) {
            slack_fast = Some(fast);
            // normal mode: let the kernel coalesce this timer with others (<= 1 ms late, period is 33 ms)
            unsafe { libc::prctl(libc::PR_SET_TIMERSLACK, if fast { 50_000u64 } else { 1_000_000u64 }, 0, 0, 0) };
        }
        std::thread::sleep(if fast { FAST_PERIOD } else { SLOW_PERIOD });

        let cur = sensors.read().unwrap_or_else(|| load(&CURRENT));
        let target = load(&TARGET);
        if target != 0 && cur != 0 {
            let force = last_force.elapsed() >= REASSERT;
            if force {
                last_force = Instant::now();
            }
            let smooth = flag(&SMOOTH);
            if smooth && !was_smooth {
                if let Some(top) = clusters.last() {
                    sm.reset(top.idx);
                }
            }
            was_smooth = smooth;
            tick(&mut clusters, target, cur, force, if smooth { Some(&mut sm) } else { None });
        }
        gpu.tick();
        watcher.tick();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_top_hysteresis() {
        // cold: climb toward idx 0, never below 0
        assert_eq!(step_top(3, 10, 70_000, 75_000), 2);
        assert_eq!(step_top(0, 10, 70_000, 75_000), 0);
        // exactly target-1500 still counts as cold (original: unsigned <=)
        assert_eq!(step_top(3, 10, 73_500, 75_000), 2);
        // hot: descend, clamped to the last entry
        assert_eq!(step_top(3, 10, 73_501, 75_000), 4);
        assert_eq!(step_top(9, 10, 90_000, 75_000), 9);
        // tiny target must not wrap around
        assert_eq!(step_top(0, 4, 40_000, 1_000), 1);
    }

    #[test]
    fn follow_is_ceiling_of_limit() {
        let f = [2_000_000, 1_800_000, 1_600_000, 1_400_000, 1_200_000];
        assert_eq!(follow(0, &f, 3_000_000), 0); // limit above table: stay at max
        assert_eq!(follow(0, &f, 1_600_000), 2); // exact match
        assert_eq!(follow(0, &f, 1_500_000), 2); // smallest freq >= limit
        assert_eq!(follow(2, &f, 1_900_000), 0); // limit rises: climb back
        assert_eq!(follow(0, &f, 500_000), 4); // below table: floor
    }

    #[test]
    fn smooth_holds_new_value_for_window() {
        let t0 = Instant::now();
        let w = Duration::from_millis(500);
        let ms = |n: u64| t0 + Duration::from_millis(n);
        let mut s = Smoother::new();
        s.reset(5);
        // hot (above target-1.5C), small steps: first change applies at once, then is held
        let first = s.pick(ms(0), 20, 76_000, 75_000, w, 8_000);
        assert_eq!(first, 6);
        for k in 1..15u64 {
            assert_eq!(s.pick(ms(k * 33), 20, 76_000, 75_000, w, 8_000), 6, "held at {k}");
        }
        // hold over (>=500 ms): only ONE more step toward the newest value, even though raw is far ahead
        let after = s.pick(ms(520), 20, 76_000, 75_000, w, 8_000);
        assert_eq!(after, 7);
        // and then holds again
        assert_eq!(s.pick(ms(553), 20, 76_000, 75_000, w, 8_000), after);
    }

    #[test]
    fn smooth_jump_bypasses_hold() {
        let t0 = Instant::now();
        let w = Duration::from_millis(500);
        let ms = |n: u64| t0 + Duration::from_millis(n);
        let mut s = Smoother::new();
        s.reset(5);
        let held = s.pick(ms(0), 20, 74_000, 75_000, w, 8_000);
        assert_eq!(s.pick(ms(33), 20, 74_000, 75_000, w, 8_000), held);
        // +10 C at once: follows raw immediately although the hold is still running
        let a = s.pick(ms(66), 20, 84_000, 75_000, w, 8_000);
        let b = s.pick(ms(99), 20, 84_000, 75_000, w, 8_000);
        assert!(b > a, "bypass should keep following raw: {a} {b}");
        // spike 0 = always bypass == aggressive
        let mut g = Smoother::new();
        g.reset(5);
        for k in 0..10u64 {
            let out = g.pick(ms(k * 33), 20, 76_000, 75_000, w, 0);
            assert_eq!(out, g.raw);
        }
    }
}
