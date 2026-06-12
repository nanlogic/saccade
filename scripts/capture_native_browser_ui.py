#!/usr/bin/env python3
import argparse
import json
import pathlib
import platform
import subprocess
import sys
import tempfile
import time


WORKSPACE = pathlib.Path(__file__).resolve().parents[1]

BROWSERS = {
    "chrome": {
        "app_name": "Google Chrome",
        "bundle_id": "com.google.Chrome",
        "kind": "chrome",
        "filename": "chrome_native_window.png",
    },
    "safari": {
        "app_name": "Safari",
        "bundle_id": "com.apple.Safari",
        "kind": "safari",
        "filename": "safari_native_window.png",
    },
    "firefox": {
        "app_name": "Firefox",
        "bundle_id": "org.mozilla.firefox",
        "kind": "generic",
        "process_name": "Firefox",
        "filename": "firefox_native_window.png",
    },
}


def main():
    args = parse_args()
    output_dir = pathlib.Path(args.output_dir).resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    browser = BROWSERS[args.browser]
    manifest_path = output_dir / "native_browser_ui_manifest.json"
    screenshot_path = output_dir / browser["filename"]

    manifest = base_manifest(args, browser, screenshot_path)
    try:
        if platform.system() != "Darwin":
            raise CaptureUnavailable("native browser UI capture currently supports macOS only")
        ensure_browser_exists(browser)
        probe_screen_capture()

        bounds = open_browser_window(args, browser)
        manifest["window"] = bounds
        rect = bounds_to_capture_rect(bounds)
        result = subprocess.run(
            ["screencapture", "-x", f"-R{rect}", str(screenshot_path)],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=max(5, args.timeout_sec),
        )
        if result.returncode != 0 or not screenshot_path.exists():
            raise CaptureUnavailable(
                "screencapture failed; grant Screen Recording permission to the terminal/Codex app",
                stderr=result.stderr.strip(),
                returncode=result.returncode,
            )
        manifest["status"] = "captured"
        manifest["screenshot"] = str(screenshot_path)
        manifest["screenshot_bytes"] = screenshot_path.stat().st_size
    except CaptureUnavailable as error:
        manifest["status"] = "capture_unavailable"
        manifest["error"] = error.to_json()
    except Exception as error:
        manifest["status"] = "capture_failed"
        manifest["error"] = {"message": str(error), "type": type(error).__name__}
    finally:
        if args.close_window:
            close_browser_window(browser, manifest.get("window", {}).get("window_id"))
        manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")

    print(
        "NATIVE BROWSER UI "
        f"browser={args.browser} status={manifest['status']} manifest={manifest_path}"
    )
    if manifest["status"] != "captured" and not args.allow_failure:
        return 2
    return 0


def parse_args():
    parser = argparse.ArgumentParser(
        description="Capture a real macOS browser window with native browser chrome for demo evidence."
    )
    parser.add_argument("--browser", choices=sorted(BROWSERS), required=True)
    parser.add_argument("--url", required=True)
    parser.add_argument("--output-dir", required=True)
    parser.add_argument("--left", type=int, default=80)
    parser.add_argument("--top", type=int, default=60)
    parser.add_argument("--width", type=int, default=1280)
    parser.add_argument("--height", type=int, default=860)
    parser.add_argument("--settle-ms", type=int, default=1800)
    parser.add_argument("--timeout-sec", type=float, default=15)
    parser.add_argument(
        "--close-window",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="Close the temporary browser window after capture.",
    )
    parser.add_argument(
        "--allow-failure",
        action="store_true",
        help="Write a manifest and exit 0 when macOS capture permissions are unavailable.",
    )
    return parser.parse_args()


def base_manifest(args, browser, screenshot_path):
    return {
        "engine": "saccade-native-browser-ui-capture-v0",
        "captured_at_unix_ms": int(time.time() * 1000),
        "browser": args.browser,
        "app_name": browser["app_name"],
        "bundle_id": browser["bundle_id"],
        "url": args.url,
        "status": "pending",
        "method": "macos_osascript_plus_screencapture_rect",
        "requested_bounds": {
            "left": args.left,
            "top": args.top,
            "width": args.width,
            "height": args.height,
        },
        "screenshot": str(screenshot_path),
        "limitations": [
            "captures the visible macOS screen rectangle, so the browser window must be unobscured",
            "requires macOS Screen Recording permission for the terminal/Codex host app",
            "intended for public/demo evidence, not agent truth or replay verification",
        ],
    }


class CaptureUnavailable(Exception):
    def __init__(self, message, **extra):
        super().__init__(message)
        self.message = message
        self.extra = extra

    def to_json(self):
        value = {"message": self.message, "type": type(self).__name__}
        value.update(self.extra)
        return value


def ensure_browser_exists(browser):
    result = subprocess.run(
        ["osascript", "-e", f'id of application "{browser["app_name"]}"'],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=5,
    )
    if result.returncode != 0:
        raise CaptureUnavailable(
            f'{browser["app_name"]} is not scriptable or not installed',
            stderr=result.stderr.strip(),
            returncode=result.returncode,
        )


