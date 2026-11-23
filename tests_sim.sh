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

# Host-side simulation of the daemon against a fake sysfs tree. Usage: tests_sim.sh <binary>
set -u
BIN=${1:-target/debug/cpu_limiter}
R=$(mktemp -d); export CLMT_ROOT=$R
export CLMT_MODPROP=$R/module.prop
printf "id=cpu_limiter_rs\nname=Test\nversion=R.02\ndescription=lama\n" > $CLMT_MODPROP
C=$R/sys/devices/system/cpu/cpufreq
mkdir -p $R/sdcard/Android $R/sys/class/thermal $R/system
echo "ro.product.brand=POCO" > $R/system/build.prop
mk() { mkdir -p $C/policy$1; echo "$2" > $C/policy$1/scaling_available_frequencies; echo 9999999 > $C/policy$1/scaling_max_freq; echo "$3" > $C/policy$1/related_cpus; }
mk 0 "2000000 1800000 1600000 1400000 1200000 1000000" "0 1 2 3"
mk 4 "2600000 2400000 2200000 2000000 1800000 1500000 1000000" "4 5 6"
mk 7 "3350000 3200000 3000000 2800000 2600000 2400000 2200000 2000000 1800000 1600000 1400000 1200000" "7"
for z in 0 1 2; do mkdir -p $R/sys/class/thermal/thermal_zone$z; done
echo battery > $R/sys/class/thermal/thermal_zone0/type; echo 30000 > $R/sys/class/thermal/thermal_zone0/temp
echo soc_max > $R/sys/class/thermal/thermal_zone1/type; echo 60000 > $R/sys/class/thermal/thermal_zone1/temp
echo gpu     > $R/sys/class/thermal/thermal_zone2/type; echo 99000 > $R/sys/class/thermal/thermal_zone2/temp
rd() { cat $C/policy$1/scaling_max_freq; }
fail=0; chk() { if [ "$2" = "$3" ]; then echo "PASS $1 ($2)"; else echo "FAIL $1: got '$2' want '$3'"; fail=1; fi; }

$BIN init-config; chk "config created (default balance 65C)" "$(grep -c '^targetTemp=65000' $R/sdcard/Android/naki/clmt/clmt.conf)" 1
chk "default smooth params" "$(grep -E '^smooth(Window|Spike)=' $R/sdcard/Android/naki/clmt/clmt.conf | tr '\n' ' ')" "smoothWindow=310 smoothSpike=5000 "
chk "default gpu target" "$(grep '^gpuTargetTemp=' $R/sdcard/Android/naki/clmt/clmt.conf)" "gpuTargetTemp=53000"
chk "default gpu ceiling stays dynamic (0)" "$(grep '^gpuMaxFreq=' $R/sdcard/Android/naki/clmt/clmt.conf)" "gpuMaxFreq=0"
# fixture: the rest of this script was written for these mode temperatures
$BIN set powersave=60000 balance=75000 performance=95000 fast=85000 >/dev/null
$BIN mode balance; chk "mode balance -> target" "$(grep '^targetTemp=' $R/sdcard/Android/naki/clmt/clmt.conf)" "targetTemp=75000"
chk "mode file" "$(cat $R/sdcard/Android/naki/clmt/clmt_mode)" balance
$BIN mode bogus >/dev/null; chk "unknown mode rc" "$?" 1
$BIN mode init; chk "mode init -> balance temp" "$(grep '^targetTemp=' $R/sdcard/Android/naki/clmt/clmt.conf)" "targetTemp=75000"

