#!/usr/bin/env python3
"""Image reader protocol tests with synthetic files and a local HTTP server."""
import argparse
import contextlib
import http.server
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time
import unittest

import test_reader_pty as reader

SVG = b'<svg xmlns="http://www.w3.org/2000/svg" width="160" height="480"><rect width="160" height="480" fill="red"/></svg>'
GRAPHICS = re.compile(rb"\x1b_G(.*?);(.*?)\x1b\\", re.S)

def serve(root, delay):
    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            with (root / "requests").open("a") as log:
                log.write(self.path + "\n")
            if delay:
                time.sleep(delay)
            self.send_response(200)
            self.send_header("Content-Length", str(len(SVG)))
            self.end_headers()
            try:
                self.wfile.write(SVG)
            except (BrokenPipeError, ConnectionResetError):
                pass
        def log_message(self, *_):
            pass
    server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
    (root / "port").write_text(str(server.server_port))
    server.serve_forever()


class ImagesTests(unittest.TestCase):
    def terminal(self, text, options=()):
        terminal = reader.Terminal(text=text, options=options, cols=100)
        self.addCleanup(terminal.close)
        return terminal

    def settled(self, terminal, seconds=.35):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if not terminal.read():
                break

    def placement(self, terminal):
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            placements = [header for header, _ in GRAPHICS.findall(terminal.data) if header.startswith(b"a=p,")]
            if placements:
                return placements[-1]
            terminal.read()
        self.fail("No complete image placement received")

    def visible_text(self, terminal, marker):
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            plain = re.sub(rb"\x1b\[[0-9;?]*[a-zA-Z]", b"", terminal.data)
            if marker.encode() in plain:
                return
            terminal.read()
        self.fail(f"No visible {marker!r}")

    def assert_restored(self, terminal):
        import termios
        self.assertEqual(termios.tcgetattr(terminal.master), terminal.before)
        self.assertNotIn(b"\x1b[?1049h", terminal.data)
        self.assertNotIn(b"\x1b[3J", terminal.data)
        commands = [header for header, _ in GRAPHICS.findall(terminal.data)]
        self.assertFalse(any(b"d=A" in header or b"d=a" in header for header in commands))

    def test_local_image_scroll_sidebar_help_search_resize_and_owned_cleanup(self):
        terminal = self.terminal("# Start\n\n![local](asset.svg)\n\n# End\n\n" + "body\n\n" * 35)
        Path(terminal.temp.name, "asset.svg").write_bytes(SVG)
        placement = self.placement(terminal)
        image_id = re.search(rb"i=(\d+)", placement)[1]
        self.settled(terminal)
        offset = len(terminal.data)
        terminal.frame(b"\x1b[B")
        self.settled(terminal)
        self.assertIn(b"a=p,", terminal.data[offset:])
        frame = terminal.frame(b"o")
        self.assertIn("标题目录".encode(), frame)
        terminal.frame(b"\t")
        frame = terminal.frame(b"?")
        self.assertIn("帮助".encode(), frame)
        self.assertNotIn(b"a=p,", frame)
        terminal.frame(b"\x1b")
        terminal.frame(b"/")
        terminal.frame("End".encode())
        terminal.frame(b"\r")
        terminal.frame(b"\x1b")
        offset = len(terminal.data)
        terminal.resize(60, 15)
        terminal.until(reader.END, offset)
        terminal.frame(b"\x1b")
        terminal.frame(b"\x1b[H")
        self.settled(terminal)
        terminal.frame(b"\x1b[F")
        self.assertEqual(terminal.finish(b"q"), 0)
        self.assertIn(b"a=d,d=I,i=" + image_id, terminal.data)
        self.assert_restored(terminal)

    @contextlib.contextmanager
    def server(self, delay=0):
        # Keep the PTY-forking parent single-threaded. The local server runs in
        # its own process, avoiding inherited Python locks in the reader child.
        with tempfile.TemporaryDirectory(prefix="tmd-image-http-") as temporary:
            root = Path(temporary)
            requests = root / "requests"
            requests.write_text("")
            process = subprocess.Popen([sys.executable, "-B", __file__, "--server", str(root), "--delay", str(delay)])
            try:
                deadline = time.monotonic() + 8
                while not (root / "port").exists():
                    if process.poll() is not None or time.monotonic() > deadline:
                        self.fail("local image server did not start")
                    time.sleep(.01)
                port = (root / "port").read_text()
                yield f"http://127.0.0.1:{port}/badge?v=2", requests
            finally:
                process.terminate()
                process.wait(timeout=3)

    def test_remote_default_loads_extensionless_svg_and_remote_off_makes_no_request(self):
        with self.server() as (url, requests):
            terminal = self.terminal(f"# Start\n\n<img src='{url}' width='120' alt='remote'>")
            self.placement(terminal)
            self.assertEqual(requests.read_text().splitlines(), ["/badge?v=2"])
            self.assertEqual(terminal.finish(b"q"), 0)
            self.assert_restored(terminal)
            requests.write_text("")
            terminal = self.terminal(f"![remote]({url})", ("--remote-images", "off"))
            terminal.until(reader.END)
            self.settled(terminal)
            self.assertEqual(requests.read_text(), "")
            self.assertNotIn(b"\x1b_G", terminal.data)
            self.assertEqual(terminal.finish(b"q"), 0)

    def test_images_off_has_no_io_and_missing_image_is_visible_failure(self):
        terminal = self.terminal("![missing](missing.png)", ("--images", "off"))
        terminal.until(reader.END)
        self.settled(terminal)
        self.assertNotIn(b"\x1b_G", terminal.data)
        self.assertNotIn("无法加载".encode(), terminal.data)
        self.assertEqual(terminal.finish(b"q"), 0)
        terminal = self.terminal("![missing](missing.png)")
        self.visible_text(terminal, "无法加载")
        self.assertEqual(terminal.finish(b"q"), 0)
        self.assert_restored(terminal)

    def test_slow_remote_does_not_block_navigation_or_exit(self):
        with self.server(delay=2) as (url, requests):
            terminal = self.terminal(f"# Start\n\n![slow]({url})\n\n" + "body\n\n" * 40)
            terminal.until(reader.END)
            deadline = time.monotonic() + 3
            while not requests.read_text() and time.monotonic() < deadline:
                terminal.read()
            self.assertTrue(requests.read_text())
            started = time.monotonic()
            frame = terminal.frame(b"\x1b[F")
            self.assertIn("末尾".encode(), frame)
            self.assertEqual(terminal.finish(b"q"), 0)
            self.assertLess(time.monotonic() - started, 1.0)
            self.assert_restored(terminal)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--server", type=Path)
    parser.add_argument("--delay", type=float, default=0)
    parser.add_argument("--binary", type=Path, default=reader.BINARY)
    args, remaining = parser.parse_known_args()
    if args.server:
        serve(args.server, args.delay)
        sys.exit(0)
    reader.BINARY = args.binary.resolve()
    unittest.main(argv=[__file__, *remaining])
