#!/usr/bin/env python3
"""Fail on missing, truncated, wrong-format or unexpected audio exports."""
import json
import pathlib
import subprocess
root = pathlib.Path(__file__).resolve().parents[2]
for name, low, high in [('gezi-launch-x.mp4', 25, 35), ('gezi-teaser-x.mp4', 8, 12)]:
    path = root / 'launch/output' / name
    result = json.loads(subprocess.check_output(['ffprobe', '-v', 'error', '-show_streams', '-show_format', '-of', 'json', str(path)]))
    streams = result['streams']
    assert len(streams) == 1 and streams[0]['codec_type'] == 'video', result
    v = streams[0]
    assert v['codec_name'] == 'h264' and v['pix_fmt'] == 'yuv420p', v
    assert (v['width'], v['height']) == (1920, 1080), v
    assert low <= float(result['format']['duration']) <= high, result
    assert v['avg_frame_rate'] == '30/1', v
    subprocess.run(['ffmpeg', '-v', 'error', '-i', str(path), '-f', 'null', '-'], check=True)
    print(path)
for name in ['gezi-poster.png', 'contact-sheet.png']:
    path = root / 'launch/output' / name
    assert path.stat().st_size > 1000, path
    print(path)
