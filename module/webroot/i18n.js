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

/* Tiny i18n engine. Strings live in lang/<code>.js; English is the default and the fallback. */
(function () {
  "use strict";
  const DEFAULT = "en";
  let cur = DEFAULT;
  try { const s = localStorage.getItem("clmt.lang"); if (s && window.LANGS && window.LANGS[s]) cur = s; } catch (_) { /* ignore */ }

  function t(key, vars) {
    const L = window.LANGS;
    let s = (L[cur] && L[cur][key]) ?? L[DEFAULT][key] ?? key;
    if (vars) s = s.replace(/\{(\w+)\}/g, (m, k) => (vars[k] == null ? m : vars[k]));
    return s;
  }

  // data-i18n="key" -> textContent; data-i18n-attr="aria-label=key;placeholder=key" -> attributes.
  function applyStatic(root) {
    (root || document).querySelectorAll("[data-i18n]").forEach((el) => { const v = t(el.dataset.i18n); if (el.textContent !== v) el.textContent = v; });
    (root || document).querySelectorAll("[data-i18n-attr]").forEach((el) => {
      el.dataset.i18nAttr.split(";").forEach((pair) => { const [a, k] = pair.split("="); if (a && k) el.setAttribute(a.trim(), t(k.trim())); });
    });
    document.documentElement.lang = cur;
  }

  function setLang(code) {
    if (!window.LANGS[code]) return;
    cur = code;
    try { localStorage.setItem("clmt.lang", code); } catch (_) { /* ignore */ }
    applyStatic();
  }

  window.I18N = { t, applyStatic, setLang, get lang() { return cur; }, codes: () => Object.keys(window.LANGS) };
  applyStatic();
})();
