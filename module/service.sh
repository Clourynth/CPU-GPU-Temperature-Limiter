#!/system/bin/sh
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

# Runs once per boot (late_start service).
MODDIR=${0%/*}
BIN="$MODDIR/cpu_limiter"

wait_until_login() {
    # /data may not be decrypted/mounted early in boot
    while [ "$(getprop sys.boot_completed)" != "1" ]; do
        sleep 1
    done
    # wait until the user unlocks the screen (/sdcard is writable)
    test_file="/sdcard/Android/.CLMT_PERMISSION_TEST"
    true > "$test_file"
    while [ ! -f "$test_file" ]; do
        true > "$test_file"
        sleep 1
    done
    rm -f "$test_file"

    uperf="/data/adb/modules/uperf"
    if [ -d "$uperf" ] && [ ! -f "$uperf/disable" ]; then
        while [ -z "$(pidof uperf)" ]; do
            sleep 1
        done
    fi
}

if [ "$(getprop sys.boot_completed)" != "1" ]; then
    wait_until_login
    "$BIN" boot-tweaks
fi

"$BIN" detect-gpu >/dev/null 2>&1   # before the daemon writes anything to the GPU
"$BIN" prop 2>/dev/null
killall -15 cpu_limiter 2>/dev/null
nohup "$BIN" daemon > /dev/null 2>&1 &

# Scene (powercfg) integration: Scene calls `sh /data/powercfg.sh <mode>`
cp -af "$MODDIR/powercfg.json" /data/powercfg.json
printf '%s\n' "exec \"$BIN\" mode \"\$1\"" > /data/powercfg.sh