# --- CPU thermal takeover switch: apply, record, restore ---
CC=$R/sys/devices/system/cpu/cpu0/core_ctl; mkdir -p $CC; echo 1 > $CC/enable; chmod 644 $CC/enable
ST=$R/data/adb/cpu_limiter_rs_tweaks.state
$BIN set cpuTakeover=off >/dev/null; $BIN boot-tweaks
chk "takeover off: node untouched" "$(cat $CC/enable) $(stat -c %a $CC/enable)" "1 644"
$BIN set cpuTakeover=on >/dev/null; $BIN boot-tweaks
chk "takeover on: node forced to 0 and locked" "$(cat $CC/enable) $(stat -c %a $CC/enable)" "0 444"
[ -s $ST ] && echo "PASS state file written" || { echo "FAIL no state file"; fail=1; }
$BIN revert-tweaks
chk "revert: value and mode restored" "$(cat $CC/enable) $(stat -c %a $CC/enable)" "1 644"
[ ! -e $ST ] && echo "PASS state file removed" || { echo "FAIL state file left"; fail=1; }
$BIN daemon & PID=$!
sleep 0.7
chk "cold (60C<73.5C): top stays max" "$(rd 7)" 3350000
# live toggle: the daemon applies / restores without restart
sleep 1.3; chk "daemon applied takeover (default on)" "$(cat $CC/enable)" 0
$BIN set cpuTakeover=off >/dev/null; sleep 2.3
chk "daemon restored on switch off" "$(cat $CC/enable) $(stat -c %a $CC/enable)" "1 644"
$BIN set cpuTakeover=on >/dev/null; sleep 2.3
chk "daemon re-applied on switch on" "$(cat $CC/enable)" 0
$BIN stop >/dev/null; chk "stop restores the system thermal" "$(cat $CC/enable) $(stat -c %a $CC/enable)" "1 644"
$BIN start >/dev/null; sleep 2.5; chk "start re-applies takeover" "$(cat $CC/enable)" 0
echo 90000 > $R/sys/class/thermal/thermal_zone1/temp   # soc_max only; gpu zone must be ignored
sleep 3
echo "hot  -> p7=$(rd 7) p4=$(rd 4) p0=$(rd 0)"
chk "hot: top bottomed out" "$(rd 7)" 1200000
chk "hot: mid follows (smallest freq >= 1.2G is 1.5G)" "$(rd 4)" 1500000
chk "hot: little follows (ceil -> 1.2G)" "$(rd 0)" 1200000
echo 72000 > $R/sys/class/thermal/thermal_zone1/temp   # <= 73.5C: climb 1 step/tick
sleep 3
echo "cool -> p7=$(rd 7) p4=$(rd 4) p0=$(rd 0)"
chk "cool: top back to max" "$(rd 7)" 3350000
chk "cool: mid at its max" "$(rd 4)" 2600000
chk "cool: little at its max" "$(rd 0)" 2000000
echo 74000 > $R/sys/class/thermal/thermal_zone1/temp   # inside (target-1.5C, ...): descend again
sleep 0.4
echo "74C -> p7=$(rd 7)"
[ "$(rd 7)" -lt 3350000 ] && echo "PASS above target-1.5C descends" || { echo "FAIL no descend"; fail=1; }
# live config change picked up
$BIN mode performance; sleep 1.5
grep -q "targetTemp: 95000" $R/sdcard/Android/naki/clmt/clmt.log && echo "PASS config reload logged" || { echo "FAIL reload"; fail=1; }
echo 80000 > $R/sys/class/thermal/thermal_zone1/temp; sleep 3
chk "95C target, 80C: top at max" "$(rd 7)" 3350000

# ---------------- tuning / lowlatency ----------------
CONF=$R/sdcard/Android/naki/clmt/clmt.conf
Z=$R/sys/class/thermal/thermal_zone1/temp
setkv() { sed -i "/^$1=/d" $CONF; echo "$1=$2" >> $CONF; sleep 1.3; }
now_ms() { local t=${EPOCHREALTIME/./}; echo $((t/1000)); }
settle_max() { echo 60000 > $Z; for _ in $(seq 1 100); do read v < $C/policy7/scaling_max_freq; [ "$v" = 3350000 ] && return; sleep 0.05; done; }
sweep_ms() {   # hot step: time until policy7 reaches its lowest entry
  settle_max; echo 90000 > $Z; local t0; t0=$(now_ms)
  while [ $(( $(now_ms) - t0 )) -lt 3000 ]; do read v < $C/policy7/scaling_max_freq; [ "$v" = 1200000 ] && { echo $(( $(now_ms) - t0 )); return; }; done; echo 99999
}

climb_ms() {   # cold step (far below target): time from the lowest limit back to the top
  echo 90000 > $Z; for _ in $(seq 1 100); do read v < $C/policy7/scaling_max_freq; [ "$v" = 1200000 ] && break; sleep 0.02; done
  echo 60000 > $Z; local t0; t0=$(now_ms)
  while [ $(( $(now_ms) - t0 )) -lt 3000 ]; do read v < $C/policy7/scaling_max_freq; [ "$v" = 3350000 ] && { echo $(( $(now_ms) - t0 )); return; }; done; echo 99999
}
watch_min() {  # lowest policy7 limit seen during $1 seconds
  local end=$(( $(now_ms) + $1 )) min=99999999; while [ "$(now_ms)" -lt "$end" ]; do read v < $C/policy7/scaling_max_freq; [ "$v" -lt "$min" ] && min=$v; done; echo $min
}

