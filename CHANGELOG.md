**Welcome, and thank you for trying the very first release of CPU & GPU Temperature Limiter!** 🥳
This is where it all starts: a small, friendly limiter that keeps your phone cool by adjusting CPU and GPU speed one careful step at a time.

### ✨ What's inside

**🧠 CPU limiter**
- 🎛️ Four ready-made modes (Power save, Balanced, Performance, Fast) plus a custom one, each with its own target temperature
- ⚡ **Aggressive** tuning for instant reaction, or **Smooth** tuning with an adjustable filter window and jump threshold
- 🕹️ Low latency option (checks the temperature about every 8 ms)
- 🌡️ Automatic temperature sensor choice, or pick one yourself
- 🛡️ Optional **CPU thermal takeover** on Xiaomi / Redmi / POCO phones, fully reversible

**🎮 GPU limiter**
- 🔒 Cap the GPU's top speed and set a target temperature
- 🧩 **Mali** support, and **Adreno** support (experimental)
- 🛡️ Optional GPU thermal takeover for Mali on HyperOS
- 🔎 Smart detection that only acts when it is sure, and says clearly when a GPU is not supported

**🌐 WebUI**
- 💎 Material 3 Expressive design with light and dark themes
- 🇬🇧🇮🇩 English and Indonesian, switchable on the fly
- 📈 Live temperatures, daemon controls and a log viewer
- 🔄 Changes apply in about a second, no reboot needed
- 🧰 One-tap **device report** for unsupported GPUs, and a hidden shortcut (tap the GPU temperature 7 times) for everyone else

**🛠️ Under the hood**
- 🪶 A single small Rust program, one thread, almost no CPU use
- 🔎 A device check during install shows what your phone supports
- 🧯 Stopping or uninstalling restores everything

### 📱 Good to know

- ✅ Tested on the **Poco X6 Pro** (Dimensity 8300, HyperOS)
- 🧪 The GPU limiter, thermal takeover and Adreno support are experimental, and have not been tried on other devices yet

### 🙏 Thanks

- **[nakixii](https://github.com/nakixii/Magisk_CPULimiter)** for the original *Magisk_CPULimiter*, the idea this module's CPU limiting is based on.
- **[ahmed-alnassif](https://github.com/ahmed-alnassif/GPU-Unlocker)** for *GPU-Unlocker*, the inspiration behind the GPU side.

Thank you for being here at the start. If you try it, please share how it went by opening an issue. Every report helps! 💚
