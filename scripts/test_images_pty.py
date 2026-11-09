#!/usr/bin/env python3
"""Image reader protocol tests with synthetic files and a local HTTP server."""
import argparse
import contextlib
from collections import Counter
import fcntl
import faulthandler
import http.server
import os
from pathlib import Path
import re
import subprocess
import signal
import socket
import socketserver
import struct
import sys
import tempfile
import termios
import time
import unittest
from unittest.mock import patch

import test_reader_pty as reader

SVG = b'<svg xmlns="http://www.w3.org/2000/svg" width="160" height="480"><rect width="160" height="480" fill="red"/></svg>'
GRAPHICS = re.compile(rb"\x1b_G(.*?);(.*?)\x1b\\", re.S)

def http_server(handler):
    # HTTPServer.server_bind does a reverse DNS lookup for an unused server
    # name. The synthetic server must not depend on the runner's DNS service.
    return socketserver.TCPServer(("127.0.0.1", 0), handler)

def serve(root, delay, socks=False):
    if socks:
        class SocksHandler(socketserver.StreamRequestHandler):
            def handle(self):
                self.request.settimeout(3)
                version, count = self.rfile.read(2)
                assert version == 5
                self.rfile.read(count)
                self.wfile.write(b"\x05\x00")
                self.wfile.flush()
                header = self.rfile.read(4)
                assert header == b"\x05\x01\x00\x03"
                domain = self.rfile.read(self.rfile.read(1)[0])
                self.rfile.read(2)
                self.wfile.write(b"\x05\x00\x00\x01\x7f\x00\x00\x01\x00\x50")
                self.wfile.flush()
                with (root / "requests").open("ab") as log:
                    log.write(domain + b"\n")
                for _ in range(64):
                    line = self.rfile.readline(8192)
                    if line == b"\r\n":
                        break
                    if not line:
                        return
                else:
                    return
                self.wfile.write(b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: "
                                 + str(len(SVG)).encode() + b"\r\n\r\n" + SVG)
        server = socketserver.TCPServer(("127.0.0.1", 0), SocksHandler)
        (root / "port").write_text(str(server.server_address[1]))
        faulthandler.cancel_dump_traceback_later()
        server.serve_forever()
        return
    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            with (root / "requests").open("a") as log:
                log.write(self.path + "\n")
            if delay:
                time.sleep(delay)
            payload = SVG.replace(b'width="160" height="480"', b'width="2048" height="2048"') if self.path.startswith("/large") else SVG
            self.send_response(200)
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            try:
                self.wfile.write(payload)
            except (BrokenPipeError, ConnectionResetError):
                pass
        def log_message(self, *_):
            pass
    server = http_server(Handler)
    (root / "port").write_text(str(server.server_address[1]))
    faulthandler.cancel_dump_traceback_later()
    server.serve_forever()


class ImagesTests(unittest.TestCase):
    def test_http_fixture_startup_does_not_resolve_hostnames(self):
        with patch.object(socket, "getfqdn", side_effect=AssertionError("unexpected DNS lookup")):
            with http_server(http.server.BaseHTTPRequestHandler) as server:
                self.assertGreater(server.server_address[1], 0)

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
    def server(self, delay=0, socks=False):
        # Keep the PTY-forking parent single-threaded. The local server runs in
        # its own process, avoiding inherited Python locks in the reader child.
        with tempfile.TemporaryDirectory(prefix="tmd-image-http-") as temporary:
            root = Path(temporary)
            requests = root / "requests"
            requests.write_text("")
            command = [sys.executable, "-B", __file__, "--server", str(root), "--delay", str(delay)]
            if socks:
                command.append("--socks")
            process = subprocess.Popen(command)
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

    def test_pixel_only_resize_reflows_and_keeps_images_within_body(self):
        terminal = self.terminal("![image](asset.svg)\n\nTAIL-MARKER\n\n" + "body\n\n" * 25)
        Path(terminal.temp.name, "asset.svg").write_bytes(SVG)
        fcntl.ioctl(terminal.master, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 100, 800, 384))
        self.placement(terminal)
        self.settled(terminal)
        terminal.data.clear()
        fcntl.ioctl(terminal.master, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 100, 400, 768))
        os.kill(terminal.pid, signal.SIGWINCH)
        self.visible_text(terminal, "TAIL-MARKER")
        placement = self.placement(terminal)
        self.assertLessEqual(int(re.search(rb"w=(\d+)", placement)[1]), 400)
        self.assertEqual(terminal.finish(b"q"), 0)
        self.assert_restored(terminal)

    def test_evicted_prefetch_does_not_refetch_while_idle(self):
        with self.server() as (url, requests):
            base = url.split("/badge")[0]
            text = "\n\n".join(f'<img src="{base}/large/{i}" width="192">' for i in range(4))
            terminal = self.terminal(text + "\n\n" + "body\n\n" * 30)
            self.placement(terminal)
            self.settled(terminal, 4)
            before = Counter(requests.read_text().splitlines())
            self.assertEqual(set(before), {f"/large/{i}" for i in range(4)})
            self.settled(terminal, 1)
            after = Counter(requests.read_text().splitlines())
            self.assertEqual(before, after)
            self.assertTrue(all(count == 1 for count in after.values()))
            self.assertEqual(terminal.finish(b"q"), 0)
            self.assert_restored(terminal)

    def test_deep_svg_fails_without_aborting_or_leaving_terminal_raw(self):
        with tempfile.TemporaryDirectory(prefix="tmd-deep-svg-") as directory:
            path = Path(directory) / "deep.svg"
            path.write_text('<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">'
                            + '<g opacity=".5">' * 400 + '<rect width="10" height="10"/>'
                            + '</g>' * 400 + '</svg>')
            terminal = self.terminal(f"![deep]({path})")
            self.visible_text(terminal, "无法加载")
            self.assertEqual(terminal.finish(b"q"), 0)
            self.assertNotIn(b"stack overflow", terminal.data)
            self.assert_restored(terminal)

    def test_remote_images_use_configured_socks_proxy(self):
        with self.server(socks=True) as (url, requests):
            proxy = url.split("/badge")[0].replace("http://", "socks5h://")
            environment = {key: proxy for key in ("ALL_PROXY", "all_proxy", "HTTP_PROXY", "http_proxy", "HTTPS_PROXY", "https_proxy")}
            environment.update(NO_PROXY="", no_proxy="")
            with patch.dict(os.environ, environment):
                terminal = self.terminal("![proxied](http://tmd-image.invalid/badge)")
            self.placement(terminal)
            self.assertEqual(requests.read_text().splitlines(), ["tmd-image.invalid"])
            self.assertEqual(terminal.finish(b"q"), 0)
            self.assert_restored(terminal)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--server", type=Path)
    parser.add_argument("--delay", type=float, default=0)
    parser.add_argument("--socks", action="store_true")
    parser.add_argument("--binary", type=Path, default=reader.BINARY)
    args, remaining = parser.parse_known_args()
    if args.server:
        faulthandler.dump_traceback_later(6)
        serve(args.server, args.delay, args.socks)
        sys.exit(0)
    reader.BINARY = args.binary.resolve()
    unittest.main(argv=[__file__, *remaining])