$BIN mode balance; sleep 1.3            # target 75000 -> threshold 73.5C
setkv tuning raw; setkv lowlatency off
T_NORMAL=$(sweep_ms)
setkv lowlatency on
T_LOW=$(sweep_ms)
echo "sweep: normal=${T_NORMAL}ms lowlatency(raw)=${T_LOW}ms"
[ "$T_LOW" -lt $(( T_NORMAL * 6 / 10 )) ] && echo "PASS lowlatency sweeps clearly faster" || { echo "FAIL lowlatency not faster"; fail=1; }
grep -q "lowlatency: on" $R/sdcard/Android/naki/clmt/clmt.log && echo "PASS lowlatency logged" || { echo "FAIL lowlatency log"; fail=1; }

setkv tuning smooth
T_SMOOTH=$(sweep_ms)
echo "sweep: lowlatency+smooth=${T_SMOOTH}ms"
[ "$T_SMOOTH" -lt "$T_NORMAL" ] && echo "PASS smooth+lowlatency still faster than normal" || { echo "FAIL smooth+lowlat slow"; fail=1; }
grep -q "tuning: smooth" $R/sdcard/Android/naki/clmt/clmt.log && echo "PASS tuning smooth logged" || { echo "FAIL tuning log"; fail=1; }


setkv tuning raw; setkv lowlatency off
C_NORMAL=$(climb_ms)
setkv lowlatency on
C_LOW=$(climb_ms)
echo "cold climb (60C, 15C below target): normal=${C_NORMAL}ms lowlatency=${C_LOW}ms"
[ "$C_LOW" -lt $(( C_NORMAL * 6 / 10 )) ] && echo "PASS lowlatency is fast even far from target" || { echo "FAIL lowlatency gated by temperature"; fail=1; }

# single noisy sample (30 ms spike at 95C while really at 70C): raw reacts, smooth ignores it
spike() { settle_max; echo 70000 > $Z; sleep 0.3; ( echo 95000 > $Z; sleep 0.1; echo 70000 > $Z ) & watch_min 250; wait; }
setkv tuning raw
MIN_RAW=$(spike)
setkv tuning smooth; setkv smoothSpike 50000
MIN_SMOOTH=$(spike)
setkv smoothSpike 8000
MIN_BYPASS=$(spike)
echo "spike: raw min limit=$MIN_RAW smooth min limit=$MIN_SMOOTH bypass=$MIN_BYPASS"
[ "$MIN_RAW" -lt 3350000 ] && echo "PASS raw reacts to a noisy spike" || { echo "FAIL raw did not react (test not sensitive)"; fail=1; }
[ "$MIN_SMOOTH" -gt "$MIN_RAW" ] && echo "PASS smooth holds the clock after a noisy spike (bypass disabled)" || { echo "FAIL smooth did not hold ($MIN_SMOOTH vs raw $MIN_RAW)"; fail=1; }
[ "$MIN_BYPASS" -lt 3350000 ] && echo "PASS big jump (>=8C) bypasses the smooth filter" || { echo "FAIL bypass did not react ($MIN_BYPASS)"; fail=1; }
$BIN set smoothWindow=5 >/dev/null 2>&1 && { echo "FAIL smoothWindow=5 accepted"; fail=1; } || echo "PASS smoothWindow range validated"
$BIN set smoothWindow=2000 smoothSpike=0 >/dev/null && grep -q "^smoothWindow=2000" $R/sdcard/Android/naki/clmt/clmt.conf && echo "PASS smooth params written" || { echo "FAIL smooth params"; fail=1; }
setkv smoothWindow 500; setkv smoothSpike 8000

# aliases from the old naming
setkv tuning aggressive; setkv lowlatency off
grep -q "tuning: raw" $R/sdcard/Android/naki/clmt/clmt.log && echo "PASS alias aggressive -> raw" || { echo "FAIL alias"; fail=1; }

