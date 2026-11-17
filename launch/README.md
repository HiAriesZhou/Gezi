# Launch media

Use the published binary, not an unreleased build, for public recordings.

```sh
python3 scripts/launch/capture.py --binary /absolute/path/to/gezi
bash scripts/launch/build.sh
```

Requires macOS, Kaku, Swift, Accessibility/Automation permission for the host,
Screen Recording permission, FFmpeg with libx264 and ffprobe. No global terminal
preferences are changed. A dedicated window is created; leave other windows alone.
Only that window is recorded. Inspect it before sharing; never share the desktop.
The fresh shell has no profiles or history. Close the demo window after review.

The capture stops before recording if Screen Recording is unavailable. The raw
movie and provenance are private and ignored. Exports are silent H.264 1080p30:
32-second launch, 10-second teaser, actual UI poster, and a review contact sheet.
The build refuses absent capture/provenance and verifies formats and full decode.
Inspect the contact sheet and play both videos before uploading. Native graphics
claims require visual confirmation in the actual clip. No simulated UI is used.

Generated footage, private storyboards and reports stay out of commits. Social
copy is under content/. Website video embedding should follow only after the
actual clip passes review and a hosting destination is approved; no broken media
placeholder is shipped while OS capture permission is pending.
