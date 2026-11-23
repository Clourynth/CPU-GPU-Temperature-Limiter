/*
 * CPU & GPU Temperature Limiter
 * Copyright (C) 2026  Clourynth
 *
 * Credits: nakixii (Magisk_CPULimiter) and ahmed-alnassif.
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

/* CPU & GPU Temperature Limiter WebUI — bicara dengan binary modul lewat ksu.exec (root). */
(() => {
  "use strict";

  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => Array.from(r.querySelectorAll(s));
  const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
  const deg = (mc) => { const d = mc / 1000; return Number.isInteger(d) ? String(d) : d.toFixed(1); };
  const ghz = (khz) => (khz / 1e6).toFixed(2);
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const REDUCED = matchMedia("(prefers-reduced-motion: reduce)").matches;
  // Tulis ke DOM hanya kalau nilainya berubah, supaya tiap detik tidak ada layout/repaint sia-sia.
  const t = (k, v) => window.I18N.t(k, v);
  const setText = (el, v) => { if (el.textContent !== v) el.textContent = v; };
  const setHidden = (el, v) => { if (el.hidden !== v) el.hidden = v; };
  const isOpen = (el) => el.classList.contains("open");

  // Panel simpan: muncul dengan pegas, hilang dengan meluncur turun.
  let saveLeave = 0;
  function showSavebar(visible) {
    const el = $("#savebar"), sc = $("#scroller");
    if (visible) {
      sc.classList.add("has-save");
      clearTimeout(saveLeave);
      el.classList.remove("leaving");
      if (el.hidden) el.hidden = false;
    } else if (!el.hidden && !el.classList.contains("leaving")) {
      if (REDUCED) { el.hidden = true; sc.classList.remove("has-save"); return; }
      el.classList.add("leaving");
      saveLeave = setTimeout(() => { el.hidden = true; el.classList.remove("leaving"); sc.classList.remove("has-save"); }, 270);
    }
  }
  const setVar = (el, k, v) => { if (el.style.getPropertyValue(k) !== v) el.style.setProperty(k, v); };

  const MODES = [
    { key: "powersave", icon: "eco" },
    { key: "balance", icon: "balance" },
    { key: "performance", icon: "rocket_launch" },
    { key: "fast", icon: "bolt" },
  ];
  MODES.forEach((m) => Object.defineProperties(m, {
    name: { get: () => t(`mode.${m.key}.short`) },
    full: { get: () => t(`mode.${m.key}`) },
  }));
  const TEMP_MIN = 30000, TEMP_MAX = 120000;

  /* ---------- ikon ---------- */
  function setIcon(svg, name) {
    svg.setAttribute("viewBox", "0 -960 960 960");
    svg.setAttribute("aria-hidden", "true");
    svg.innerHTML = `<path d="${window.ICONS[name] || ""}"/>`;
  }
  const icon = (name) => `<svg class="ic" viewBox="0 -960 960 960" aria-hidden="true"><path d="${window.ICONS[name] || ""}"/></svg>`;
  $$("svg[data-i]").forEach((el) => setIcon(el, el.dataset.i));

  /* ---------- jembatan ke root shell ---------- */
  let MODDIR = "/data/adb/modules/cpu_limiter_rs";
  try {
    const raw = window.ksu && typeof ksu.moduleInfo === "function" ? ksu.moduleInfo() : null;
    const mi = typeof raw === "string" ? JSON.parse(raw) : raw;
    if (mi && typeof mi.moduleDir === "string" && /^\/[A-Za-z0-9_./-]+$/.test(mi.moduleDir)) MODDIR = mi.moduleDir;
  } catch (_) { /* pakai bawaan */ }
  const BIN = `${MODDIR}/cpu_limiter`;

  let seq = 0;
  function sh(cmd, timeoutMs = 15000) {
    return new Promise((resolve) => {
      if (!window.ksu || typeof ksu.exec !== "function") {
        resolve({ errno: -1, stdout: "", stderr: t("err.nomanager") });
        return;
      }
      const cb = `__clmt_${++seq}_${Date.now()}`;
      let done = false;
      const finish = (r) => { if (done) return; done = true; delete window[cb]; clearTimeout(timer); resolve(r); };
      const timer = setTimeout(() => finish({ errno: -2, stdout: "", stderr: t("err.timeout") }), timeoutMs);
      window[cb] = (errno, stdout, stderr) => finish({ errno, stdout: stdout || "", stderr: stderr || "" });
      try { ksu.exec(cmd, JSON.stringify({}), cb); } catch (e) { finish({ errno: -1, stdout: "", stderr: String(e) }); }
    });
  }
  const bin = (args) => sh(`'${BIN}' ${args}`);

  /* ---------- state ---------- */
  let S = null;            // status terakhir dari binary
  let draft = null;        // nilai yang sedang diedit
  let base = null;         // nilai server saat draft dibuat (untuk deteksi perubahan)
  let busy = false;
  let daemonBusy = false;
  const sliders = [];      // { sync() }

  const KEYS = ["target", "powersave", "balance", "performance", "fast", "cpuTakeover", "tuning", "smoothWindow", "smoothSpike", "lowlatency", "sensor", "gpu", "gpuTakeover", "gpuTarget", "gpuMax", "gpuSensor"];
  const fromConfig = (c) => ({
    target: c.targetTemp, powersave: c.powersave, balance: c.balance, performance: c.performance, fast: c.fast,
    cpuTakeover: c.cpuTakeover || "on", tuning: c.tuning, smoothWindow: c.smoothWindow ?? 310, smoothSpike: c.smoothSpike ?? 5000, lowlatency: c.lowlatency, sensor: c.sensor || "",
    gpu: c.gpu || "off", gpuTakeover: c.gpuTakeover || "off", gpuTarget: c.gpuTargetTemp || 53000, gpuMax: c.gpuMaxFreq || 0, gpuSensor: c.gpuSensor || "auto",
  });
  const changedKeys = () => (draft && base ? KEYS.filter((k) => draft[k] !== base[k]) : []);
  const isDirty = () => changedKeys().length > 0;

  function activeMode(d) {
    if (S && S.mode) {
      const m = MODES.find((x) => x.key === S.mode);
      if (m && d[m.key] === d.target) return m.key;
    }
    const m = MODES.find((x) => d[x.key] === d.target);
    return m ? m.key : null;
  }

  /* ---------- snackbar ---------- */
  let snackTimer = 0;
  function snack(msg, err = false, ms = 0) {
    const el = $("#snack");
    el.textContent = msg;
    el.className = "snack" + (err ? " err" : "");
    el.hidden = false;
    clearTimeout(snackTimer);
    snackTimer = setTimeout(() => { el.hidden = true; }, ms || (err ? 4200 : 2400));
  }

  /* ---------- slider ekspresif ---------- */
  function makeSlider({ label, get, set, compact, min = TEMP_MIN, max = TEMP_MAX, div = 1000, inputStep = 0.5, stepBy = 1000, fmt = (v) => deg(v) + "°C" }) {
    const wrap = document.createElement("div");
    wrap.className = "slider" + (compact ? " compact" : "");
    const id = "sl" + Math.random().toString(36).slice(2, 8);
    wrap.innerHTML = `
      <div class="slider-top"><label class="slider-label" for="${id}">${label}</label><output class="slider-value" for="${id}"></output></div>
      <div class="slider-row">
        <button class="step-btn" type="button" aria-label="${t("step.minus")}">${icon("remove")}</button>
        <input id="${id}" type="range" min="${min / div}" max="${max / div}" step="${inputStep}">
        <button class="step-btn" type="button" aria-label="${t("step.plus")}">${icon("add")}</button>
      </div>`;
    const input = $("input", wrap), out = $("output", wrap);
    const [minus, plus] = $$(".step-btn", wrap);
    const paint = () => {
      const v = get() / div;
      input.value = v;
      const pct = ((clamp(v, input.min, input.max) - input.min) / (input.max - input.min)) * 100;
      input.style.setProperty("--p", pct + "%");
      out.textContent = fmt(get());
    };
    const commit = (mc) => { set(clamp(Math.round(mc / (inputStep * div)) * inputStep * div, min, max)); onDraft(); };
    input.addEventListener("input", () => commit(parseFloat(input.value) * div));
    minus.addEventListener("click", () => commit(get() - stepBy));
    plus.addEventListener("click", () => commit(get() + stepBy));
    sliders.push({ sync: paint });
    paint();
    return wrap;
  }

  /* ---------- bagian pengaturan ---------- */
  function buildControls() {
    sliders.length = 0;
    const st = $("#slider-target"); st.innerHTML = "";
    st.appendChild(makeSlider({ label: t("t.target.label"), get: () => draft.target, set: (v) => { draft.target = v; } }));

    const pr = $("#presets-in"); pr.innerHTML = "";
    MODES.forEach((m, i) => {
      const sl = makeSlider({
        label: m.full, compact: true, get: () => draft[m.key],
        set: (v) => { const wasActive = draft[m.key] === draft.target; draft[m.key] = v; if (wasActive) draft.target = v; },
      });
      sl.style.setProperty("--i", i);
      pr.appendChild(sl);
    });
    const sw = $("#slider-smooth-window"); sw.innerHTML = "";
    sw.appendChild(makeSlider({ label: t("smooth.window"), min: 10, max: 5000, div: 1, inputStep: 10, stepBy: 50, fmt: (v) => v + " ms", get: () => draft.smoothWindow, set: (v) => { draft.smoothWindow = v; } }));
    const ss = $("#slider-smooth-spike"); ss.innerHTML = "";
    ss.appendChild(makeSlider({ label: t("smooth.spike"), min: 0, max: 50000, div: 1000, inputStep: 0.5, stepBy: 1000, get: () => draft.smoothSpike, set: (v) => { draft.smoothSpike = v; } }));
    const gt = $("#slider-gpu-temp"); gt.innerHTML = "";
    gt.appendChild(makeSlider({ label: t("gpu.templimit.label"), get: () => draft.gpuTarget, set: (v) => { draft.gpuTarget = v; } }));
    ensureGpuSlider(true);
    renderModes(); renderTuning(); renderLowlat(); renderSensor(); renderGpuControls(); renderGpuSensor(true);
  }

  function syncControls() { sliders.forEach((s) => s.sync()); renderModes(); renderTuning(); renderLowlat(); renderCpuTake(); renderSensorSummary(); renderGpuControls(); }

  // Grey out + block interaction (keyboard too, via `inert`) for settings that cannot be used right now.
  function lock(el, locked) {
    if (!el) return;
    if (el.classList.contains("locked") !== locked) { el.classList.toggle("locked", locked); el.toggleAttribute("inert", locked); }
  }
  function renderLocks() {
    const down = !(S && S.running);                       // daemon stopped or unreachable: nothing to configure
    const gpuOff = !draft || draft.gpu !== "on";          // GPU limiter off: only its own switch stays usable
    const want = new Map($$("#page-cpu .group, #page-gpu .group").map((g) => [g, down]));
    if (gpuOff) ["#h-gpu-max", "#h-gpu-temp", "#h-gpu-sensor"].forEach((id) => want.set($(id).closest(".group"), true));
    want.forEach((v, g) => lock(g, v));
    lock($("#sw-gpu-take").closest(".row-switch"), !down && gpuOff);
  }

  function renderCpuTake() {
    const el = $("#sw-cpu-take");
    if (!draft) return;
    const sup = !!(S && S.config && S.config.cpuTakeoverSupported);   // only Xiaomi / Redmi / POCO
    setHidden($("#h-cpu-ctl").closest(".group"), !sup);
    const up = !!(S && S.running);                       // daemon stopped: takeover is released and cannot be switched
    const on = up && draft.cpuTakeover === "on";
    if (el.checked !== on) el.checked = on;
    if (el.disabled !== !up) el.disabled = !up;
    const row = el.closest(".row-switch");
    if (row.classList.contains("disabled") === up) row.classList.toggle("disabled", !up);
  }
  $("#sw-cpu-take").addEventListener("change", (e) => { draft.cpuTakeover = e.target.checked ? "on" : "off"; onDraft(); });

  function renderModes() {
    const box = $("#modes");
    const active = draft ? activeMode(draft) : null;
    if (!box.children.length) {
      MODES.forEach((m) => {
        const b = document.createElement("button");
        b.type = "button"; b.className = "mode"; b.setAttribute("role", "radio"); b.dataset.k = m.key;
        b.innerHTML = `${icon(m.icon)}<span class="m-name">${m.name}</span><span class="m-temp"></span>`;
        b.addEventListener("click", () => { draft.target = draft[m.key]; onDraft(); });
        box.appendChild(b);
      });
    }
    $$("button", box).forEach((b) => {
      b.setAttribute("aria-checked", String(b.dataset.k === active));
      $(".m-temp", b).textContent = draft ? deg(draft[b.dataset.k]) + "°" : "";
    });
  }

  function renderTuning() {
    $$("#seg-tuning button").forEach((b) => b.setAttribute("aria-checked", String(draft && b.dataset.v === draft.tuning)));
    const sm = !!draft && draft.tuning === "smooth";
    $("#help-tuning").textContent = t(sm ? "tuning.smooth.help" : "tuning.raw.help");
    setHidden($("#smooth-warn"), !sm);
    setHidden($("#smooth-extra"), !sm);
  }
  $$("#seg-tuning button").forEach((b) => b.addEventListener("click", () => { draft.tuning = b.dataset.v; onDraft(); }));

  function renderLowlat() { $("#sw-lowlat").checked = !!draft && draft.lowlatency === "on"; }
  $("#sw-lowlat").addEventListener("change", (e) => { draft.lowlatency = e.target.checked ? "on" : "off"; onDraft(); });

  function toggleCollapse(btn, box, force) {
    const open = force != null ? force : !isOpen(box);
    btn.setAttribute("aria-expanded", String(open));
    box.classList.toggle("open", open);
    return open;
  }
  $("#presets-toggle").addEventListener("click", (e) => toggleCollapse(e.currentTarget, $("#presets")));

  /* ---------- sensor ---------- */
  function zoneOptions() {
    const seen = new Set(), list = [];
    (S ? S.zones : []).forEach((z) => { if (z.type && !seen.has(z.type)) { seen.add(z.type); list.push(z); } });
    return list;
  }
  function renderSensorSummary() {
    if (!draft) return;
    setText($("#sensor-title"), draft.sensor || t("sensor.auto"));
    setText($("#sensor-sub"), S && S.sensorUsed ? t("sensor.used", { s: S.sensorUsed }) : t("sensor.none"));
  }
  let sensorSig = "";
  function renderSensor(force) {
    renderSensorSummary();
    if (!force && !isOpen($("#sensor-box"))) return;
    const q = $("#sensor-filter").value.trim().toLowerCase();
    const ul = $("#sensor-list");
    const rows = [{ value: "", title: t("sensor.auto"), sub: t("sensor.auto.sub"), temp: null }];
    if (draft.sensor && !zoneOptions().some((z) => z.type === draft.sensor)) rows.push({ value: draft.sensor, title: draft.sensor, sub: t("sensor.missing"), temp: null });
    zoneOptions().forEach((z) => rows.push({ value: z.type, title: z.type, sub: z.zone, temp: z.temp }));
    const shown = rows.filter((r) => !q || r.title.toLowerCase().includes(q) || r.sub.toLowerCase().includes(q));
    const sig = shown.map((r) => r.value + "|" + r.sub).join(",") + "#" + draft.sensor;
    if (sig === sensorSig) { // daftar sama: cukup perbarui angka suhu
      $$("li", ul).forEach((li, i) => { const t = $(".z-temp", li); if (t && shown[i] && shown[i].temp != null) setText(t, deg(shown[i].temp) + "°"); });
      return;
    }
    sensorSig = sig;
    const scroll = ul.scrollTop;
    ul.innerHTML = "";
    shown.forEach((r, i) => {
      const li = document.createElement("li");
      li.style.setProperty("--i", Math.min(i, 7));
      const b = document.createElement("button");
      b.type = "button"; b.setAttribute("role", "radio");
      b.setAttribute("aria-checked", String(r.value === draft.sensor));
      b.innerHTML = `<span class="z-name"></span><span class="z-sub"></span>${r.temp != null ? '<span class="z-temp"></span>' : ""}`;
      $(".z-name", b).textContent = r.title; $(".z-sub", b).textContent = r.sub;
      if (r.temp != null) $(".z-temp", b).textContent = deg(r.temp) + "°";
      b.addEventListener("click", () => { draft.sensor = r.value; onDraft(); });
      li.appendChild(b); ul.appendChild(li);
    });
    ul.scrollTop = scroll;
  }
  $("#sensor-toggle").addEventListener("click", (e) => {
    const box = $("#sensor-box");
    if (!isOpen(box)) {
      renderSensor(true);       // isi daftar dulu (masih tertutup), lalu buka supaya barisnya ikut beranimasi
      void box.offsetHeight;
      toggleCollapse(e.currentTarget, box, true);
    } else toggleCollapse(e.currentTarget, box, false);
  });
  $("#sensor-filter").addEventListener("input", renderSensor);


  /* ---------- GPU: kontrol ---------- */
  const mhz = (hz) => Math.round(hz / 1e6);
  let gpuFreqsCache = [];
  const gpuFreqs = () => gpuFreqsCache.slice().sort((a, b) => a - b); // naik, Hz
  const gpuAvailable = () => !!(S && S.gpu && S.gpu.available);
  function gpuIndexOf(hz) {
    const f = gpuFreqs();
    if (!f.length) return 0;
    if (!hz) return f.length - 1;                 // 0 = tanpa batas = frekuensi tertinggi
    let i = 0; f.forEach((v, k) => { if (v <= hz) i = k; });
    return i;
  }

  // Slider bertanda: satu langkah = satu entri tabel OPP GPU.
  // Frekuensi aman = maksimum yang dipakai sistem sendiri (terdeteksi saat instal). Di atasnya dianggap berbahaya.
  function safeIndex(f) {
    const safe = S && S.config ? S.config.gpuSafeMax : 0;
    if (!safe || f.length < 2) return -1;
    let i = -1; f.forEach((v, k) => { if (v <= safe) i = k; });
    return i >= 0 && i < f.length - 1 ? i : -1;
  }
  function renderGpuDanger() {
    const el = $("#gpu-danger");
    const f = gpuFreqs(), safeIdx = safeIndex(f);
    if (!draft || safeIdx < 0) { setHidden(el, true); return; }
    setHidden(el, false);
    const i = gpuIndexOf(draft.gpuMax), lim = mhz(f[safeIdx]);
    const hy = !!(S && S.gpu && S.gpu.takeoverPossible);
    const need = hy && draft.gpuTakeover !== "on";   // di HyperOS, zona merah butuh ambil alih thermal
    let msg;
    if (i > safeIdx) {
      msg = t("danger.over", { f: mhz(f[i]), lim });
      if (need) msg += t("danger.over.need", { lim });
    } else {
      msg = t("danger.zone", { lim });
      if (need) msg += t("danger.zone.need");
    }
    setText($("#gpu-danger-text"), msg);
    el.classList.toggle("hot", i > safeIdx);
  }
  let gpuSig = "", gpuEntry = null;
  function ensureGpuSlider(force) {
    const f = gpuFreqs(), sig = f.join(",") + "|" + (S && S.config ? S.config.gpuSafeMax : 0);
    const host = $("#slider-gpu-max");
    if (!force && sig === gpuSig && host.firstChild) return;
    gpuSig = sig;
    if (gpuEntry) { const k = sliders.indexOf(gpuEntry); if (k >= 0) sliders.splice(k, 1); gpuEntry = null; }
    host.innerHTML = "";
    if (f.length < 2) { host.innerHTML = `<p class="help">${t("gpu.freq.table")}</p>`; return; }
    const n = f.length;
    const safeIdx = safeIndex(f);
    const wrap = document.createElement("div");
    wrap.className = "slider";
    const id = "sl" + Math.random().toString(36).slice(2, 8);
    wrap.innerHTML = `
      <div class="slider-top"><label class="slider-label" for="${id}">${t("gpu.maxfreq.label")}</label><output class="slider-value" for="${id}"></output></div>
      <div class="slider-row">
        <button class="step-btn" type="button" aria-label="${t("step.down")}">${icon("remove")}</button>
        <div class="range-wrap"><div class="range-fill"></div><div class="range-ticks" aria-hidden="true">${"<i></i>".repeat(n)}</div>
          <input id="${id}" class="ticked" type="range" min="0" max="${n - 1}" step="1"></div>
        <button class="step-btn" type="button" aria-label="${t("step.up")}">${icon("add")}</button>
      </div>
      <div class="range-ends"><span>${mhz(f[0])} MHz</span><span class="range-hint"></span><span>${mhz(f[n - 1])} MHz</span></div>`;
    const input = $("input", wrap), out = $("output", wrap), hint = $(".range-hint", wrap), fill = $(".range-fill", wrap);
    const [minus, plus] = $$(".step-btn", wrap);
    const paint = () => {
      const i = gpuIndexOf(draft.gpuMax);
      input.value = i;
      fill.style.setProperty("--p", ((i / (n - 1)) * 100) + "%");
      fill.style.setProperty("--d", safeIdx < 0 ? "101%" : (((safeIdx + 0.5) / (n - 1)) * 100) + "%");
      renderGpuDanger();
      $$(".range-ticks i", wrap).forEach((t, k) => { t.classList.toggle("on", k <= i); t.classList.toggle("bad", safeIdx >= 0 && k > safeIdx); });
      setText(out, `${mhz(f[i])} MHz`);
      setText(hint, i === n - 1 ? t("nolimit") : "");
    };
    const commit = (i) => { i = clamp(Math.round(i), 0, n - 1); draft.gpuMax = i === n - 1 ? 0 : f[i]; onDraft(); };
    input.addEventListener("input", () => commit(+input.value));
    minus.addEventListener("click", () => commit(gpuIndexOf(draft.gpuMax) - 1));
    plus.addEventListener("click", () => commit(gpuIndexOf(draft.gpuMax) + 1));
    host.appendChild(wrap);
    gpuEntry = { sync: paint };
    sliders.push(gpuEntry);
    paint();
  }

  function renderGpuControls() {
    if (!draft) return;
    const ok = gpuAvailable(), hy = !!(S && S.gpu && S.gpu.takeoverPossible);
    const sw = $("#sw-gpu"), tk = $("#sw-gpu-take");
    setHidden(tk.closest(".row-switch"), !(S && S.gpu && S.gpu.kind === "mali"));   // thermal takeover exists for Mali only
    if (sw.checked !== (draft.gpu === "on")) sw.checked = draft.gpu === "on";
    const tkOn = hy && draft.gpuTakeover === "on";
    if (tk.checked !== tkOn) tk.checked = tkOn;
    sw.disabled = !ok;
    const tkOff = !ok || !hy || draft.gpu !== "on";
    tk.disabled = tkOff;
    tk.closest(".row-switch").classList.toggle("disabled", tkOff);
    sw.closest(".row-switch").classList.toggle("disabled", !ok);
    setText($("#gpu-take-sub"), t(!hy ? "gpu.take.sub.nohyper" : draft.gpu !== "on" ? "gpu.take.sub.needon" : "gpu.take.sub.on"));
    [$("#slider-gpu-max"), $("#slider-gpu-temp")].forEach((el) => el.classList.toggle("dim", draft.gpu !== "on"));
    renderGpuDanger();
    renderLocks();
  }
  $("#sw-gpu").addEventListener("change", (e) => { draft.gpu = e.target.checked ? "on" : "off"; if (!e.target.checked) draft.gpuTakeover = "off"; onDraft(); });
  $("#sw-gpu-take").addEventListener("change", (e) => { draft.gpuTakeover = e.target.checked ? "on" : "off"; onDraft(); });

  /* ---------- GPU: sensor (boleh lebih dari satu) ---------- */
  // "auto" (atau kosong) = Otomatis; selain itu daftar tipe zona yang dipilih.
  const gpuList = () => (draft.gpuSensor || "").split(",").map((x) => x.trim()).filter((x) => x && x !== "auto");
  const gpuAuto = () => gpuList().length === 0;
  const gpuSel = () => (gpuAuto() ? ["auto"] : gpuList());
  let gpuSensorSig = "";
  function renderGpuSensor(force) {
    if (!draft) return;
    const sel = gpuSel();
    setText($("#gpu-sensor-title"), gpuAuto() ? t("sensor.auto") : sel.join(", "));
    const used = S && S.gpu && S.gpu.sensorUsed;
    setText($("#gpu-sensor-sub"), used ? t("sensor.used", { s: used }) : t("gpu.sensor.none"));
    if (!force && !isOpen($("#gpu-sensor-box"))) return;
    const q = $("#gpu-sensor-filter").value.trim().toLowerCase();
    const ul = $("#gpu-sensor-list");
    const rows = zoneOptions().map((z) => ({ value: z.type, title: z.type, sub: z.zone, temp: z.temp }));
    gpuList().forEach((x) => { if (!rows.some((r) => r.value === x)) rows.unshift({ value: x, title: x, sub: t("sensor.missing"), temp: null }); });
    rows.unshift({ value: "auto", title: t("sensor.auto"), sub: t("gpu.sensor.auto.sub"), temp: null });
    const shown = rows.filter((r) => !q || r.title.toLowerCase().includes(q) || r.sub.toLowerCase().includes(q));
    const sig = shown.map((r) => r.value + "|" + r.sub).join(",");
    if (sig === gpuSensorSig) {
      $$("li", ul).forEach((li, i) => {
        const r = shown[i], b = $("button", li);
        const on = String(sel.includes(r.value));
        if (b.getAttribute("aria-checked") !== on) b.setAttribute("aria-checked", on);
        const t = $(".z-temp", li); if (t && r.temp != null) setText(t, deg(r.temp) + "°");
      });
      return;
    }
    gpuSensorSig = sig;
    const scroll = ul.scrollTop;
    ul.innerHTML = "";
    shown.forEach((r, i) => {
      const li = document.createElement("li");
      li.style.setProperty("--i", Math.min(i, 7));
      const b = document.createElement("button");
      b.type = "button"; b.setAttribute("role", r.value === "auto" ? "radio" : "checkbox");
      b.setAttribute("aria-checked", String(sel.includes(r.value)));
      b.innerHTML = `<span class="z-name"></span><span class="z-sub"></span>${r.temp != null ? '<span class="z-temp"></span>' : ""}`;
      $(".z-name", b).textContent = r.title; $(".z-sub", b).textContent = r.sub;
      if (r.temp != null) $(".z-temp", b).textContent = deg(r.temp) + "°";
      b.addEventListener("click", () => {
        if (r.value === "auto") { draft.gpuSensor = "auto"; onDraft(); return; }
        const cur = gpuList(), has = cur.includes(r.value);
        const next = has ? cur.filter((x) => x !== r.value) : [...cur, r.value];
        draft.gpuSensor = next.length ? next.join(",") : "auto"; onDraft();
      });
      li.appendChild(b); ul.appendChild(li);
    });
    ul.scrollTop = scroll;
  }
  $("#gpu-sensor-toggle").addEventListener("click", (e) => {
    const box = $("#gpu-sensor-box");
    if (!isOpen(box)) {
      renderGpuSensor(true);
      void box.offsetHeight;
      toggleCollapse(e.currentTarget, box, true);
    } else toggleCollapse(e.currentTarget, box, false);
  });
  $("#gpu-sensor-filter").addEventListener("input", () => renderGpuSensor(true));

  /* ---------- Halaman (navbar bawah) ---------- */
  const TITLES = { cpu: "page.cpu", gpu: "page.gpu" };
  let page = "cpu";
  function showPage(name, initial) {
    if (!TITLES[name] || (name === page && !initial)) return;
    quiet(500);
    $$(".page").forEach((el) => { setHidden(el, el.id !== "page-" + name); });
    const to = $("#page-" + name);
    if (!initial && !REDUCED) {
      to.style.setProperty("--dx", name === "gpu" ? "28px" : "-28px");
      to.classList.remove("enter"); void to.offsetWidth; to.classList.add("enter");
    }
    $$(".nav-item").forEach((b) => { if (b.dataset.page === name) b.setAttribute("aria-current", "page"); else b.removeAttribute("aria-current"); });
    setText($("#page-title"), t(TITLES[name]));
    page = name;
    document.getElementById("scroller").scrollTo(0, 0);   // pindah halaman selalu mulai dari atas
    if (S) { renderGpu(); renderGpuSensor(); }
  }
  $$(".nav-item").forEach((b) => b.addEventListener("click", () => showPage(b.dataset.page)));
  // the bar never scrolls the page: swipes and wheel on it do nothing (taps still reach the buttons)
  ["touchmove", "wheel"].forEach((ev) => $("#nav").addEventListener(ev, (e) => { if (e.cancelable) e.preventDefault(); }, { passive: false }));

  /* ---------- hero ---------- */
  const hex = (h) => { h = h.trim().replace("#", ""); return [0, 2, 4].map((i) => parseInt(h.substr(i, 2), 16)); };
  const mix = (a, b, t) => a.map((v, i) => Math.round(v + (b[i] - v) * t));
  const rgb = (c) => `rgb(${c[0]},${c[1]},${c[2]})`;

  // Warna skala panas dibaca sekali (dan lagi kalau tema sistem berubah), bukan tiap detik.
  let stops = null;
  const readStops = () => {
    const cs = getComputedStyle(document.documentElement);
    const g = (n) => hex(cs.getPropertyValue(n));
    stops = {};
    ["bg", "fg", "ck"].forEach((k) => { stops[k] = [g(`--h0-${k}`), g(`--h1-${k}`), g(`--h2-${k}`)]; });
  };
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => { stops = null; heatQ.cpu = heatQ.gpu = -1; });
  function heatColors(r) {
    if (!stops) readStops();
    const pick = (k) => { const [c0, c1, c2] = stops[k]; return r < 0.5 ? mix(c0, c1, r * 2) : mix(c1, c2, (r - 0.5) * 2); };
    return { bg: rgb(pick("bg")), fg: rgb(pick("fg")), ck: rgb(pick("ck")) };
  }

  // Bentuk cookie: tetap (10 lobus), digambar sekali. Yang berubah mengikuti suhu hanya warnanya,
  // supaya tidak ada animasi cat ulang (fill/d) di lapisan yang sedang berputar.
  function cookiePath(lobes, amp) {
    const R = 84, pts = [];
    for (let i = 0; i <= 240; i++) {
      const t = (i / 240) * Math.PI * 2;
      const rad = R * (1 + amp * Math.cos(lobes * t));
      pts.push((rad * Math.cos(t)).toFixed(1) + " " + (rad * Math.sin(t)).toFixed(1));
    }
    return "M" + pts.join("L") + "Z";
  }
  const heatQ = { cpu: -1, gpu: -1 };
  $("#cookie-path").setAttribute("d", cookiePath(10, 0.08));
  $("#gpu-cookie-path").setAttribute("d", cookiePath(7, 0.1));
  $("#splash-path").setAttribute("d", $("#cookie-path").getAttribute("d"));

  // Warna + gelombang kartu suhu; hanya berubah per 1/32 langkah.
  function paintHeat(key, hero, clip, temp, target) {
    const r = temp == null ? 0 : clamp((temp - TEMP_MIN) / Math.max(1, target - TEMP_MIN), 0, 1);
    const q = Math.round(r * 32);
    if (q === heatQ[key]) return;
    heatQ[key] = q;
    const rq = q / 32, col = heatColors(rq);
    setVar(hero, "--hero-bg", col.bg); setVar(hero, "--hero-fg", col.fg); setVar(hero, "--ck", col.ck);
    setVar(clip, "--w", (rq * 100).toFixed(1) + "%");
  }

  // Layar muat: hilang begitu data pertama tampil (atau error / lewat 8 detik).
  let splashGone = false;
  function hideSplash() {
    if (splashGone) return;
    splashGone = true;
    const el = $("#splash");
    el.classList.add("out");
    setTimeout(() => el.remove(), 450);
  }
  setTimeout(hideSplash, 8000);

  function renderHero() {
    const hero = $("#hero");
    const pill = $("#state-pill");
    if (!S) { return; }
    const c = S.config, temp = S.temp, running = S.running;

    const cls = "pill " + (running ? "on" : "off");
    if (pill.className !== cls) pill.className = cls;
    setText($("#state-text"), t(running ? "pill.running" : "pill.stopped"));

    const am = MODES.find((m) => m.key === (S.mode || "")) || MODES.find((m) => c[m.key] === c.targetTemp);
    const modeName = am && c[am.key] === c.targetTemp ? am.full : t("mode.custom");
    setText($("#hero-target"), t("target", { t: deg(c.targetTemp), mode: modeName }));

    const trigger = c.targetTemp - 1500;
    let note;
    if (!running) note = t("note.cpu.stopped");
    else if (temp == null) note = t("note.cpu.nosensor");
    else if (temp > trigger) note = t("note.cpu.limiting");
    else note = t("note.cpu.margin", { d: deg(trigger - temp) });
    setText($("#hero-note"), note);
    setText($("#temp-num"), temp == null ? "--" : (temp / 1000).toFixed(1));

    paintHeat("cpu", hero, $("#wave-clip"), temp, c.targetTemp);
    setText($("#ver"), t("version", { v: S.version }));
  }

  function renderDaemon() {
    if (!S) return;
    const sec = $("#daemon"), sw = $("#sw-daemon");
    const cls = "daemon" + (S.running ? "" : " off");
    if (sec.className !== cls) sec.className = cls;
    if (!daemonBusy && sw.checked !== S.running) sw.checked = S.running;
    renderCpuTake();
    setText($("#daemon-sub"), S.running ? t("daemon.running", { pid: S.pid ? " · PID " + S.pid : "" }) : t("daemon.stopped"));
    renderLocks();
  }

  const cpuLabel = (cpus) => {
    const n = cpus.split(/\s+/).filter(Boolean).map(Number).sort((a, b) => a - b);
    if (!n.length) return "CPU";
    return n.length === 1 ? `CPU ${n[0]}` : `CPU ${n[0]}–${n[n.length - 1]}`;
  };
  let freqSig = "";
  function renderFreq() {
    if (!S) return;
    const ul = $("#freq-list");
    const sig = S.policies.map((p) => p.id).join(",");
    if (sig !== freqSig) { // susunan cluster berubah (atau pertama kali): buat barisnya sekali
      freqSig = sig;
      ul.innerHTML = "";
      S.policies.forEach((p) => {
        const li = document.createElement("li");
        li.dataset.id = p.id;
        li.innerHTML = `<span class="freq-name"></span><span class="freq-val"></span>
          <div class="freq-bar"><div class="freq-fill"></div></div><span class="freq-sub"></span>`;
        setText($(".freq-name", li), cpuLabel(p.cpus));
        ul.appendChild(li);
      });
      if (!S.policies.length) ul.innerHTML = '<li class="help">' + t("cpu.nodriver") + "</li>";
    }
    S.policies.forEach((p) => {
      const li = $(`li[data-id="${p.id}"]`, ul);
      if (!li) return;
      const hw = p.hw || p.limit || 1;
      const limit = Math.min(p.limit || hw, hw);
      const capped = limit < hw * 0.995;
      const cls = "freq-item" + (capped ? " capped" : "");
      if (li.className !== cls) li.className = cls;
      setText($(".freq-val", li), ghz(limit) + " GHz");
      setVar($(".freq-fill", li), "--w", ((limit / hw) * 100).toFixed(1) + "%");
      const now = p.cur ? t("freq.now", { cur: ghz(p.cur) }) : "";
      setText($(".freq-sub", li), t(capped ? "freq.capped" : "freq.free", { max: ghz(hw), now }));
    });
  }


  /* ---------- data perangkat GPU (dump) ---------- */
  let dumping = false, dumpPath = "";
  async function dumpGpu() {   // binary prints the path of the saved file; null = failed or already running
    if (dumping) return null;
    dumping = true;
    const r = await bin("dump-gpu");
    dumping = false;
    const path = (r.stdout || "").trim().split("\n").pop();
    return r.errno === 0 && /^\//.test(path) ? path : null;
  }
  function renderDumpPath() {
    setText($("#dump-file"), dumpPath);
    setHidden($("#dump-path"), !dumpPath);
  }
  $("#btn-dump").addEventListener("click", async () => {
    const b = $("#btn-dump");
    b.disabled = true;
    const path = await dumpGpu();
    b.disabled = false;
    if (path) { dumpPath = path; renderDumpPath(); snack(t("dump.saved")); }
    else snack(t("dump.fail"), true);
  });
  // Tap the GPU temperature 7 times quickly (gaps <= 600 ms) to save the same data. Works any number of times.
  let taps = 0, lastTap = 0;
  $("#gpu-cookie").addEventListener("click", async () => {
    const now = Date.now();
    taps = now - lastTap <= 600 ? taps + 1 : 1;
    lastTap = now;
    if (taps < 7) return;
    taps = 0;
    const path = await dumpGpu();
    if (path) { dumpPath = path; renderDumpPath(); snack(t("dump.tap", { path }), false, 7000); }
    else snack(t("dump.fail"), true);
  });

  function renderGpu() {
    if (!S || !S.gpu) return;
    const g = S.gpu, c = S.config, hero = $("#gpu-hero");
    const sup = !!g.supported;
    setHidden($("#gpu-unsupported"), sup);
    setHidden($("#gpu-body"), !sup);
    if (!sup) { renderDumpPath(); return; }
    const on = c.gpu === "on", temp = g.temp, target = c.gpuTargetTemp;
    const pill = $("#gpu-pill");
    const cls = "pill " + (!g.available ? "off" : on && S.running ? "on" : "idle");
    if (pill.className !== cls) pill.className = cls;
    setText($("#gpu-pill-text"), t(!g.available ? "pill.gpu.missing" : !on ? "pill.gpu.system" : S.running ? "pill.gpu.limited" : "pill.gpu.stopped"));
    const ceil = g.ceiling != null ? g.ceiling : c.gpuMaxFreq;
    const cap = ceil ? `${mhz(ceil)} MHz` : t("nolimit");
    setText($("#gpu-hero-target"), t("target.gpu", { t: deg(target), cap }));
    const trigger = target - 1500;
    let note;
    if (!g.available) note = t("note.gpu.missing");
    else if (!on) note = t("note.gpu.off");
    else if (!S.running) note = t("note.gpu.stopped");
    else if (temp == null) note = t("note.gpu.nosensor");
    else if (temp > trigger) note = t("note.gpu.limiting");
    else note = t("note.gpu.margin", { d: deg(trigger - temp) });
    setText($("#gpu-hero-note"), note);
    setText($("#gpu-temp-num"), temp == null ? "--" : (temp / 1000).toFixed(1));
    paintHeat("gpu", hero, $("#gpu-wave-clip"), temp, target);

    // baris frekuensi
    const ul = $("#gpu-freq-list");
    if (!ul.firstChild) {
      ul.innerHTML = `<li class="freq-item"><span class="freq-name">${t("gpu.cooling.title")}</span><span class="freq-val"></span>
        <div class="freq-bar"><div class="freq-fill"></div></div><span class="freq-sub"></span></li>`;
    }
    const li = ul.firstChild, f = gpuFreqs(), top = f.length ? f[f.length - 1] : (g.limit || 1);
    const limit = Math.min(g.limit || top, top);
    const capped = limit < top * 0.995;
    const lc = "freq-item" + (capped ? " capped" : "");
    if (li.className !== lc) li.className = lc;
    setText($(".freq-val", li), g.limit ? `${mhz(limit)} MHz` : "—");
    setVar($(".freq-fill", li), "--w", ((limit / top) * 100).toFixed(1) + "%");
    const now = g.cur ? t("gfreq.now", { cur: mhz(g.cur) }) : "";
    setText($(".freq-sub", li), g.available ? (t(capped ? "gfreq.capped" : "gfreq.free", { max: mhz(top), now })) : "—");
    const cool = g.cooling;
    setText($("#gpu-cooling"), cool ? t("gpu.cooling", { name: cool.name, type: cool.type ? " (" + cool.type + ")" : "", state: cool.state == null ? "?" : cool.state, limiting: cool.state > 0 ? t("gpu.cooling.limiting") : "" }) : t("gpu.cooling.none"));
    ensureGpuSlider(false);
    renderGpuControls();
  }

  function renderNotice(fatal) {
    const n = $("#notice");
    if (fatal) setText($("#notice-text"), fatal);
    setHidden(n, !fatal);
    setText($("#foot"), t("foot"));
  }

  /* ---------- siklus data ---------- */
  function onDraft() {
    syncControls();
    renderSensor();
    renderGpuSensor();
    showSavebar(isDirty());
  }

  /* ---------- aliran data ----------
     Utama: satu proses `cpu_limiter watch` lewat ksu.spawn yang mengirim status tiap detik (tanpa membuat shell su
     baru tiap detik). Cadangan: polling `status` lewat ksu.exec kalau spawn tidak tersedia atau tidak menjawab. */
  let zonesCache = [];
  let quietUntil = 0, applyTimer = 0, latest = null;
  const quiet = (ms) => { quietUntil = Math.max(quietUntil, performance.now() + ms); };
  // Selama jari menyentuh/menggulir atau animasi buka-tutup berjalan, render ditunda sebentar supaya tidak berebut frame.
  ["pointerdown", "touchstart"].forEach((ev) => document.addEventListener(ev, () => quiet(700), { capture: true, passive: true }));
  ["scroll", "touchmove"].forEach((ev) => document.addEventListener(ev, () => quiet(250), { capture: true, passive: true }));

  // Sensor yang sesaat tidak membalas (mis. gpu2 sedang tidur): pakai nilai terakhir yang baik beberapa detik
  // supaya angka tidak berkedip menjadi "--".
  const lastGood = { cpu: [null, 0], gpu: [null, 0] };
  const GRACE_MS = 8000;
  function keepTemp(key, v) {
    const now = performance.now();
    if (v != null) { lastGood[key] = [v, now]; return v; }
    const [lv, at] = lastGood[key];
    return lv != null && now - at < GRACE_MS ? lv : null;
  }
  function ingest(next) {
    next.temp = keepTemp("cpu", next.temp);
    if (next.gpu) next.gpu.temp = keepTemp("gpu", next.gpu.temp);
    if (next.zones) {
      const old = new Map(zonesCache.map((z) => [z.zone, z]));
      zonesCache = next.zones.map((z) => (z.temp == null && old.has(z.zone) && old.get(z.zone).temp != null ? { ...z, temp: old.get(z.zone).temp } : z));
    }
    next.zones = zonesCache;
    if (next.gpu) { if (next.gpu.freqs) gpuFreqsCache = next.gpu.freqs; else next.gpu.freqs = gpuFreqsCache; }
    latest = next;
    scheduleApply();
  }
  function scheduleApply() {
    if (applyTimer || !latest || document.hidden || sheet.open || busy) return;
    const wait = quietUntil - performance.now();
    if (wait > 0) { applyTimer = setTimeout(() => { applyTimer = 0; scheduleApply(); }, wait + 30); return; }
    const next = latest; latest = null;
    apply(next);
  }

  function apply(next) {
    const first = !S;
    S = next;
    const server = fromConfig(S.config);
    if (first || (!isDirty() && JSON.stringify(server) !== JSON.stringify(base))) {
      base = server; draft = { ...server };
      first ? buildControls() : onDraft();
    }
    renderHero(); renderDaemon(); renderFreq(); renderGpu(); renderNotice(null);
    renderSensor(); renderGpuSensor();
    showSavebar(isDirty());
    hideSplash();
  }

  function fatal(r) {
    const why = (r.stderr || "").trim() || t("err.code", { n: r.errno });
    renderNotice(t("notice.nobin", { why }));
    setText($("#state-text"), t("pill.disconnected"));
    $("#state-pill").className = "pill off";
    hideSplash();
  }

  // Ambil status sekali lewat exec (polling cadangan, atau setelah aksi yang butuh angka terbaru).
  let fetching = false;
  async function fetchOnce(force) {
    if (fetching || busy) return;
    fetching = true;
    try {
      if (!force) while (performance.now() < quietUntil) await sleep(60); // jangan memulai proses saat animasi
      const r = await bin("status");
      if (r.errno !== 0) { fatal(r); return; }
      try { ingest(JSON.parse(r.stdout)); } catch (_) { renderNotice(t("notice.badreply")); }
    } finally { fetching = false; }
  }

  const makeStdio = () => ({ l: {}, on(e, f) { (this.l[e] = this.l[e] || []).push(f); }, emit(e, ...a) { (this.l[e] || []).forEach((f) => f(...a)); } });
  // Polling lewat exec selalu jalan sebagai jaring pengaman; dia hanya "tidur" selama aliran spawn benar-benar mengirim data.
  let spawnOk = !!(window.ksu && typeof ksu.spawn === "function");
  let child = null, streamFails = 0, lastStream = -1e9;
  const streamAlive = () => performance.now() - lastStream < 2500;

  function spawnWatch() {
    if (!spawnOk || child || document.hidden) return;
    const name = `__clmt_watch_${++seq}_${Date.now()}`;
    const c = { dead: false, got: false, buf: "", stdout: makeStdio(), stderr: makeStdio(), stdin: makeStdio(), l: {} };
    c.on = (e, f) => { (c.l[e] = c.l[e] || []).push(f); };
    c.emit = (e, ...a) => { (c.l[e] || []).forEach((f) => f(...a)); };
    child = c;
    const end = (failed) => {
      if (c.dead) return;
      c.dead = true; clearTimeout(c.watchdog); delete window[name];
      if (child === c) child = null;
      if (failed && ++streamFails >= 2) { spawnOk = false; return; }
      if (!failed) streamFails = 0;
      setTimeout(spawnWatch, 150);
    };
    c.stdout.on("data", (chunk) => {
      if (c.dead) return;
      c.buf += chunk;
      let i;
      while ((i = c.buf.indexOf("\n")) >= 0) {
        const line = c.buf.slice(0, i).trim(); c.buf = c.buf.slice(i + 1);
        if (!line) continue;
        try { ingest(JSON.parse(line)); c.got = true; lastStream = performance.now(); } catch (_) { /* potongan/baris rusak: lewati */ }
      }
    });
    c.on("exit", () => end(!c.got));
    c.on("error", () => end(true));
    window[name] = c;
    // Kalau 5 detik tidak ada data sama sekali, anggap spawn tidak berfungsi di manager ini.
    c.watchdog = setTimeout(() => { if (!c.got) { c.dead = true; delete window[name]; if (child === c) child = null; spawnOk = false; } }, 5000);
    try { ksu.spawn(BIN, JSON.stringify(["watch", "60"]), JSON.stringify({}), name); }
    catch (e) { end(true); }
  }

  let alive = true;
  async function pollLoop() {
    while (alive) {
      if (!document.hidden && !sheet.open && !streamAlive()) await fetchOnce(false);
      await sleep(1000);
    }
  }

  async function save() {
    const keys = changedKeys();
    if (!keys.length || busy) return;
    busy = true;
    const btn = $("#btn-save"); btn.disabled = true;
    const map = { target: "targetTemp", gpuTarget: "gpuTargetTemp", gpuMax: "gpuMaxFreq" };
    const args = keys.map((k) => `${map[k] || k}=${draft[k]}`);
    const mode = activeMode(draft);
    if (mode && S && S.mode !== mode && keys.includes("target")) args.push(`mode=${mode}`);
    const r = await bin(`set ${args.map((a) => `'${a}'`).join(" ")}`);
    if (r.errno !== 0) {
      busy = false; btn.disabled = false;
      snack((r.stderr || r.stdout || t("save.fail")).replace(/^error:\s*/i, "").trim(), true);
      return;
    }
    if (keys.includes("sensor")) await bin("restart");
    busy = false; btn.disabled = false;
    base = null; S = S && { ...S };
    snack(t(keys.includes("sensor") ? "saved.restart" : "saved"));
    const prev = draft;
    const r2 = await bin("status");
    try { S = JSON.parse(r2.stdout); } catch (_) { /* biarkan */ }
    base = S ? fromConfig(S.config) : prev;
    draft = { ...base };
    onDraft(); renderHero(); renderDaemon(); renderFreq(); renderGpu(); renderNotice(null);
  }

  async function daemonOp(kind) {
    if (daemonBusy) return;
    daemonBusy = true;
    const sw = $("#sw-daemon"), rb = $("#btn-restart");
    sw.disabled = true; rb.disabled = true;
    if (kind === "restart") rb.classList.add("spin");
    const r = await bin(kind);
    await sleep(kind === "stop" ? 200 : 600);
    daemonBusy = false;
    sw.disabled = false; rb.disabled = false; rb.classList.remove("spin");
    await fetchOnce(true);
    if (r.errno !== 0) snack((r.stderr || t("cmd.fail")).replace(/^error:\s*/i, "").trim(), true);
    else snack(t(kind === "stop" ? "daemon.stop.ok" : kind === "start" ? "daemon.start.ok" : "daemon.restart.ok"));
  }

  async function loadLog() {
    const body = $("#log-body");
    body.removeAttribute("data-i18n");
    const r = await bin("log 150");
    body.textContent = r.errno === 0 ? (r.stdout.trim() || t("log.empty")) : (r.stderr || t("log.fail"));
    body.scrollTop = body.scrollHeight;
  }

  $("#btn-save").addEventListener("click", save);
  $("#btn-discard").addEventListener("click", () => { draft = { ...base }; onDraft(); });
  $("#btn-restart").addEventListener("click", () => daemonOp("restart"));
  $("#sw-daemon").addEventListener("change", (e) => daemonOp(e.target.checked ? "start" : "stop"));
  const sheet = $("#sheet");
  let closing = 0;
  function closeSheet() {
    if (!sheet.open || sheet.classList.contains("closing")) return;
    if (REDUCED) { sheet.close(); return; }
    sheet.classList.add("closing");
    closing = setTimeout(() => { sheet.classList.remove("closing"); if (sheet.open) sheet.close(); }, 270);
  }
  $("#btn-log").addEventListener("click", () => { clearTimeout(closing); sheet.classList.remove("closing"); sheet.showModal(); loadLog(); });
  $("#log-close").addEventListener("click", closeSheet);
  $("#log-refresh").addEventListener("click", loadLog);
  sheet.addEventListener("click", (e) => { if (e.target === sheet) closeSheet(); });
  sheet.addEventListener("cancel", (e) => { e.preventDefault(); closeSheet(); });

  /* ---------- Language ---------- */
  const langBtn = $("#btn-lang"), langMenu = $("#lang-menu");
  function relang() {
    window.I18N.applyStatic();
    setText($("#lang-code"), window.I18N.lang.toUpperCase());
    setText($("#page-title"), t(TITLES[page]));
    $("#modes").innerHTML = "";
    if (draft) buildControls();
    if (S) { renderHero(); renderDaemon(); renderFreq(); renderGpu(); renderNotice(null); renderSensor(); renderGpuSensor(); }
  }
  function closeLang() { langMenu.hidden = true; langBtn.setAttribute("aria-expanded", "false"); }
  function openLang() {
    langMenu.innerHTML = "";
    window.I18N.codes().forEach((code) => {
      const li = document.createElement("li");
      li.setAttribute("role", "option"); li.tabIndex = 0; li.dataset.code = code;
      li.setAttribute("aria-selected", String(code === window.I18N.lang));
      li.innerHTML = `<span></span>${code === window.I18N.lang ? icon("check") : ""}`;
      $("span", li).textContent = window.LANGS[code]["lang.name"];
      const pick = () => { closeLang(); if (code !== window.I18N.lang) { window.I18N.setLang(code); relang(); } };
      li.addEventListener("click", pick);
      li.addEventListener("keydown", (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); pick(); } });
      langMenu.appendChild(li);
    });
    langMenu.hidden = false; langBtn.setAttribute("aria-expanded", "true");
  }
  langBtn.addEventListener("click", (e) => { e.stopPropagation(); langMenu.hidden ? openLang() : closeLang(); });
  document.addEventListener("click", (e) => { if (!langMenu.hidden && !langMenu.contains(e.target)) closeLang(); });
  document.addEventListener("keydown", (e) => { if (e.key === "Escape") closeLang(); });
  setText($("#lang-code"), window.I18N.lang.toUpperCase());
  setText($("#page-title"), t(TITLES[page]));

  // Awal: polling langsung jalan (data pertama cepat, tidak menunggu spawn); spawn dicoba bersamaan.
  pollLoop();
  spawnWatch();
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) return;
    fetchOnce(true);
    spawnWatch();
    scheduleApply();
  });
  sheet.addEventListener("close", () => { fetchOnce(true); scheduleApply(); });
})();
