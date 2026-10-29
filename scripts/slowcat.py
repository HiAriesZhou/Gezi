#!/usr/bin/env python3
"""Write a file to stdout at a fixed byte rate, like an AI tool streaming
its answer. Used to try tmd's streaming by hand:

    python3 scripts/slowcat.py fixtures/long.md | cargo run --release
"""

import argparse
import os
import sys
import time

# Bytes written per write; small enough to cut lines and UTF-8 characters
# at arbitrary points, which is what streaming must survive.
CHUNK = 7


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("file", help="file to write")
    parser.add_argument(
        "--rate", type=int, default=300, help="bytes per second (default: 300)"
    )
    args = parser.parse_args()
    if args.rate <= 0:
        parser.error("--rate must be positive")

    with open(args.file, "rb") as f:
        data = f.read()

    out = sys.stdout.buffer
    pause = CHUNK / args.rate
    try:
        for start in range(0, len(data), CHUNK):
            out.write(data[start : start + CHUNK])
            out.flush()
            time.sleep(pause)
    except BrokenPipeError:
        # The reader quit (e.g. tmd was interrupted). Point stdout at
        # /dev/null so the flush at interpreter exit does not fail again.
        os.dup2(os.open(os.devnull, os.O_WRONLY), sys.stdout.fileno())
        return 0
    except KeyboardInterrupt:
        return 0
    return 0


if __name__ == "__main__":
    sys.exit(main())
