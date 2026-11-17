#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/../.."
for tool in ffmpeg ffprobe python3; do command -v "$tool" >/dev/null || { echo "Missing: $tool" >&2; exit 1; }; done
raw=launch/raw/kaku.mov
[[ -s "$raw" && -s launch/raw/provenance.json ]] || { echo 'Real Kaku recording and provenance required. Run capture.py first.' >&2; exit 1; }
mkdir -p launch/output
# Reserve paper margins for captions; preserve the real window aspect ratio.
cat > launch/raw/caption-open.txt <<'CAPTION'
Read the document beside the code.
CAPTION
cat > launch/raw/caption-search.txt <<'CAPTION'
Search text. Jump between headings.
CAPTION
cat > launch/raw/caption-end.txt <<'CAPTION'
Gezi | Markdown, at home in your terminal.
gezi.arieszhou.com | macOS Apple Silicon | Read-only
CAPTION
filter="scale=1760:900:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2:color=0x202522,setsar=1,fps=30,drawtext=textfile=launch/raw/caption-open.txt:fontcolor=0xF5F1E8:fontsize=32:x=(w-tw)/2:y=36:enable='lt(t,12)',drawtext=textfile=launch/raw/caption-search.txt:fontcolor=0xF5F1E8:fontsize=32:x=(w-tw)/2:y=36:enable='between(t,12,26)',drawtext=textfile=launch/raw/caption-end.txt:fontcolor=0xF5F1E8:fontsize=26:x=(w-tw)/2:y=980:enable='gte(t,26)'"
ffmpeg -hide_banner -y -i "$raw" -t 32 -vf "$filter" -an -c:v libx264 -crf 18 -pix_fmt yuv420p -movflags +faststart launch/output/gezi-launch-x.mp4
ffmpeg -hide_banner -y -ss 10 -i "$raw" -t 10 -vf "$filter" -an -c:v libx264 -crf 18 -pix_fmt yuv420p -movflags +faststart launch/output/gezi-teaser-x.mp4
ffmpeg -hide_banner -y -ss 11 -i launch/output/gezi-launch-x.mp4 -frames:v 1 launch/output/gezi-poster.png
ffmpeg -hide_banner -y -i launch/output/gezi-launch-x.mp4 -vf 'fps=1/4,scale=480:-1,tile=4x2' -frames:v 1 launch/output/contact-sheet.png
python3 scripts/launch/verify.py
