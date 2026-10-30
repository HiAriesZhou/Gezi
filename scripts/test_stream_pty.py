#!/usr/bin/env python3
"""Regress the probe's startup race with the child deliberately scheduled first."""

import argparse
import contextlib
import io
import json
import os
from pathlib import Path
import sys
import shlex
import tempfile
import time
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
import check_stream_pty as probe

BINARY = Path(__file__).resolve().parents[1] / "target/debug/tmd"


class StartupOrdering(unittest.TestCase):
    def test_debug_diagnostic_clears_active_area_before_writing(self):
        import pty
        import select
        import subprocess
        import termios
        import fcntl
        import struct
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
        child = subprocess.Popen([str(BINARY), "--from", "claude-json", "--debug",
                                  "--theme", "default-dark", "--width", "80"],
                                 stdin=subprocess.PIPE, stdout=slave, stderr=slave)
        os.close(slave)

        def frame(record):
            child.stdin.write(record)
            child.stdin.flush()
            received = bytearray()
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                if select.select([master], [], [], .05)[0]:
                    received.extend(os.read(master, 65536))
                    if b"\x1b[?2026l" in received:
                        # The debug case has two frames around stderr output.
                        if record.startswith(b"broken") and b"tmd: JSONL" not in received:
                            continue
                        return bytes(received)
            self.fail("no diagnostic/draw frame received")

        try:
            first = frame(b'{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":"Hello"}}}\n')
            self.assertIn(b"Hello\r\n", first)
            bad = frame(b"broken\n")
            self.assertIn(b"tmd: JSONL line 2", bad)
            self.assertIn(b"\x1b[1F", bad)
            self.assertLess(bad.index(b"\x1b[1F"), bad.index(b"tmd: JSONL"))
            frame(b'{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" world"}}}\n')
            child.stdin.close()
            child.wait(timeout=5)
            self.assertEqual(child.returncode, 0)
        finally:
            if child.poll() is None:
                child.kill()
                child.wait()
            if not child.stdin.closed:
                child.stdin.close()
            os.close(master)

    def test_producer_waits_for_resize_even_when_renderer_starts_late(self):
        with tempfile.TemporaryDirectory(prefix="tmd-pty-delayed-") as directory:
            wrapper = Path(directory) / "tmd-delayed"
            wrapper.write_text("#!/bin/sh\nsleep 1\nexec " + shlex.quote(str(BINARY)) + ' "$@"\n')
            wrapper.chmod(0o700)
            probe.check_case(wrapper, interrupt=False)

    def test_c_locale_does_not_reinterpret_the_utf8_producer_command(self):
        with patch.dict(os.environ, {"LANG": "C", "LC_ALL": "C"}):
            probe.check_case(BINARY, interrupt=False)

    def test_shell_cannot_change_the_baseline_before_it_is_saved(self):
        original = probe.pty.fork

        def child_first():
            result = original()
            if result[0] > 0:
                time.sleep(.3)
            return result

        receipts = io.StringIO()
        with patch.object(probe.pty, "fork", child_first), contextlib.redirect_stdout(receipts):
            probe.check_case(BINARY, interrupt=False)
            probe.check_case(BINARY, interrupt=True)
        results = [json.loads(line) for line in receipts.getvalue().splitlines()]
        self.assertEqual([r["exit"] for r in results], [0, 130])
        self.assertTrue(all(all(r["checks"].values()) for r in results))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path,
                        default=Path(__file__).resolve().parents[1] / "target/debug/tmd")
    args, remaining = parser.parse_known_args()
    BINARY = args.binary.resolve()
    if not BINARY.is_file():
        parser.error("build tmd first or pass its path with --binary")
    unittest.main(argv=[sys.argv[0], *remaining])