def probe_screen_capture():
    with tempfile.TemporaryDirectory(prefix="saccade-screen-probe-") as tmp:
        probe_path = pathlib.Path(tmp) / "probe.png"
        result = subprocess.run(
            ["screencapture", "-x", "-R0,0,1,1", str(probe_path)],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=5,
        )
        if result.returncode != 0 or not probe_path.exists():
            raise CaptureUnavailable(
                "macOS screen capture is unavailable; grant Screen Recording permission to the terminal/Codex app",
                stderr=result.stderr.strip(),
                returncode=result.returncode,
            )


def open_browser_window(args, browser):
    left = args.left
    top = args.top
    right = args.left + args.width
    bottom = args.top + args.height
    settle_seconds = max(0, args.settle_ms) / 1000
    url = applescript_string(args.url)
    if browser["kind"] == "chrome":
        script = f"""
tell application "{browser['app_name']}"
  activate
  set w to make new window
  set URL of active tab of w to "{url}"
  set bounds of w to {{{left}, {top}, {right}, {bottom}}}
  delay {settle_seconds}
  set b to bounds of w
  return (id of w as text) & "|" & (item 1 of b as text) & "," & (item 2 of b as text) & "," & (item 3 of b as text) & "," & (item 4 of b as text)
end tell
"""
    elif browser["kind"] == "safari":
        script = f"""
tell application "{browser['app_name']}"
  activate
  set d to make new document with properties {{URL:"{url}"}}
  set w to front window
  set bounds of w to {{{left}, {top}, {right}, {bottom}}}
  delay {settle_seconds}
  set b to bounds of w
  return (id of w as text) & "|" & (item 1 of b as text) & "," & (item 2 of b as text) & "," & (item 3 of b as text) & "," & (item 4 of b as text)
end tell
"""
    else:
        return open_generic_browser_window(args, browser)
    result = subprocess.run(
        ["osascript", "-e", script],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=max(5, args.timeout_sec),
    )
    if result.returncode != 0:
        raise CaptureUnavailable(
            f'failed to open {browser["app_name"]} window through AppleScript',
            stderr=result.stderr.strip(),
            returncode=result.returncode,
        )
    return parse_window_response(result.stdout.strip())


def open_generic_browser_window(args, browser):
    left = args.left
    top = args.top
    settle_seconds = max(0, args.settle_ms) / 1000
    result = subprocess.run(
        ["open", "-na", browser["app_name"], "--args", "--new-window", args.url],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=max(5, args.timeout_sec),
    )
    if result.returncode != 0:
        raise CaptureUnavailable(
            f'failed to open {browser["app_name"]} through macOS open',
            stderr=result.stderr.strip(),
            returncode=result.returncode,
        )
    time.sleep(settle_seconds)
    process_name = applescript_string(browser.get("process_name", browser["app_name"]))
    script = f"""
tell application "System Events"
  tell process "{process_name}"
    set frontmost to true
    delay 0.2
    set w to front window
    set position of w to {{{left}, {top}}}
    set size of w to {{{args.width}, {args.height}}}
    delay {settle_seconds}
    set p to position of w
    set s to size of w
    return "generic|" & (item 1 of p as text) & "," & (item 2 of p as text) & "," & ((item 1 of p) + (item 1 of s) as text) & "," & ((item 2 of p) + (item 2 of s) as text)
  end tell
end tell
"""
    result = subprocess.run(
        ["osascript", "-e", script],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=max(5, args.timeout_sec),
    )
    if result.returncode != 0:
        raise CaptureUnavailable(
            f'failed to position {browser["app_name"]} window through System Events',
            stderr=result.stderr.strip(),
            returncode=result.returncode,
        )
    return parse_window_response(result.stdout.strip())


def parse_window_response(value):
    window_id = None
    bounds_text = value
    if "|" in value:
        window_id, bounds_text = value.split("|", 1)
    left, top, right, bottom = [int(float(part.strip())) for part in bounds_text.split(",")]
    return {
        "window_id": window_id,
        "left": left,
        "top": top,
        "right": right,
        "bottom": bottom,
        "width": max(0, right - left),
        "height": max(0, bottom - top),
    }


def bounds_to_capture_rect(bounds):
    return f"{bounds['left']},{bounds['top']},{bounds['width']},{bounds['height']}"


def close_browser_window(browser, window_id):
    if not window_id:
        return
    if browser["kind"] == "chrome":
        script = f"""
tell application "{browser['app_name']}"
  try
    close (first window whose id is {window_id})
  end try
end tell
"""
    elif browser["kind"] == "generic":
        process_name = applescript_string(browser.get("process_name", browser["app_name"]))
        script = f"""
tell application "System Events"
  tell process "{process_name}"
    try
      keystroke "w" using command down
    end try
  end tell
end tell
"""
    else:
        script = f"""
tell application "{browser['app_name']}"
  try
    close (first window whose id is {window_id})
  end try
end tell
"""
    subprocess.run(
        ["osascript", "-e", script],
        text=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        timeout=5,
    )


def applescript_string(value):
    return value.replace("\\", "\\\\").replace('"', '\\"')


if __name__ == "__main__":
    sys.exit(main())