# ---------------- WebUI helpers: set / status / log / restart ----------------
$BIN set tuning=raw lowlatency=off balance=72000 >/dev/null; chk "set: rc" "$?" 0
sleep 1.3
grep -q "^balance=72000" $CONF && echo "PASS set writes value" || { echo "FAIL set value"; fail=1; }
$BIN set balance=1 2>/dev/null; chk "set: invalid rejected rc" "$?" 2
grep -q "^balance=72000" $CONF && echo "PASS invalid set leaves config untouched" || { echo "FAIL config changed by bad set"; fail=1; }
$BIN set tuning=smooth lowlatency=on >/dev/null; sleep 1.5
grep -q "tuning: smooth" $R/sdcard/Android/naki/clmt/clmt.log && grep -q "lowlatency: on" $R/sdcard/Android/naki/clmt/clmt.log && echo "PASS daemon hot-reloads values written by set" || { echo "FAIL set not hot-reloaded"; fail=1; }
ST=$($BIN status)
echo "$ST" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["running"] and d["pid"]; assert d["config"]["tuning"]=="smooth"; assert d["config"]["lowlatency"]=="on"; assert len(d["policies"])==3; assert d["temp"]>0; print("PASS status JSON ok")' || { echo "FAIL status"; fail=1; }
$BIN log 2 | wc -l | { read n; chk "log tail lines" "$n" 2; }
OLDPID=$PID
$BIN restart >/dev/null; sleep 0.6
NEWPID=$($BIN status | python3 -c 'import sys,json; print(json.load(sys.stdin)["pid"])')
[ -n "$NEWPID" ] && [ "$NEWPID" != "None" ] && [ "$NEWPID" != "$OLDPID" ] && echo "PASS restart replaced the daemon" || { echo "FAIL restart ($OLDPID -> $NEWPID)"; fail=1; }
PID=$NEWPID
chk "exactly one daemon after restart" "$(pgrep -fc "$BIN daemon")" 1

# ---------------- watch: streaming status for the WebUI ----------------
W=$($BIN watch 3 | python3 -c 'import sys,json; L=[json.loads(l) for l in sys.stdin]; print(len(L) >= 3, "zones" in L[0], "zones" in L[1], L[0]["running"])')
[ "$W" = "True True False True" ] && echo "PASS watch: 3 lines, zones only on the first" || { echo "FAIL watch: $W"; fail=1; }
S0=$(date +%s); $BIN watch 60 | head -1 >/dev/null; sleep 1.3
[ -z "$(pgrep -f "$BIN watch")" ] && echo "PASS watch exits when the reader closes the pipe" || { echo "FAIL watch kept running"; fail=1; pkill -f "$BIN watch"; }
# reader that never reads: must give up after ~3 s once the pipe is full
( $BIN watch 60 > >(sleep 12) ) & sleep 0.5; kill -0 $! 2>/dev/null && echo "PASS watch alive while pipe has room" ; pkill -f "$BIN watch"; wait 2>/dev/null

# ---------------- module.prop description follows the daemon ----------------
desc() { grep '^description=' $CLMT_MODPROP; }
sleep 1.3
desc | grep -q "^description=🟢 Daemon running" && echo "PASS prop: running while daemon is up" || { echo "FAIL prop running: $(desc)"; fail=1; }
$BIN mode fast >/dev/null; sleep 1.4
desc | grep -q "⚡ Fast 85°C" && echo "PASS prop follows mode change" || { echo "FAIL prop mode: $(desc)"; fail=1; }
$BIN set targetTemp=74500 >/dev/null; sleep 1.4
desc | grep -q "🎛️ Custom 74.5°C" && echo "PASS prop shows custom target" || { echo "FAIL prop custom: $(desc)"; fail=1; }
grep -q "^version=R.02" $CLMT_MODPROP && grep -q "^id=cpu_limiter_rs" $CLMT_MODPROP && echo "PASS prop other lines untouched" || { echo "FAIL prop other lines"; fail=1; }
$BIN prop --pending >/dev/null
desc | grep -q "⏱️ Restart to apply" && desc | grep -q "🟢 Daemon running" && echo "PASS prop --pending keeps current daemon status + timer" || { echo "FAIL prop pending: $(desc)"; fail=1; }
$BIN set balance=75000 >/dev/null; $BIN mode balance >/dev/null; sleep 1.4
desc | grep -q "⏱️" && echo "FAIL pending marker should clear when daemon refreshes" && fail=1 || echo "PASS pending marker cleared by running daemon"

