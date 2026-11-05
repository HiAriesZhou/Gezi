#!/usr/bin/env python3
"""Check terminal output, signal cleanup and the streaming probe's startup races."""

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


class StaticLargeHeadings(unittest.TestCase):
    def test_keyboard_ctrl_c_preserves_output_queue_and_restores_tty(self):
        import errno
        import fcntl
        import pty
        import select
        import signal
        import struct
        import termios

        with tempfile.TemporaryDirectory(prefix="tmd-heading-vintr-") as directory:
            path = Path(directory) / "long.md"
            path.write_text("# Heading\n\n" * 10000)
            ready_read, ready_write = os.pipe()
            pid, master = pty.fork()
            if pid == 0:
                os.close(ready_write)
                os.read(ready_read, 1)
                os.close(ready_read)
                env = dict(os.environ, TERM_PROGRAM="Kaku", TERM="kaku")
                for key in ("TMUX", "STY"):
                    env.pop(key, None)
                # Exercise the direct-output large-heading path. Interactive
                # stdin now selects the static reader, which has its own VINTR
                # and termios regression tests in test_reader_pty.py.
                null_input = os.open(os.devnull, os.O_RDONLY)
                os.dup2(null_input, 0)
                os.close(null_input)
                os.execve(str(BINARY), [str(BINARY), "--large-headings", "--theme",
                                      "default-dark", str(path)], env)
            os.close(ready_read)
            fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
            before = termios.tcgetattr(master)
            os.write(ready_write, b"1")
            os.close(ready_write)
            received = bytearray()
            during = None
            sent = False
            waited = False
            try:
                deadline = time.monotonic() + 15
                while time.monotonic() < deadline:
                    if not select.select([master], [], [], .05)[0]:
                        continue
                    try:
                        chunk = os.read(master, 32 if not sent else 65536)
                    except OSError as error:
                        if error.errno == errno.EIO:
                            break
                        raise
                    if not chunk:
                        break
                    received.extend(chunk)
                    if not sent and b"\x1b#3" in received:
                        during = termios.tcgetattr(master)
                        # Unlike send_signal, VINTR exercises the driver's
                        # queue flush and echo behaviour as an actual key does.
                        os.write(master, b"\x03")
                        sent = True
                else:
                    self.fail("keyboard Ctrl-C did not stop the renderer")
                _, status = os.waitpid(pid, 0)
                waited = True
                self.assertTrue(sent)
                self.assertEqual(os.waitstatus_to_exitcode(status), 130)
                self.assertTrue(during[3] & termios.NOFLSH, "VINTR must not discard half a heading")
                self.assertFalse(during[3] & (termios.ECHO | termios.ECHONL))
                self.assertEqual(during[3] & (termios.ICANON | termios.ISIG),
                                 before[3] & (termios.ICANON | termios.ISIG))
                self.assertNotIn(b"^C", received)
                self.assertTrue(received.endswith(b"\x1b#5"))
                self.assertEqual(termios.tcgetattr(master), before)
            finally:
                if not waited:
                    os.kill(pid, signal.SIGKILL)
                    os.waitpid(pid, 0)
                os.close(master)

    def render(self, text, options=(), *, cols=80, rows=24, env=None, interrupt=False,
               file_input=True, interrupt_signal=None, wait_before_read=False):
        import errno
        import fcntl
        import pty
        import select
        import signal
        import struct
        import subprocess
        import termios

        with tempfile.TemporaryDirectory(prefix="tmd-heading-") as directory:
            path = Path(directory) / "heading.md"
            path.write_text(text)
            master, slave = pty.openpty()
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
            original_attributes = termios.tcgetattr(master)
            child_env = dict(os.environ, TERM_PROGRAM="Kaku", TERM="kaku")
            for name in ("TMUX", "STY"):
                child_env.pop(name, None)
            child_env.update(env or {})
            child = subprocess.Popen([str(BINARY), "--theme", "default-dark",
                                      "--large-headings", *options,
                                      *([str(path)] if file_input else [])],
                                     stdin=subprocess.DEVNULL if file_input else subprocess.PIPE,
                                     stdout=slave, stderr=slave,
                                     env=child_env)
            # Keep the slave open until the parent drains the final bytes.
            # macOS can discard unread PTY output at the last slave close.
            if not file_input:
                child.stdin.write(text.encode())
                child.stdin.close()
            received = bytearray()
            sent = False
            deadline = time.monotonic() + 15
            try:
                if wait_before_read:
                    child.wait(timeout=5)
                while time.monotonic() < deadline:
                    if not select.select([master], [], [], .05)[0]:
                        if child.poll() is not None and not select.select([master], [], [], 0)[0]:
                            break
                        continue
                    try:
                        chunk = os.read(master, 65536)
                    except OSError as error:
                        if error.errno == errno.EIO:
                            break
                        raise
                    if not chunk:
                        break
                    received.extend(chunk)
                    if interrupt and not sent and b"\x1b#3" in received:
                        child.send_signal(interrupt_signal or signal.SIGINT)
                        sent = True
                else:
                    self.fail("static renderer did not exit")
                child.wait(timeout=5)
                self.assertEqual(child.returncode,
                                 128 + (interrupt_signal or signal.SIGINT) if interrupt else 0)
                if interrupt:
                    self.assertTrue(sent, "signal must follow actual double-height output")
                self.assertTrue(received, "must exercise real CLI output")
                self.assertEqual(termios.tcgetattr(master), original_attributes)
                return bytes(received)
            finally:
                if child.poll() is None:
                    child.kill()
                    child.wait()
                os.close(slave)
                os.close(master)

    def test_native_output_and_ctrl_c_cleanup(self):
        out = self.render("# H1\n\n## H2\n\nbody")
        self.assertEqual(out.count(b"\x1b#3"), 1)
        self.assertEqual(out.count(b"\x1b#4"), 1)
        self.assertTrue(out.endswith(b"\x1b#5"))
        interrupted = self.render(("# Heading\n\n" * 10000), interrupt=True)
        self.assertTrue(interrupted.endswith(b"\x1b#5"))

    def test_other_handled_signals_restore_terminal_settings(self):
        import signal
        for signum in (signal.SIGHUP, signal.SIGQUIT, signal.SIGTERM):
            with self.subTest(signal=signum):
                out = self.render("# Heading\n\n" * 10000, interrupt=True,
                                  interrupt_signal=signum)
                self.assertTrue(out.endswith(b"\x1b#5"))

    def test_fallbacks_do_not_duplicate_h1_or_emit_size_controls(self):
        for settings in [
            {"options": ["--ansi"]},
            {"env": {"TERM_PROGRAM": "unknown"}},
            {"env": {"TMUX": "session"}},
            {"env": {"STY": "session"}},
            {"env": {"TERM": "screen-256color"}},
            {"cols": 3},
            {"rows": 2},
            {"options": ["--width", "81"]},
            {"file_input": False},
        ]:
            with self.subTest(settings=settings):
                out = self.render("# X", **settings)
                self.assertNotIn(b"\x1b#", out)
                if settings.get("file_input", True):
                    self.assertEqual(out.count(b"X"), 1)
                else:
                    # Streamed active rows may be redrawn before commit.
                    self.assertIn(b"X", out)

    def test_static_output_survives_renderer_exiting_before_parent_reads(self):
        out = self.render("# X", options=["--ansi"], wait_before_read=True)
        self.assertEqual(out.count(b"X"), 1)

    def test_redirected_output_matches_the_normal_renderer(self):
        import subprocess
        with tempfile.TemporaryDirectory(prefix="tmd-heading-redirect-") as directory:
            path = Path(directory) / "heading.md"
            path.write_text("# 中文 title\n\nbody")
            command = [str(BINARY), "--theme", "default-dark", "--width", "20", str(path)]
            normal = subprocess.run(command, check=True, capture_output=True).stdout
            large = subprocess.run([*command, "--large-headings"], check=True,
                                   capture_output=True, env=dict(os.environ, TERM_PROGRAM="Kaku")).stdout
            self.assertTrue(normal)
            self.assertEqual(large, normal)
            self.assertNotIn(b"\x1b#", large)


