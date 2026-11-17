#!/usr/bin/env python3
"""Record only a dedicated, sanitized Kaku window. Never capture the desktop."""
import argparse
import json
import pathlib
import shutil
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]

def run(*args, **kw):
    return subprocess.run(args, check=True, text=True, **kw)

def apple(source):
    return run('osascript', '-e', source, capture_output=True).stdout.strip()

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', required=True, type=pathlib.Path)
    args = p.parse_args()
    binary = args.binary.resolve(strict=True)
    for tool in ('swift', 'osascript', 'screencapture'):
        if not shutil.which(tool):
            raise SystemExit(f'Missing dependency: {tool}')
    access = run('swift', '-e', 'import CoreGraphics; print(CGPreflightScreenCaptureAccess())', capture_output=True).stdout.strip()
    if access != 'true':
        raise SystemExit('BLOCKED: enable Screen Recording for the host app in macOS Privacy & Security, then retry.')
    version = run(str(binary), '--version', capture_output=True).stdout.strip()
    raw = ROOT / 'launch/raw'
    raw.mkdir(parents=True, exist_ok=True)
    # A fresh shell with no profile, history, personal prompt or previous scrollback.
    import shlex
    command = 'exec env -i HOME=/tmp PATH=/usr/bin:/bin TERM=xterm-256color TERM_PROGRAM=Kaku /bin/zsh -f'
    apple('tell application "Kaku" to activate')
    apple('tell application "System Events" to tell process "Kaku" to keystroke "n" using command down')
    time.sleep(2)
    def keys(text):
        apple('tell application "System Events" to tell process "Kaku" to keystroke ' + json.dumps(text))
    def enter():
        apple('tell application "System Events" to tell process "Kaku" to key code 36')
    keys(command); enter(); time.sleep(1)
    keys("PROMPT='$ '; clear"); enter()
    apple('tell application "System Events" to tell process "Kaku" to set position of front window to {30, 50}')
    apple('tell application "System Events" to tell process "Kaku" to set size of front window to {1440, 900}')
    # Select the dedicated front Kaku window, never a display index.
    source = '''import CoreGraphics
import Foundation
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
for w in windows where w[kCGWindowOwnerName as String] as? String == "Kaku" && w[kCGWindowLayer as String] as? Int == 0 { print(w[kCGWindowNumber as String]!); break }
'''
    wid = run('swift', '-e', source, capture_output=True).stdout.strip()
    if not wid.isdigit():
        raise SystemExit('Cannot identify dedicated Kaku window')
    movie = raw / 'kaku.mov'
    if movie.exists():
        raise SystemExit('Move existing launch/raw/kaku.mov before recording again')
    record = subprocess.Popen(['screencapture', '-v', '-V', '32', '-l', wid, str(movie)])
    try:
        time.sleep(2)
        keys('cat ' + shlex.quote(str(ROOT / 'launch/fixtures/reading.md'))); enter()
        time.sleep(3)
        keys('clear; ' + shlex.quote(str(binary)) + ' --language en --theme default-dark ' + shlex.quote(str(ROOT / 'launch/fixtures/reading.md'))); enter()
        time.sleep(7)
        keys('/clarity'); enter(); time.sleep(5)
        apple('tell application "System Events" to tell process "Kaku" to key code 53')
        keys('o'); time.sleep(7)
        keys('o'); time.sleep(7)
    finally:
        record.wait(timeout=45)
    if not movie.exists() or movie.stat().st_size < 10000:
        raise SystemExit('Capture missing or empty')
    (raw / 'provenance.json').write_text(json.dumps({'binary_version': version, 'window_id': wid, 'source': 'real Kaku window; screencapture', 'fixture': 'launch/fixtures/reading.md'}, indent=2))
    print(movie)

if __name__ == '__main__':
    main()
