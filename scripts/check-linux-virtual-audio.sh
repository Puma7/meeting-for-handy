#!/usr/bin/env bash
# Real PulseAudio check with virtual devices only. Starts its own private
# PulseAudio server (separate socket and state), so a desktop's running audio
# service, its defaults and physical devices are never touched. The actual
# `pactl` device listing and the actual `parec` capture path are exercised;
# this is not a physical microphone, PipeWire or long-session test.
set -euo pipefail
if [[ $# != 2 ]]; then
  echo 'Use: check-linux-virtual-audio.sh DEVICE_CAPTURE_EXAMPLE NEW_OUTPUT_DIRECTORY' >&2
  exit 2
fi
example=$(python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$1")
output=$(python3 -c 'import os,sys; print(os.path.abspath(sys.argv[1]))' "$2")
if [[ ! -x "$example" || -e "$output" ]]; then
  echo 'Expected the built device_capture example and a new output directory.' >&2
  exit 2
fi
for tool in pulseaudio pactl parec pacat python3; do command -v "$tool" >/dev/null; done
mkdir -p "$output"
private=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/handymeeting-pulse.XXXXXXXX")
mkdir -m 700 "$private/run" "$private/state"
export PULSE_RUNTIME_PATH="$private/run" PULSE_STATE_PATH="$private/state" PULSE_SERVER="unix:$private/run/native"
unset PULSE_SINK PULSE_SOURCE
server=''
players=()
cleanup() {
  # Stop only the processes this script started, then its private directory.
  for pid in "${players[@]}" $server; do kill "$pid" 2>/dev/null || true; done
  for pid in "${players[@]}" $server; do wait "$pid" 2>/dev/null || true; done
  rm -rf -- "$private"
}
trap cleanup EXIT
pulseaudio -n --daemonize=no --use-pid-file=no --exit-idle-time=-1 --log-target=stderr \
  --load="module-native-protocol-unix auth-anonymous=1 socket=$private/run/native" \
  2> "$output/pulseaudio.log" &
server=$!
for _ in $(seq 100); do pactl info >/dev/null 2>&1 && break; sleep 0.1; done
pactl info | tee "$output/pulseaudio-info.txt"
pactl --version | tee "$output/pactl-version.txt"
# System output: a null sink; its monitor is the system track.
pactl load-module module-null-sink sink_name=handymeeting_ci_output rate=16000 channels=1 sink_properties=device.description=HandyMeeting-CI-output >/dev/null
# Microphone: a separate null sink exposed as an ordinary (non-monitor) source.
pactl load-module module-null-sink sink_name=handymeeting_ci_microphone_feed rate=16000 channels=1 sink_properties=device.description=HandyMeeting-CI-microphone-feed >/dev/null
pactl load-module module-remap-source source_name=handymeeting_ci_microphone master=handymeeting_ci_microphone_feed.monitor source_properties=device.description=HandyMeeting-CI-microphone >/dev/null
pactl set-default-sink handymeeting_ci_output
pactl set-default-source handymeeting_ci_microphone
"$example" --list-devices > "$output/devices.json"
python3 - "$output/devices.json" <<'PY'
import json, sys
devices = json.load(open(sys.argv[1]))
print(json.dumps(devices, indent=2))
def find(source, device):
    return [d for d in devices if d["source"] == source and d["id"] == device]
system = find("system", "handymeeting_ci_output.monitor")
microphone = find("microphone", "handymeeting_ci_microphone")
assert len(system) == 1 and system[0]["isDefault"], "Default system output monitor is not listed"
assert len(microphone) == 1 and microphone[0]["isDefault"], "Default virtual microphone is not listed"
assert not [d for d in devices if d["source"] == "microphone" and d["id"].endswith(".monitor")], "A monitor was listed as a microphone"
print("Real pactl listing: system monitor and separate microphone found")
PY
# Two distinct tones, one per virtual device, so track separation is checkable.
python3 - "$output" <<'PY'
import math, struct, sys
for name, frequency in [("system", 440), ("microphone", 1000)]:
    with open(f"{sys.argv[1]}/{name}-tone.raw", "wb") as tone:
        tone.write(b"".join(struct.pack("<h", int(8000 * math.sin(2 * math.pi * frequency * i / 16000))) for i in range(16000 * 30)))
PY
pacat --playback --raw --format=s16le --rate=16000 --channels=1 --latency-msec=50 --device=handymeeting_ci_output "$output/system-tone.raw" &
players+=($!)
pacat --playback --raw --format=s16le --rate=16000 --channels=1 --latency-msec=50 --device=handymeeting_ci_microphone_feed "$output/microphone-tone.raw" &
players+=($!)
# Warm the owned playback streams before recording. Capture readiness can precede
# actual PCM, so request nine seconds within the unchanged8–10s receipt bounds.
# The verifier still requires eight seconds of actual tone after any declared
# startup prefix; no gap is reclassified as complete and no limit is relaxed.
sleep 3
"$example" "$output/capture" 9
python3 "$(dirname "$0")/check-linux-audio-result.py" "$output/capture" "$output/checks.json"
