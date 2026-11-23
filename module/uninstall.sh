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

"${0%/*}/cpu_limiter" revert-tweaks 2>/dev/null
rm -f /data/powercfg.json /data/powercfg.sh
rm -f /data/cache/clmt_mode
killall -15 cpu_limiter 2>/dev/null
