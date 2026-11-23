#!/usr/bin/env python3
"""HIL test for the ESP32 serial JSON REPL.

Loads one test object per tests/test_obj*.json file, prints which object and
payload it is sending (like the legacy C++ suite), and checks the JSON status
lines printed back by the firmware. Two negative cases are also sent:
malformed JSON and an over-long line.

Requires pyserial. Close `espflash monitor` (Ctrl-C) before running this: the
port is opened exclusively. Usage:

    python3 tests/test_serial.py                  # auto-detect port
    python3 tests/test_serial.py --port /dev/ttyACM0 --expect-success
    python3 tests/test_serial.py --verbose --raw-log /tmp/serial.log

Color is used only on a TTY; set NO_COLOR to disable it explicitly.
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import re
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path

import serial
from serial import SerialException

ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")
# Search anywhere: the first response can arrive glued to a truncated boot log
# line on a freshly flashed board, which has no trailing newline.
STATUS_RE = re.compile(r'\{\s*"status"\s*:\s*"([A-Z_]+)"\s*\}')
BANNER = "Serial REPL ready"
RULE_WIDTH = 78


class Color:
    RESET = "\033[0m"
    BOLD = "\033[1m"
    DIM = "\033[2m"
    RED = "\033[31m"
    GREEN = "\033[32m"
    YELLOW = "\033[33m"
    BLUE = "\033[34m"
    MAGENTA = "\033[35m"
    CYAN = "\033[36m"


_USE_COLOR = sys.stdout.isatty() and not os.environ.get("NO_COLOR")


def paint(text: str, *codes: str) -> str:
    if not _USE_COLOR or not codes:
        return text
    return "".join(codes) + text + Color.RESET


def info(text: str) -> str:
    return paint(text, Color.BLUE)


def ok(text: str) -> str:
    return paint(text, Color.GREEN)


def warn(text: str) -> str:
    return paint(text, Color.YELLOW)


def err(text: str) -> str:
    return paint(text, Color.RED)


def dim(text: str) -> str:
    return paint(text, Color.DIM)


def heading(text: str) -> str:
    return paint(text, Color.BOLD, Color.CYAN)


def label(text: str) -> str:
    return paint(text, Color.BOLD, Color.MAGENTA)


def status_color(status: str) -> str:
    if status == "FORWARD_SUCCESS":
        return Color.GREEN
    if status == "FORWARD_FAIL":
        return Color.YELLOW
    return Color.RED


def rule(title: str = "") -> None:
    """Print a dim horizontal rule, optionally with a bold cyan title."""
    if title:
        head = f"── {title} "
        print(heading(head) + dim("─" * max(0, RULE_WIDTH - len(head))))
    else:
        print(dim("─" * RULE_WIDTH))


class PortError(Exception):
    pass


@dataclass
class Case:
    name: str
    label: str
    source: str
    payload: str
    pretty: str
    expected: str | None  # None = valid object, success or fail accepted


@dataclass
class Result:
    case: Case
    allowed: str
    actual: str | None
    elapsed_ms: int
    passed: bool


class Console:
    def __init__(self, port: str, baud: int, raw_log: Path | None, verbose: bool):
        self.raw_log = raw_log
        self.verbose = verbose
        try:
            self.ser = serial.Serial(
                port=port,
                baudrate=baud,
                timeout=0.1,
                exclusive=True,
                dsrdtr=False,
                rtscts=False,
            )
        except SerialException as error:
            raise PortError(
                f"cannot open {port} exclusively: {error}. "
                "Close espflash monitor (Ctrl-C) first."
            ) from error
        # Do not reset the input buffer: if we attach while boot logs are still
        # streaming, that truncates a line mid-way and glues the first status
        # response onto the log fragment.
        self._log_file = raw_log.open("w", encoding="utf-8") if raw_log else None

    def close(self) -> None:
        self.ser.close()
        if self._log_file:
            self._log_file.close()

    def write_line(self, text: str) -> None:
        self.ser.write(text.encode("utf-8") + b"\n")

    def read_line(self, deadline: float) -> str | None:
        while True:
            raw = self.ser.readline()
            if raw:
                line = ANSI_RE.sub("", raw.decode("utf-8", "replace")).strip()
                if self._log_file:
                    self._log_file.write(f"{time.time():.3f} {line}\n")
                    self._log_file.flush()
                if self.verbose:
                    print(dim(f"    [serial] {line}"))
                return line
            if time.monotonic() >= deadline:
                return None

    def wait_for_banner(self, seconds: float) -> bool:
        deadline = time.monotonic() + seconds
        while True:
            line = self.read_line(deadline)
            if line is None:
                return False
            if BANNER in line:
                return True

    def wait_for_status(self, deadline: float) -> tuple[str | None, list[str]]:
        logs = []
        while True:
            line = self.read_line(deadline)
            if line is None:
                return None, logs
            match = STATUS_RE.search(line)
            if match:
                return match.group(1), logs
            logs.append(line)


def find_test_objs(tests_dir: Path) -> list[Path]:
    return sorted(path for path in tests_dir.glob("test_obj*.json") if path.is_file())


def load_cases(tests_dir: Path) -> list[Case]:
    repo_root = tests_dir.parent
    cases = []

    for path in find_test_objs(tests_dir):
        try:
            obj = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            print(err(f"[!] Warning: skipping {path}: {error}"), file=sys.stderr)
            continue

        model = obj.get("cap_model", "?")
        pretty_label = f"{model} / {str(obj.get('test_type', '?')).upper()}"
        cases.append(
            Case(
                name=path.stem,
                label=pretty_label,
                source=str(path.relative_to(repo_root)),
                payload=json.dumps(obj, separators=(",", ":")),
                pretty=json.dumps(obj, indent=2),
                expected=None,
            )
        )

    cases.extend(
        [
            Case(
                name="bad_json",
                label="malformed payload",
                source="inline",
                payload="{this is not json}",
                pretty="{this is not json}",
                expected="BAD_JSON",
            ),
            Case(
                name="long_line",
                label="600-byte line",
                source="inline",
                payload="A" * 600,
                pretty='"A" * 600  (600 bytes)',
                expected="LINE_TOO_LONG",
            ),
        ]
    )
    return cases


def find_port(explicit: str | None) -> str:
    if explicit:
        return explicit
    candidates = sorted(glob.glob("/dev/ttyACM*") + glob.glob("/dev/ttyUSB*"))
    if not candidates:
        raise PortError("no /dev/ttyACM* or /dev/ttyUSB* found; pass --port")
    return candidates[0]


def warn_if_monitor_running() -> None:
    try:
        found = subprocess.run(
            ["pgrep", "-af", "espflash.*monitor"],
            check=False,
            capture_output=True,
            text=True,
        )
    except FileNotFoundError:
        return
    if found.returncode == 0:
        print(warn("[-] An espflash monitor is running; close it (Ctrl-C) first"))
        print(dim(found.stdout.strip()))


def run_case(
    console: Console, case: Case, timeout: float, expect_success: bool
) -> Result:
    if case.expected is None:
        allowed = (
            {"FORWARD_SUCCESS"} if expect_success else {"FORWARD_SUCCESS", "FORWARD_FAIL"}
        )
    else:
        allowed = {case.expected}

    kind = "Object" if case.expected is None else "Case"
    rule(f"{kind}: {case.name}  ({case.label})")
    print(info("[*] Payload source: ") + paint(case.source, Color.CYAN))
    print(info("[*] Sending payload:"))
    for line in case.pretty.splitlines():
        print(dim(f"    {line}"))

    started = time.monotonic()
    console.write_line(case.payload)
    actual, logs = console.wait_for_status(started + timeout)
    elapsed_ms = round((time.monotonic() - started) * 1000)

    if actual:
        passed = actual in allowed
        mark = ok("[+]") if passed else err("[-]")
        response = paint(json.dumps({"status": actual}), status_color(actual))
        print(f"{mark} Response: {response} {dim(f'({elapsed_ms} ms)')}")
    else:
        print(f"{err('[-]')} No status response within {timeout:g}s ({elapsed_ms} ms)")
    for line in logs:
        print(dim(f"    [log] {line}"))

    return Result(
        case=case,
        allowed=" | ".join(sorted(allowed)),
        actual=actual,
        elapsed_ms=elapsed_ms,
        passed=actual in allowed,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", help="serial port (default: auto-detect)")
    parser.add_argument("--baud", type=int, default=115200)
    parser.add_argument("--timeout", type=float, default=5.0, help="seconds per case")
    parser.add_argument(
        "--boot-timeout", type=float, default=3.0, help="seconds to wait for the REPL banner"
    )
    parser.add_argument("--expect-success", action="store_true", help="require UART ACKs")
    parser.add_argument("--verbose", action="store_true", help="echo every serial line")
    parser.add_argument("--raw-log", type=Path, help="write the whole transcript here")
    args = parser.parse_args()

    warn_if_monitor_running()

    tests_dir = Path(__file__).resolve().parent
    cases = load_cases(tests_dir)
    objects = [case for case in cases if case.expected is None]
    if not objects:
        print(err("[-] No tests/test_obj*.json files found"), file=sys.stderr)
        return 2

    try:
        port = find_port(args.port)
    except PortError as error:
        print(err(f"[-] {error}"), file=sys.stderr)
        return 2

    rule("ESP32 Serial REPL - HIL test")
    print(info("[*] Target port: ") + paint(port, Color.CYAN))
    print(info("[*] Objects: ") + paint(str(len(objects)), Color.CYAN))
    for case in objects:
        print(dim(f"      {case.name}: {case.source}  ->  ") + label(case.label))
    print(info("[*] Timeout: ") + paint(f"{args.timeout:g}s per case", Color.CYAN))
    if args.expect_success:
        print(warn("[*] Mode: requiring FORWARD_SUCCESS (target MCU must answer ACKs)"))
    else:
        print(info("[*] Mode: accepting FORWARD_SUCCESS or FORWARD_FAIL"))

    try:
        console = Console(port, args.baud, args.raw_log, args.verbose)
    except PortError as error:
        print(err(f"[-] {error}"), file=sys.stderr)
        return 2

    results = []
    try:
        if args.boot_timeout > 0:
            if console.wait_for_banner(args.boot_timeout):
                print(ok("[+] Serial REPL banner seen"))
            else:
                print(warn("[-] Serial REPL banner not seen; continuing anyway"))
        for case in cases:
            results.append(run_case(console, case, args.timeout, args.expect_success))
    finally:
        console.close()

    rule("Results")
    for result in results:
        mark = ok("[+]") if result.passed else err("[-]")
        row = (
            f"{result.case.name:12} {result.case.label:28} "
            f"expected={result.allowed:32} got={result.actual or '<none>':16} "
            f"{result.elapsed_ms:5} ms"
        )
        print(f"{mark} {paint(row, Color.GREEN if result.passed else Color.RED)}")

    failed = [result for result in results if not result.passed]
    if failed:
        print(err(f"[-] {len(failed)}/{len(results)} tests failed"))
        return 1
    print(ok(f"[+] All {len(results)} tests passed"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
