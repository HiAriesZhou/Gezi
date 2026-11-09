#!/usr/bin/env python3
"""Measure release-binary reader response on a synthetic 1 MiB document."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import platform
import time

import test_reader_pty as harness


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    harness.BINARY = args.binary.resolve()
    sections = []
    for index in range(1000):
        sections.append(
            f"# Section-{index:04d} 中文 👩‍💻\n\n"
            f"Marker-{index:04d} README 与 AI 方案。\n\n"
            + "中文排版 mixed English **bold** `inline` é 👩‍💻。\n\n" * 5
            + "- task one\n- 任务二\n\n"
            + "> quoted explanation 引用。\n\n"
            + "| Item | Value |\n|---|---|\n| 中文 | 42 |\n\n"
            + '```rust\nfn main() {\n    println!("hello 中文");\n}\n```\n\n'
        )
    text = "".join(sections)
    remaining = 1048576 - len(text.encode())
    assert remaining > 0
    text += "padding text\n" * (remaining // 13) + " " * (remaining % 13)
    document = text.encode()
    assert len(document) == 1048576
    samples = {name: [] for name in (
        "outline_open_reflow", "outline_confirm", "sidebar_focus",
        "search_unique", "search_common", "search_missing", "search_next",
        "outline_close_reflow",
    )}
    terminal = harness.Terminal(("--large-headings",), text=text, cols=80)

    def measure(name, key):
        started = time.perf_counter()
        frame = terminal.frame(key)
        samples[name].append((time.perf_counter() - started) * 1000)
        return frame

    try:
        terminal.until(harness.END)
        for index in range(100):
            frame = measure("outline_open_reflow", b"o")
            assert "● 标题目录".encode() in frame
            assert b"\x1b#3" not in frame and b"\x1b#4" not in frame
            terminal.frame(b"\x1b[F" if index % 2 else b"\x1b[H")
            frame = measure("outline_confirm", b"\r")
            expected = b"Section-0999" if index % 2 else b"Section-0000"
            assert expected in frame and "● 正文".encode() in frame
            assert "● 标题目录".encode() in measure("sidebar_focus", b"\t")
            terminal.frame(b"\t")
            for kind, query, status in (
                ("search_unique", f"Marker-{index * 10:04d}", "匹配 1/1"),
                ("search_common", "中文", "匹配 1/"),
                ("search_missing", "nonexistent-query-xyz", "未找到匹配"),
            ):
                terminal.frame(b"/")
                terminal.frame(b"\x15")
                terminal.frame(b"\x1b[200~" + query.encode() + b"\x1b[201~")
                frame = measure(kind, b"\r")
                assert status.encode() in frame and "标题目录".encode() in frame
                if kind == "search_common":
                    assert "匹配 2/".encode() in measure("search_next", b"n")
            terminal.frame(b"\x1b")
            assert "标题目录".encode() not in measure("outline_close_reflow", b"o")
        assert terminal.finish(b"\x03") == 130
        assert harness.termios.tcgetattr(terminal.master) == terminal.before
    finally:
        terminal.close()

    def statistics(values):
        ordered = sorted(values)
        return {
            "n": len(values),
            "first_ms": values[0],
            "p50_ms": ordered[math.ceil(len(values) * .50) - 1],
            "p95_ms": ordered[math.ceil(len(values) * .95) - 1],
            "max_ms": max(values),
        }

    report = {
        "os": platform.platform(),
        "window": {"columns": 80, "rows": 24},
        "document_bytes": len(document),
        "document_sha256": hashlib.sha256(document).hexdigest(),
        "binary_sha256": hashlib.sha256(harness.BINARY.read_bytes()).hexdigest(),
        "method": (
            "100 samples per action, including first use. Monotonic key write to "
            "synchronized-output end; frame assertions; nearest-rank p95. "
            "PTY timing excludes terminal rendering and human input."
        ),
        "results": {name: statistics(values) for name, values in samples.items()},
        "samples_ms": samples,
    }
    print(json.dumps(report, indent=2))
    return 0 if all(result["p95_ms"] <= 100 for result in report["results"].values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
