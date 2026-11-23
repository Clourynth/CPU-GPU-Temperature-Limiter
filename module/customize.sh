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

# shellcheck shell=sh
# CPU & GPU Temperature Limiter - installer logic (KernelSU / KernelSU Next / Magisk). Module has no /system content,
# so the metamodule (Magic Mount-rs etc.) has nothing to mount for it.

CFG_DIR="/sdcard/Android/naki/clmt"
CFG_PATH="$CFG_DIR/clmt.conf"
LOG_PATH="$CFG_DIR/clmt.log"

if [ "$ARCH" != "arm64" ]; then
    abort "! Only arm64 is supported (ARCH=$ARCH)"
fi

# The original module (id: cpu_limiter) must not be active at the same time.
OLD="/data/adb/modules/cpu_limiter"
if [ -d "$OLD" ] && [ ! -f "$OLD/disable" ] && [ ! -f "$OLD/remove" ]; then
    abort "! The original CPU Limiter module is still active. Disable/remove it, reboot, then install again."
fi

# Must not be used together with Uperf (except the Limiter variant of Uperf).
UPERF_PATH="/data/adb/modules/uperf"
if [ -d "$UPERF_PATH" ] && [ ! -f "$UPERF_PATH/disable" ]; then
    if ! grep -q Limiter "$UPERF_PATH/module.prop"; then
        abort "! Must not be used together with Uperf!"
    fi
fi

ui_print "- Config       : $CFG_PATH"
ui_print "- Log          : $LOG_PATH"
ui_print "- Read the comments inside the config file!"

mkdir -p "$CFG_DIR"

set_perm_recursive "$MODPATH" 0 0 0755 0644
set_perm "$MODPATH/cpu_limiter" 0 0 0755
set_perm "$MODPATH/service.sh" 0 0 0755
set_perm "$MODPATH/uninstall.sh" 0 0 0755

# Create / migrate the config (old values are kept).
[ -f "$CFG_DIR/clmt.conf" ] && FIRST_INSTALL=0 || FIRST_INSTALL=1
if ! "$MODPATH/cpu_limiter" init-config; then
    ui_print "! Failed to create the config; it will be created again on the next install."
fi

# Mali / Adreno GPU: record the maximum frequency the system uses now (default safe limit).
# On a first install the detected safe clock also becomes the default GPU max frequency.
if [ "$FIRST_INSTALL" = 1 ]; then
    "$MODPATH/cpu_limiter" detect-gpu --first >/dev/null 2>&1
else
    "$MODPATH/cpu_limiter" detect-gpu >/dev/null 2>&1
fi

# Device summary (everything is detected right now, nothing is hardcoded).
ui_print ""
ui_print "- Detected device:"
"$MODPATH/cpu_limiter" info 2>/dev/null | while IFS= read -r line; do
    ui_print "  $line"
done
ui_print ""

# Module description: show the current daemon status + the "restart to apply" marker.
# Written to the new copy and to the currently installed module (if this is an update).
"$MODPATH/cpu_limiter" prop --pending "$MODPATH/module.prop" "/data/adb/modules/${MODID:-cpu_limiter_rs}/module.prop" 2>/dev/null
