#!/usr/bin/env python3
"""Check stream lifecycle in a PTY; real terminal visual checks are still required."""

import argparse
import errno
import fcntl
import json
import os
from pathlib import Path
import pty
import re
import select
import shlex
import signal
import struct
import sys
import termios
import time


def produce(input_format, release_fd):
    text = "```text\n" + "".join(
        f"TMD_PTY_{i:04d} " + "x" * 120 + " 中文🙂\n" for i in range(24)
    ) + "```\n"
    if input_format == "claude-json":
        events = [{"type": "content_block_start", "index": 0,
                   "content_block": {"type": "text", "text": ""}}]
        events += [{"type": "content_block_delta", "index": 0,
                    "delta": {"type": "text_delta", "text": line}}
                   for line in text.splitlines(keepends=True)]
        events += [{"type": "content_block_stop", "index": 0}]
        text = "".join(json.dumps({"type": "stream_event", "event": event},
                                ensure_ascii=False) + chr(10) for event in events)
    data = text.encode()
    # Pause before row 2 until the parent has observed row 1 and resized.
    # This keeps slow renderer startup from consuming the entire probe at 80 columns.
    wait_at = data.index(b"TMD_PTY_0001")
    waiting = True
    try:
        for start in range(0, len(data), 7):
            if waiting and start >= wait_at:
                os.read(release_fd, 1)
                os.close(release_fd)
                waiting = False
            sys.stdout.buffer.write(data[start:start + 7])
            sys.stdout.buffer.flush()
            time.sleep(.001)
    except (BrokenPipeError, KeyboardInterrupt):
        pass


ANSI = re.compile(rb"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07\x1b]*(?:\x07|\x1b\\))")


def row_widths(output):
    plain = ANSI.sub(b"", bytes(output)).replace(b"\r", b"")
    return sorted({len(line) for line in plain.splitlines()
                   if line.startswith(b"TMD_PTY_") and line.isascii()})


