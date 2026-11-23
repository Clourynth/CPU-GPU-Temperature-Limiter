# Changelog

## 🎉 R.01 · First release

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

Big thanks to [**nakixii**](https://github.com/nakixii/Magisk_CPULimiter) for the original *Magisk_CPULimiter* and to [**ahmed-alnassif**](https://github.com/ahmed-alnassif/GPU-Unlocker) for *GPU-Unlocker*, the inspiration behind the GPU side. And thank you for being here at the start. If you try it, please share how it went by opening an issue. Every report helps! 💚

---

## 🎉 R.01 · Rilis pertama

**Selamat datang, dan terima kasih sudah mencoba rilis pertama CPU & GPU Temperature Limiter!** 🥳
Inilah awal dari semuanya: limiter kecil yang ramah, yang menjaga ponsel tetap adem dengan mengatur kecepatan CPU dan GPU selangkah demi selangkah.

### ✨ Isinya

**🧠 Limiter CPU**
- 🎛️ Empat mode siap pakai (Hemat daya, Seimbang, Performa, Cepat) plus mode kustom, masing-masing dengan suhu target sendiri
- ⚡ Penyetelan **Aggressive** untuk reaksi seketika, atau **Smooth** dengan jendela filter dan ambang lonjakan yang bisa diatur
- 🕹️ Opsi latensi rendah (cek suhu sekitar tiap 8 ms)
- 🌡️ Sensor suhu otomatis, atau pilih sendiri
- 🛡️ **Takeover thermal CPU** opsional di ponsel Xiaomi / Redmi / POCO, bisa dikembalikan sepenuhnya

**🎮 Limiter GPU**
- 🔒 Batasi kecepatan tertinggi GPU dan atur suhu targetnya
- 🧩 Dukungan **Mali**, dan dukungan **Adreno** (eksperimental)
- 🛡️ Takeover thermal GPU opsional untuk Mali di HyperOS
- 🔎 Deteksi cermat yang hanya bertindak jika yakin, dan memberi tahu dengan jelas bila GPU tidak didukung

**🌐 WebUI**
- 💎 Desain Material 3 Expressive dengan tema terang dan gelap
- 🇬🇧🇮🇩 Bahasa Inggris dan Indonesia, bisa diganti langsung
- 📈 Suhu langsung, kontrol daemon, dan penampil log
- 🔄 Perubahan berlaku sekitar satu detik, tanpa reboot
- 🧰 **Laporan perangkat** sekali ketuk untuk GPU yang tidak didukung, dan jalan pintas tersembunyi (ketuk suhu GPU 7 kali) untuk semua orang

**🛠️ Di balik layar**
- 🪶 Satu program Rust kecil, satu thread, hampir tidak memakai CPU
- 🔎 Pemeriksaan perangkat saat instal menunjukkan apa yang didukung ponsel Anda
- 🧯 Menghentikan atau mencopot modul mengembalikan semuanya

### 📱 Perlu diketahui

- ✅ Sudah diuji di **Poco X6 Pro** (Dimensity 8300, HyperOS)
- 🧪 Limiter GPU, takeover thermal, dan dukungan Adreno bersifat eksperimental, dan belum dicoba di perangkat lain

### 🙏 Terima kasih

Terima kasih besar untuk [**nakixii**](https://github.com/nakixii/Magisk_CPULimiter) atas *Magisk_CPULimiter* yang asli dan [**ahmed-alnassif**](https://github.com/ahmed-alnassif/GPU-Unlocker) atas *GPU-Unlocker*, inspirasi sisi GPU. Dan terima kasih sudah hadir sejak awal. Kalau Anda mencobanya, ceritakan hasilnya lewat issue. Setiap laporan sangat membantu! 💚
