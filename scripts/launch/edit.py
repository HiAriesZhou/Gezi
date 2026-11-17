#!/usr/bin/env python3
"""Cut actual, timestamped Kaku actions; crop private chrome, never redraw UI."""
import json
import pathlib
import subprocess

root=pathlib.Path(__file__).resolve().parents[2]
raw=root/'launch/raw/kaku.mov'
meta=json.loads((root/'launch/raw/provenance.json').read_text())
shots=meta['shots']
out=root/'launch/output'
out.mkdir(exist_ok=True)
# Real window crops: raw source at left, centered reader, outline at left.
# Each crop excludes the terminal tab bar. No product pixels are synthesized.
parts=[('raw',4,'crop=min(iw\\,2200):min(ih-180\\,1500):0:120'),
       ('reading',8,'crop=min(iw\\,2200):min(ih-180\\,1500):(iw-ow)/2:120'),
       ('search',7,'crop=min(iw\\,2200):min(ih-180\\,1500):(iw-ow)/2:120'),
       ('outline',8,'crop=min(iw\\,3000):min(ih-180\\,1500):0:120')]
filters=[]
for i,(name,duration,crop) in enumerate(parts):
    start=shots[name]+1
    filters.append(f'[0:v]trim=start={start}:duration={duration},setpts=PTS-STARTPTS,{crop},scale=1760:900:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2:color=0x202522,setsar=1,fps=30[v{i}]')
# Finish on the actual diagram in the reading view after the outline closes.
start=shots['outline']+11
filters.append(f'[0:v]trim=start={start}:duration=5,setpts=PTS-STARTPTS,crop=min(iw\\,2200):min(ih-180\\,1500):(iw-ow)/2:ih-oh-150,scale=1760:900:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2:color=0x202522,setsar=1,fps=30[v4]')
filters.append('[v0][v1][v2][v3][v4]concat=n=5:v=1:a=0[cut]')
# Branding lives outside the real UI area.
captions=[('Read the document beside the code.',0,12),('Search text. Jump between headings.',12,27),('Gezi | Markdown, at home in your terminal.',27,33)]
chain='[cut]'
for i,(text,start,end) in enumerate(captions):
    path=root/f'launch/raw/caption-{i}.txt';path.write_text(text)
    chain+=f"drawtext=textfile={path}:fontcolor=0xF5F1E8:fontsize=32:x=(w-tw)/2:y=30:enable='between(t,{start},{end})',"
path=root/'launch/raw/url.txt';path.write_text('gezi.arieszhou.com  |  macOS Apple Silicon  |  Read-only')
chain+=f'drawtext=textfile={path}:fontcolor=0xF5F1E8:fontsize=25:x=(w-tw)/2:y=1000[captioned]'
filters.append(chain)
filters.append('[1:v]scale=152:48[logo];[captioned][logo]overlay=55:24[v]')
subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-y','-i',str(raw),'-i',str(root/'assets/brand/logo-on-dark.png'),'-filter_complex',';'.join(filters),'-map','[v]','-an','-c:v','libx264','-crf','18','-pix_fmt','yuv420p','-movflags','+faststart',str(out/'gezi-launch-x.mp4')],check=True)
subprocess.run(['ffmpeg','-v','error','-y','-ss','7','-i',str(out/'gezi-launch-x.mp4'),'-t','10','-an','-c:v','libx264','-crf','18','-pix_fmt','yuv420p','-movflags','+faststart',str(out/'gezi-teaser-x.mp4')],check=True)
subprocess.run(['ffmpeg','-v','error','-y','-ss','9','-i',str(out/'gezi-launch-x.mp4'),'-frames:v','1',str(out/'gezi-poster.png')],check=True)
subprocess.run(['ffmpeg','-v','error','-y','-i',str(out/'gezi-launch-x.mp4'),'-vf','fps=1/4,scale=480:-1,tile=4x2','-frames:v','1',str(out/'contact-sheet.png')],check=True)
