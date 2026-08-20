#!/usr/bin/env python3
"""Local fixture server with one deterministic slow-resource route."""

from __future__ import annotations

import argparse
import functools
import http.server
import time


class FixtureHandler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self) -> None:
        if self.path.split("?", 1)[0].endswith("/fixtures/structural/slow_resource_payload.html"):
            time.sleep(1.5)
        super().do_GET()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bind", default="127.0.0.1")
    parser.add_argument("--port", default=8765, type=int)
    parser.add_argument("--directory", required=True)
    args = parser.parse_args()
    handler = functools.partial(FixtureHandler, directory=args.directory)
    server = http.server.ThreadingHTTPServer((args.bind, args.port), handler)
    server.serve_forever()


if __name__ == "__main__":
    main()
