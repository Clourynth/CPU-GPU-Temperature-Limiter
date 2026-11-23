<div align="center">

# 🌡️ CPU & GPU Temperature Limiter

**Keep your phone cool without guessing.** Set a target temperature, and the module quietly adjusts the CPU and GPU speed to stay under it, all from a clean, modern WebUI.

[![Version](https://img.shields.io/badge/version-R.01-00796B?style=for-the-badge)](CHANGELOG.md)
[![License](https://img.shields.io/badge/license-GPLv3-blue?style=for-the-badge)](LICENSE)
[![Android](https://img.shields.io/badge/Android-arm64-3DDC84?style=for-the-badge&logo=android&logoColor=white)](#-compatibility)
[![Rust](https://img.shields.io/badge/Rust-single_binary-DEA584?style=for-the-badge&logo=rust&logoColor=black)](#-how-it-works)
[![KernelSU](https://img.shields.io/badge/KernelSU-Next_%2B_WebUI-5C6BC0?style=for-the-badge)](#-install)
[![Languages](https://img.shields.io/badge/UI-English_%2B_Indonesia-F57C00?style=for-the-badge)](README.id.md)

[🇮🇩 Baca dalam Bahasa Indonesia](README.id.md)

</div>

<!--
  When the repository exists, live badges can be added here, for example:
  ![Downloads](https://img.shields.io/github/downloads/OWNER/REPO/total?style=for-the-badge)
  ![Stars](https://img.shields.io/github/stars/OWNER/REPO?style=for-the-badge)
-->

## 📸 Screenshots

<!-- Screenshots go here (put the image files in docs/screenshots/) -->

## 👋 What is this?

Phones slow themselves down when they get hot, but they often do it late and in big jumps. This module watches the temperature **many times per second** and lowers (or raises) the maximum CPU and GPU speed one small step at a time, so the phone stays smooth and stays cool.

You pick how warm you are happy with. The module does the rest.

## ✨ Highlights

| | |
|---|---|
| 🎛️ **Four ready-made modes** | Power save, Balanced, Performance and Fast, each with its own target temperature. Or set your own. |
| ⚡ **Two tuning styles** | **Aggressive** reacts instantly. **Smooth** avoids sudden jumps in speed, with adjustable settings. |
| 🕹️ **Low latency option** | Checks the temperature every ~8 ms for the quickest reaction. |
| 🎮 **GPU limiter** | Cap the GPU's top speed and set a target temperature for it too. Supports **Mali** and (experimental) **Adreno**. |
| 🛡️ **Thermal takeover** | Optionally turn off the system's own throttling so only your limits apply. Fully reversible. |
| 🌐 **Modern WebUI** | Material 3 Expressive design, live temperatures, a log viewer, light and dark themes, **English and Indonesian**. |
| 🔄 **No reboot for changes** | Settings apply within about a second. |
| 🪶 **Tiny and light** | One small Rust program, one thread, almost no CPU use. |
| 🔎 **Checks your device at install** | Tells you right away what is supported on your phone and what is not. |
| 🧰 **Easy bug reports** | One tap saves a device report you can attach to an issue (see [Reporting a problem](#-reporting-a-problem)). |

## 🧠 How it works

Think of the CPU's speeds as a **staircase**, with the fastest speed at the top.

1. The module reads the temperature very often.
2. If it is **warmer than your target**, it moves **one step down** the staircase (a lower maximum speed).
3. If it is **cooler**, it moves **one step up** (more room to speed up).
4. A small buffer of 1.5 °C stops it from flip-flopping around the target.

The GPU follows the same simple idea. Because every move is a single step, the result is easy to predict and easy to understand.

## 🚀 Install

1. Install **KernelSU Next** (or another root manager with WebUI support).
2. Download the latest `.zip` from the [Releases](../../releases) page.
3. Open your root manager → **Modules** → **Install from storage** → pick the zip.
4. Read the device summary shown at the end of the install.
5. **Reboot**, then open the module's **WebUI** from the Modules list.

> If you still have the original *CPU Limiter* module installed, disable or remove it first (then reboot). The installer will tell you if it finds it.

## 🎛️ Using the WebUI

- **Daemon card (top):** turn the limiter on or off, or restart it. When it is off, the settings below are greyed out and the system manages heat by itself.
- **CPU page:** choose a mode, edit each mode's target temperature, pick Aggressive or Smooth tuning, switch Low latency on, choose the temperature sensor, and (on supported phones) take over the CPU thermal system.
- **GPU page:** turn the GPU limiter on, set the highest GPU speed you allow, set a target temperature and choose the GPU sensor.
- **Log button:** see what the module has been doing. **EN/ID button:** change the language.

Your settings live in `/sdcard/Android/naki/clmt/clmt.conf` and are fully documented inside that file.

## 📱 Compatibility

| | Status |
|---|---|
| **Poco X6 Pro** (Dimensity 8300, HyperOS) | ✅ Tested |
| Other phones with a **Mali** GPU | 🟡 Should work, not tested |
| Phones with an **Adreno** GPU (Snapdragon) | 🧪 Experimental, not tested on real hardware |
| CPU thermal takeover | Only on **Xiaomi / Redmi / POCO** phones |
| GPU thermal takeover | Only on **Mali** GPUs running **HyperOS** |
| CPU architecture | arm64 only |

If your GPU is not recognised, the GPU page says so and offers a button that saves a report, so support can be added later.

## ⚠️ Good to know

- This is the **first release**. It has been tested on **one device** so far.
- The **GPU limiter**, **thermal takeover** and **Adreno support** are experimental. Try them with care and tell us how it goes.
- Lowering the maximum speed means less performance when the phone is hot. That is the whole point, but pick targets you are comfortable with.

## 🧯 Safe and reversible

- Stopping the daemon from the WebUI puts everything back the way the system had it.
- Uninstalling the module cleans up after itself.
- Nothing is sent anywhere. The module works fully offline.

## 🐞 Reporting a problem

Please open an issue and include the file **`/sdcard/Android/naki/clmt/gpu-dump.txt`**. It contains only technical GPU and thermal information (no serial numbers, accounts or network data). Open it first to check what is inside. You can create it two ways:

- On an unsupported GPU, press **Save device data** on the GPU page.
- On any phone, open the GPU page and **tap the GPU temperature 7 times quickly**. A message shows where the file was saved. This also helps when the GPU is detected but something does not work as expected.

## 🙏 Credits

- **[nakixii](https://github.com/nakixii/Magisk_CPULimiter)** for the original *Magisk_CPULimiter*, the idea this module's CPU limiting is based on.
- **[ahmed-alnassif](https://github.com/ahmed-alnassif)** for the inspiration behind the GPU side.
- **Roboto Flex** font by Google, used under the SIL Open Font License (see `module/webroot/fonts/`).

Everything here was written from scratch in Rust, studying how the original works rather than copying it.

## 📜 License

Released under the **GNU General Public License v3.0 or later**. See [LICENSE](LICENSE).
Copyright © 2026 Clourynth.