class StreamingSignals(unittest.TestCase):
    def test_external_signals_restore_output_while_input_remains_open(self):
        import errno
        import fcntl
        import pty
        import select
        import signal
        import struct
        import subprocess
        import termios

        for input_format in ("text", "claude-json"):
            for signum in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP, signal.SIGQUIT):
                with self.subTest(input_format=input_format, signal=signum):
                    master, slave = pty.openpty()
                    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
                    before = termios.tcgetattr(master)
                    child = subprocess.Popen(
                        [str(BINARY), "--theme", "default-dark", "--from", input_format],
                        stdin=subprocess.PIPE, stdout=slave, stderr=slave,
                        env=dict(os.environ, TERM_PROGRAM="Kaku", TERM="kaku"))
                    received = bytearray()

                    def read_output():
                        if not select.select([master], [], [], .05)[0]:
                            if child.poll() is None or select.select([master], [], [], 0)[0]:
                                return True
                            return False
                        try:
                            chunk = os.read(master, 65536)
                        except OSError as error:
                            if error.errno == errno.EIO:
                                return False
                            raise
                        received.extend(chunk)
                        return bool(chunk)

                    try:
                        payload = b"# Signal test\n\nbody"
                        if input_format == "claude-json":
                            payload = (b'{"type":"stream_event","event":{"type":'
                                       b'"content_block_start","index":0,"content_block":'
                                       b'{"type":"text","text":"Signal test"}}}\n')
                        child.stdin.write(payload)
                        child.stdin.flush()
                        deadline = time.monotonic() + 5
                        while b"\x1b[?2026l" not in received:
                            self.assertLess(time.monotonic(), deadline, "no initial stream frame")
                            self.assertTrue(read_output(), "renderer exited before receiving a signal")
                        self.assertIn(b"\x1b[?25l", received)
                        self.assertIn(b"UnicodeVersion=push", received)
                        child.send_signal(signum)
                        deadline = time.monotonic() + 5
                        while read_output():
                            self.assertLess(time.monotonic(), deadline, "signal did not interrupt input wait")
                        child.wait(timeout=2)
                        self.assertEqual(child.returncode, 128 + signum)
                        self.assertGreater(received.rfind(b"\x1b[?25h"), received.rfind(b"\x1b[?25l"))
                        self.assertIn(b"UnicodeVersion=pop", received)
                        self.assertEqual(termios.tcgetattr(master), before)
                    finally:
                        if child.poll() is None:
                            child.kill()
                            child.wait()
                        child.stdin.close()
                        os.close(slave)
                        os.close(master)


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
            os.close(slave)
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