# stop lifts the limits, start brings the daemon back
echo 90000 > $Z; sleep 1.2   # make sure the limiter is actually capping before stopping
[ "$(rd 7)" -lt 3350000 ] && echo "PASS limiter capping before stop" || { echo "FAIL not capping"; fail=1; }
$BIN stop >/dev/null; chk "stop rc" "$?" 0
chk "stop: no daemon left" "$(pgrep -fc "$BIN daemon")" 0
chk "stop lifts limit on policy7" "$(rd 7)" 9999000
chk "stop lifts limit on policy0" "$(rd 0)" 9999000
desc | grep -q "^description=🔴 Daemon stopped" && echo "PASS prop: stopped" || { echo "FAIL prop stopped: $(desc)"; fail=1; }
chk "status says stopped" "$($BIN status | python3 -c 'import sys,json; print(json.load(sys.stdin)["running"])')" False
$BIN stop >/dev/null; chk "stop twice is fine" "$?" 0
$BIN start >/dev/null; sleep 0.8
chk "start: one daemon" "$(pgrep -fc "$BIN daemon")" 1
desc | grep -q "^description=🟢 Daemon running" && echo "PASS prop: running again after start" || { echo "FAIL prop restart: $(desc)"; fail=1; }
$BIN start >/dev/null; sleep 0.3
chk "start twice keeps one daemon" "$(pgrep -fc "$BIN daemon")" 1
PID=$(pgrep -f "$BIN daemon" | head -1)

