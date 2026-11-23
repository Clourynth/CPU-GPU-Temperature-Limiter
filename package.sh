#!/bin/bash
# CPU & GPU Temperature Limiter
# Copyright (C) 2026  Clourynth
#
# Credits: nakixii (Magisk_CPULimiter) and ahmed-alnassif.
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.

# Build the flashable module zip (needs ./build_aarch64.sh to have run first).
set -euo pipefail
cd "$(dirname "$0")"
BIN=target/aarch64-unknown-linux-musl/release/cpu_limiter
VER=$(sed -n 's/^version=//p' module/module.prop)
OUT=${1:-CPU_GPU_Temperature_Limiter_${VER}.zip}
rm -f "$OUT"
python3 -I - "$BIN" "$OUT" <<'PY'
import sys, zipfile, os
binp, out = sys.argv[1:3]
files = {
  "module.prop":"module/module.prop","customize.sh":"module/customize.sh",
  "service.sh":"module/service.sh","uninstall.sh":"module/uninstall.sh",
  "powercfg.json":"module/powercfg.json","cpu_limiter":binp,"LICENSE":"LICENSE",
  "META-INF/com/google/android/update-binary":"module/META-INF/com/google/android/update-binary",
  "META-INF/com/google/android/updater-script":"module/META-INF/com/google/android/updater-script",
}
for root, _, fs in os.walk("module/webroot"):
    for f in sorted(fs):
        src = os.path.join(root, f)
        files[os.path.relpath(src, "module")] = src
with zipfile.ZipFile(out,"w",zipfile.ZIP_DEFLATED) as z:
    for name,src in files.items():
        zi = zipfile.ZipInfo(name); zi.compress_type = zipfile.ZIP_DEFLATED
        exe = name in ("cpu_limiter","service.sh","uninstall.sh","customize.sh") or name.endswith("update-binary")
        zi.external_attr = (0o100755 if exe else 0o100644) << 16
        zi.date_time = (2026,11,19,0,0,0)
        z.writestr(zi, open(src,"rb").read())
print("wrote", out)
PY
unzip -l "$OUT"
