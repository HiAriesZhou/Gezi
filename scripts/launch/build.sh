#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/../.."
for tool in ffmpeg ffprobe python3; do command -v "$tool" >/dev/null || { echo "Missing: $tool" >&2; exit 1; }; done
raw=launch/raw/kaku.mov
[[ -s "$raw" && -s launch/raw/provenance.json ]] || { echo 'Real Kaku recording and provenance required. Run capture.py first.' >&2; exit 1; }
python3 scripts/launch/edit.py
python3 scripts/launch/verify.py