# ---------------- GPU ----------------
G=$R/sys/devices/platform/soc/13000000.mali/devfreq/13000000.mali
mkdir -p $G $R/sys/class/thermal/cooling_device2
echo "1400000000 900000000 700000000 601000000 463000000 265000000" > $G/available_frequencies
echo 601000000 > $G/max_freq; echo 300000000 > $G/cur_freq
mkdir -p $R/sys/class/devfreq $R/proc/device-tree; ln -s $G $R/sys/class/devfreq/13000000.mali
printf "mediatek,mt6897\0mediatek,mt6897-evb\0" > $R/proc/device-tree/compatible
echo devfreq-13000000.mali > $R/sys/class/thermal/cooling_device2/type; echo 3 > $R/sys/class/thermal/cooling_device2/cur_state
for z in 3 4; do mkdir -p $R/sys/class/thermal/thermal_zone$z; done
echo gpu1 > $R/sys/class/thermal/thermal_zone3/type; echo 40000 > $R/sys/class/thermal/thermal_zone3/temp
echo gpu2 > $R/sys/class/thermal/thermal_zone4/type; echo 39000 > $R/sys/class/thermal/thermal_zone4/temp
printf "ro.product.brand=POCO\nro.mi.os.version.name=OS1.0\n" > $R/system/build.prop   # POCO + HyperOS
for c in 0 4 7; do mkdir -p $R/sys/devices/system/cpu/cpu$c/regs/identification; done
echo 0x00000000410fd801 > $R/sys/devices/system/cpu/cpu0/regs/identification/midr_el1
echo 0x00000000410fd811 > $R/sys/devices/system/cpu/cpu4/regs/identification/midr_el1
GM() { cat $G/max_freq; }
INFO=$($BIN info)
chk "info: cluster core names" "$(echo "$INFO" | grep -c 'Cortex-A520\|Cortex-A720')" 2
chk "info: gpu supported line" "$(echo "$INFO" | grep -c 'GPU control: supported')" 1
chk "info: hyperos takeover available" "$(echo "$INFO" | grep -c 'available (HyperOS)')" 1
chk "status hyperos" "$($BIN status | python3 -c 'import sys,json; print(json.load(sys.stdin)["gpu"]["hyperos"])')" True
$BIN restart >/dev/null; sleep 1.5
chk "gpu default config off" "$(grep '^gpu=' $R/sdcard/Android/naki/clmt/clmt.conf)" "gpu=off"
chk "gpu default sensor auto" "$(grep '^gpuSensor=' $R/sdcard/Android/naki/clmt/clmt.conf)" "gpuSensor=auto"
chk "detect-gpu finds system max (601)" "$($BIN detect-gpu | head -1)" "detected 601000000"
chk "info: safe clock after detection" "$($BIN info | grep -c 'Safe clock : GPU 601 MHz')" 1
chk "gpuSafeMax stored" "$(grep '^gpuSafeMax=' $R/sdcard/Android/naki/clmt/clmt.conf)" "gpuSafeMax=601000000"
$BIN detect-gpu --first >/dev/null; chk "first install: gpuMaxFreq defaults to safe clock" "$(grep '^gpuMaxFreq=' $R/sdcard/Android/naki/clmt/clmt.conf)" "gpuMaxFreq=601000000"
$BIN set gpuMaxFreq=700000000 >/dev/null; $BIN detect-gpu --first >/dev/null; chk "first install does not override a chosen ceiling" "$(grep '^gpuMaxFreq=' $R/sdcard/Android/naki/clmt/clmt.conf)" "gpuMaxFreq=700000000"
$BIN set gpuMaxFreq=0 >/dev/null
echo 1400000000 > $G/max_freq
chk "detect-gpu keeps first detection" "$($BIN detect-gpu | head -1)" "kept 601000000"
chk "status gpuSafeMax + supported" "$($BIN status | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d["config"]["gpuSafeMax"], d["gpu"]["supported"])')" "601000000 True"
sleep 1.3; chk "gpu off: max_freq untouched" "$(GM)" 1400000000
$BIN set gpu=maybe 2>/dev/null; chk "gpu bad value rejected" "$?" 2
$BIN set gpuMaxFreq=123 2>/dev/null; chk "gpuMaxFreq implausible rejected" "$?" 2
$BIN set gpuTargetTemp=1000 2>/dev/null; chk "gpuTargetTemp out of range rejected" "$?" 2
$BIN set "gpuSensor=a b" 2>/dev/null; chk "gpuSensor bad chars rejected" "$?" 2
$BIN set gpu=on gpuMaxFreq=601000000 gpuTargetTemp=70000 gpuTakeover=on gpuSensor=gpu1,gpu2 >/dev/null; chk "gpu set rc" "$?" 0
sleep 2.8
chk "gpu cold: ceiling 601 MHz" "$(GM)" 601000000
chk "gpu takeover: cooling state forced 0" "$(cat $R/sys/class/thermal/cooling_device2/cur_state)" 0
echo 99000 > $R/sys/class/thermal/thermal_zone5/temp 2>/dev/null   # unrelated zone must not matter
echo 80000 > $R/sys/class/thermal/thermal_zone3/temp
sleep 1.2
chk "gpu hot: stepped to lowest OPP" "$(GM)" 265000000
echo 60000 > $R/sys/class/thermal/thermal_zone3/temp
sleep 1.2
chk "gpu cool: back to ceiling, not above" "$(GM)" 601000000
echo 5 > $R/sys/class/thermal/cooling_device2/cur_state
$BIN set gpuTakeover=off >/dev/null; sleep 2.5
chk "gpu takeover off: system cooling untouched" "$(cat $R/sys/class/thermal/cooling_device2/cur_state)" 5
echo 1400000000 > $G/max_freq; sleep 1.3
chk "gpu re-asserts ceiling if something changes it" "$(GM)" 601000000
$BIN set gpuMaxFreq=0 >/dev/null; sleep 1.5
chk "hyperos, no takeover, no ceiling: held at system max (601)" "$(GM)" 601000000
$BIN set gpuTakeover=on >/dev/null; sleep 1.5
chk "hyperos + takeover, no ceiling: top OPP" "$(GM)" 1400000000
chk "status gpu temp (max of gpu1/gpu2)" "$($BIN status | python3 -c 'import sys,json; print(json.load(sys.stdin)["gpu"]["temp"])')" 60000
chk "status gpu freqs" "$($BIN status | python3 -c 'import sys,json; d=json.load(sys.stdin)["gpu"]; print(len(d["freqs"]), d["cooling"]["name"], d["limit"])')" "6 cooling_device2 1400000000"
$BIN set gpuSensor=auto >/dev/null; sleep 12.5   # auto = every gpu* zone, incl. the 99C "gpu" zone
chk "gpu auto sensor includes all gpu* zones (hot -> lowest)" "$(GM)" 265000000
$BIN set gpuSensor=gpu1,gpu2 >/dev/null; $BIN set gpuMaxFreq=463000000 >/dev/null; sleep 1.5
chk "gpu ceiling 463" "$(GM)" 463000000
$BIN set gpu=off >/dev/null; sleep 1.5
chk "gpu off again: back to the system max (601)" "$(GM)" 601000000
$BIN set gpu=on >/dev/null; sleep 1.5
chk "gpu on again" "$(GM)" 463000000
# non-HyperOS ROM: takeover ignored (cooling untouched), no clamp to the system max
echo "ro.product.brand=POCO" > $R/system/build.prop   # not HyperOS
echo 5 > $R/sys/class/thermal/cooling_device2/cur_state
$BIN set gpu=on gpuTakeover=on gpuMaxFreq=0 >/dev/null; $BIN restart >/dev/null; sleep 2.8
chk "non-hyperos: takeover ignored" "$(cat $R/sys/class/thermal/cooling_device2/cur_state)" 5
chk "non-hyperos: no clamp (top OPP)" "$(GM)" 1400000000
chk "info: takeover unavailable" "$($BIN info | grep -c 'not available (not HyperOS)')" 1
printf "ro.product.brand=POCO\nro.mi.os.version.name=OS1.0\n" > $R/system/build.prop
# Mali on a non-MediaTek SoC is found the same way (no vendor check)
$BIN stop >/dev/null; $BIN detect-gpu --force | head -1 >/dev/null
printf "qcom,sm8650\0" > $R/proc/device-tree/compatible
chk "mali on any SoC: supported" "$($BIN status | python3 -c 'import sys,json; d=json.load(sys.stdin)["gpu"]; print(d["supported"], d["kind"])')" "True mali"
# cooling device: ambiguous candidates -> not picked, takeover not offered
mkdir -p $R/sys/class/thermal/cooling_device7; echo gpu-extra > $R/sys/class/thermal/cooling_device7/type; echo 0 > $R/sys/class/thermal/cooling_device7/cur_state
chk "exact devfreq-<node> cooling wins over other gpu devices" "$($BIN status | python3 -c 'import sys,json; print(json.load(sys.stdin)["gpu"]["cooling"]["name"])')" cooling_device2
echo other > $R/sys/class/thermal/cooling_device2/type; mkdir -p $R/sys/class/thermal/cooling_device8; echo gpu-b > $R/sys/class/thermal/cooling_device8/type; echo 0 > $R/sys/class/thermal/cooling_device8/cur_state
chk "ambiguous cooling devices: none chosen" "$($BIN status | python3 -c 'import sys,json; print(json.load(sys.stdin)["gpu"]["cooling"])')" None
chk "info: ambiguous cooling reported" "$($BIN info | grep -c 'cooling device is ambiguous')" 1
rm -rf $R/sys/class/thermal/cooling_device7 $R/sys/class/thermal/cooling_device8; echo devfreq-13000000.mali > $R/sys/class/thermal/cooling_device2/type
# No Mali, no Adreno: GPU part unsupported, nothing is written
mv $R/sys/class/devfreq/13000000.mali $R/mali.bak
chk "no gpu: status unsupported" "$($BIN status | python3 -c 'import sys,json; print(json.load(sys.stdin)["gpu"]["supported"])')" False
chk "no gpu: detect-gpu says unsupported" "$($BIN detect-gpu | head -1)" "no supported gpu"
chk "no gpu: info says not supported" "$($BIN info | grep -c 'this GPU is not supported')" 1
echo 1111000000 > $G/max_freq
$BIN start >/dev/null; sleep 2.5
chk "no gpu: daemon leaves max_freq alone" "$(GM)" 1111000000
$BIN stop >/dev/null
chk "no gpu: stop leaves max_freq alone" "$(GM)" 1111000000
# Adreno (kgsl): MHz knob and table
K=$R/sys/class/kgsl/kgsl-3d0; mkdir -p $K
echo "840 778 700 625 550 450 315" > $K/freq_table_mhz; echo 840 > $K/max_clock_mhz; echo 450 > $K/min_clock_mhz
chk "adreno: status kind" "$($BIN status | python3 -c 'import sys,json; d=json.load(sys.stdin)["gpu"]; print(d["supported"], d["kind"], d["limit"], d["takeoverPossible"])')" "True adreno 840000000 False"
chk "adreno: info line" "$($BIN info | grep -c 'GPU control: supported (Adreno)')" 1
chk "adreno: takeover not available" "$($BIN info | grep -c 'GPU thermal takeover: not available (Mali GPUs only)')" 1
$BIN set gpu=on gpuMaxFreq=700000000 gpuTargetTemp=70000 gpuTakeover=on gpuSensor=gpu1,gpu2 >/dev/null
$BIN detect-gpu --force >/dev/null   # safe clock of this GPU (nothing below the top: none)
echo 40000 > $R/sys/class/thermal/thermal_zone3/temp
$BIN start >/dev/null; sleep 2.8
chk "adreno: ceiling written in MHz" "$(cat $K/max_clock_mhz)" 700
chk "adreno: min clock untouched" "$(cat $K/min_clock_mhz)" 450
echo 80000 > $R/sys/class/thermal/thermal_zone3/temp; sleep 1.5
chk "adreno hot: lowest OPP" "$(cat $K/max_clock_mhz)" 315
echo 40000 > $R/sys/class/thermal/thermal_zone3/temp; sleep 1.5
chk "adreno cool: back to ceiling" "$(cat $K/max_clock_mhz)" 700
echo 5 > $R/sys/class/thermal/cooling_device2/cur_state
chk "adreno: cooling device never touched" "$(cat $R/sys/class/thermal/cooling_device2/cur_state)" 5
$BIN stop >/dev/null
chk "adreno stop: top clock restored" "$(cat $K/max_clock_mhz)" 840
rm -rf $R/sys/class/kgsl; mv $R/mali.bak $R/sys/class/devfreq/13000000.mali
$BIN set gpuMaxFreq=0 >/dev/null; echo 601000000 > $G/max_freq; $BIN detect-gpu --force >/dev/null
# gpu-dump: a read-only text snapshot, path printed
D=$($BIN dump-gpu); chk "dump-gpu prints the path" "$D" "/sdcard/Android/naki/clmt/gpu-dump.txt"
DF=$R/sdcard/Android/naki/clmt/gpu-dump.txt
chk "dump file written" "$(test -s $DF && echo yes || echo no)" yes
chk "dump has device props" "$(grep -c '^ro.product.brand: ' $DF)" 1
chk "dump has devfreq values" "$(grep -c 'available_frequencies: 1400000000' $DF)" 1
chk "dump has cooling type" "$(grep -c 'type=devfreq-13000000.mali' $DF)" 1
chk "dump has thermal zones" "$(grep -c '^thermal_zone3: type=gpu1' $DF)" 1
chk "dump has module detection" "$(grep -c 'GPU control: supported (Mali)' $DF)" 1
D2=$($BIN dump-gpu); chk "dump-gpu can run again" "$D2" "/sdcard/Android/naki/clmt/gpu-dump.txt"
# CPU takeover exists on Xiaomi / Redmi / POCO only
chk "status cpuTakeoverSupported (POCO)" "$($BIN status | python3 -c 'import sys,json; print(json.load(sys.stdin)["config"]["cpuTakeoverSupported"])')" True
echo "ro.product.brand=google" > $R/system/build.prop; echo "ro.product.manufacturer=Google" >> $R/system/build.prop
chk "non-xiaomi: cpuTakeoverSupported false" "$($BIN status | python3 -c 'import sys,json; print(json.load(sys.stdin)["config"]["cpuTakeoverSupported"])')" False
chk "non-xiaomi: info says not available" "$($BIN info | grep -c 'CPU thermal takeover: not available')" 1
$BIN set cpuTakeover=on >/dev/null; $BIN boot-tweaks
chk "non-xiaomi: takeover never applied" "$(test -e $ST && echo yes || echo no)" no
printf "ro.product.brand=POCO\nro.mi.os.version.name=OS1.0\n" > $R/system/build.prop
printf "mediatek,mt6897\0" > $R/proc/device-tree/compatible
echo 463000000 > $G/max_freq; $BIN start >/dev/null; sleep 2.5; $BIN stop >/dev/null
chk "stop restores the system max (601)" "$(GM)" 601000000
$BIN start >/dev/null; sleep 1.2
$BIN set gpu=off >/dev/null; sleep 1.3
PID=$(pgrep -f "$BIN daemon" | head -1)

kill $PID; wait $PID 2>/dev/null 2>&1
echo "--- log ---"; cat $R/sdcard/Android/naki/clmt/clmt.log
echo "perm: $(stat -c %a $C/policy7/scaling_max_freq)"
rm -rf $R
exit $fail