def stop_child(pid, master):
    """Only signal the session this probe created, never an unrelated foreground job."""
    try:
        foreground = os.tcgetpgrp(master)
        if foreground > 0 and os.getsid(foreground) == pid:
            os.killpg(foreground, signal.SIGKILL)
    except (ProcessLookupError, OSError):
        pass
    try:
        os.kill(pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    os.waitpid(pid, 0)


def check_case(binary, interrupt, input_format="text"):
    release_read, release_write = os.pipe()
    os.set_inheritable(release_read, True)
    # Keep the shell input below macOS MAX_CANON (1024 bytes).
    command = (
        f"{shlex.quote(sys.executable)} {shlex.quote(str(Path(__file__).resolve()))} "
        f"--produce {shlex.quote(input_format)} --release-fd {release_read} | "
        f"{shlex.quote(str(binary))} --theme default-dark --from {shlex.quote(input_format)}; "
        "result=$?; printf 'TMD_PROCESS_EXIT:%s\\n' \"$result\"; exit \"$result\"\n"
    ).encode()
    ready_read, ready_write = os.pipe()
    try:
        pid, master = pty.fork()
    except BaseException:
        os.close(ready_read)
        os.close(ready_write)
        os.close(release_read)
        os.close(release_write)
        raise
    if pid == 0:
        os.close(release_write)
        os.close(ready_write)
        ready = os.read(ready_read, 1)
        os.close(ready_read)
        if not ready:
            os._exit(1)
        # No user shell configuration or persistent history is needed.
        for name in ("BASH_ENV", "ENV", "PROMPT_COMMAND"):
            os.environ.pop(name, None)
        os.environ.update(TERM="xterm-256color", TERM_PROGRAM="Kaku",
                          PS1="TMD_READY> ", HISTFILE="/dev/null", INPUTRC="/dev/null")
        # Readline's C-locale Meta bindings reinterpret UTF-8 command bytes.
        # Job control remains interactive; editing is not part of this probe.
        os.execl("/bin/bash", "bash", "--noprofile", "--norc", "--noediting", "-i")

    os.close(ready_read)
    os.close(release_read)
    output = bytearray()
    status = None
    sent = resized = interrupted = False
    started = time.monotonic()
    try:
        fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
        # The shell cannot enter readline and alter these attributes yet.
        before = termios.tcgetattr(master)
        os.write(ready_write, b"1")
        os.close(ready_write)
        ready_write = None
        while time.monotonic() - started < 20:
            # Synchronize on observed output, not runner speed or startup timing.
            widths = row_widths(output)
            if sent and 80 in widths and not resized:
                fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", 12, 60, 0, 0))
                os.write(release_write, b"1")
                os.close(release_write)
                release_write = None
                resized = True
            if sent and interrupt and 60 in widths and not interrupted:
                os.write(master, b"\x03")
                interrupted = True
            if select.select([master], [], [], .02)[0]:
                try:
                    chunk = os.read(master, 65536)
                except OSError as exc:
                    if exc.errno != errno.EIO:
                        raise
                    break
                if not chunk:
                    break
                output.extend(chunk)
                if not sent and b"TMD_READY> " in output:
                    os.write(master, command)
                    sent = True
                    started = time.monotonic()
        else:
            receipt = {
                "command_sent": sent, "resized": resized, "interrupted": interrupted,
                "bytes": len(output), "frames": output.count(b"\x1b[?2026h"),
                "widths": row_widths(output),
                "tail": bytes(output[-500:]).decode("utf-8", errors="replace"),
            }
            raise RuntimeError("PTY probe timed out: " + json.dumps(receipt))

        _, status = os.waitpid(pid, 0)
        after = termios.tcgetattr(master)
    finally:
        if release_write is not None:
            os.close(release_write)
        if ready_write is not None:
            os.close(ready_write)
        if status is None:
            stop_child(pid, master)
        os.close(master)

    code = os.waitstatus_to_exitcode(status)
    data = bytes(output)
    widths = row_widths(data)
    expected = 130 if interrupt else 0
    checks = {
        "expected_exit": code == expected,
        "streamed_multiple_frames": data.count(b"\x1b[?2026h") > 1,
        "synchronized_update_finished": data.rfind(b"\x1b[?2026l")
        > data.rfind(b"\x1b[?2026h") >= 0,
        "resized_output": 80 in widths and 60 in widths,
        "cursor_hidden_then_restored": data.find(b"\x1b[?25l") >= 0
        and data.rfind(b"\x1b[?25h") > data.rfind(b"\x1b[?25l"),
        "unicode_push_then_pop": data.find(b"UnicodeVersion=push") >= 0
        and data.rfind(b"UnicodeVersion=pop") > data.rfind(b"UnicodeVersion=push"),
        "terminal_flags_preserved": before == after,
        "shell_resumed": f"TMD_PROCESS_EXIT:{expected}".encode() in data,
    }
    result = {"format": input_format, "case": "ctrl-c" if interrupt else "normal-resize", "exit": code,
              "frames": data.count(b"\x1b[?2026h"), "observed_widths": widths,
              "checks": checks}
    print(json.dumps(result), flush=True)
    if not all(checks.values()):
        raise RuntimeError("PTY checks failed: " + ", ".join(
            key for key, passed in checks.items() if not passed))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path,
                        default=Path(__file__).resolve().parents[1] / "target/release/tmd")
    parser.add_argument("--produce", choices=("text", "claude-json"), help=argparse.SUPPRESS)
    parser.add_argument("--release-fd", type=int, help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.produce:
        if args.release_fd is None:
            parser.error("--produce requires --release-fd")
        produce(args.produce, args.release_fd)
        return
    binary = args.binary.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error("build tmd first or pass an executable with --binary")
    for input_format in ("text", "claude-json"):
        check_case(binary, interrupt=False, input_format=input_format)
        check_case(binary, interrupt=True, input_format=input_format)


if __name__ == "__main__":
    main()
