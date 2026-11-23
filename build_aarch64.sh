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

# Cross-compile a static aarch64 binary (runs on Android) from an x86_64 Linux host.
# Needs: stable rustc, `pip install ziglang`, and rust-src (rustup component add rust-src,
# or a copy of rust-lang/rust's library/ under $(rustc --print sysroot)/lib/rustlib/src/rust/).
set -euo pipefail
cd "$(dirname "$0")"
ZIGCC=$(mktemp -d)/zigcc
cat > "$ZIGCC" <<'EOS'
#!/bin/sh
exec python3 -m ziglang cc -target aarch64-linux-musl "$@"
EOS
chmod +x "$ZIGCC"
RUSTC_BOOTSTRAP=1 \
RUSTFLAGS="-C linker=$ZIGCC -C link-self-contained=no -C target-feature=+crt-static -C relocation-model=static" \
cargo build --release -Zbuild-std=std,panic_abort --target aarch64-unknown-linux-musl
echo "built: target/aarch64-unknown-linux-musl/release/cpu_limiter"
