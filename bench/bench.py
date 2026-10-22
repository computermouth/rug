#!/usr/bin/env python3
"""Benchmarks rug against Firefox and Chromium.

Each browser starts with a fresh throwaway profile and opens harness.html,
which runs the suites and posts results back here. Meanwhile this script
samples the memory (PSS) of the browser's whole process tree.

    bench/bench.py --fetch          # download benchmarks and test pages once
    bench/bench.py                  # run everything (~1h)
    bench/bench.py --browsers rug --suites load --runs 3

Everything downloaded or written goes in target/bench.
"""

import argparse
import http.server
import json
import os
import shutil
import signal
import statistics
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BENCH = ROOT / "target" / "bench"
HARNESS = Path(__file__).resolve().parent / "harness.html"
WIDTH, HEIGHT = 1100, 750

REPOS = {
    "speedometer": ("https://github.com/WebKit/Speedometer.git", "release/3.1"),
    "jetstream": ("https://github.com/WebKit/JetStream.git", "JetStream2.2"),
    "motionmark": ("https://github.com/WebKit/MotionMark.git", "release/MotionMark1.3.2"),
}
SITES = [
    "https://en.wikipedia.org/wiki/Web_browser",
    "https://news.ycombinator.com/",
    "https://developer.mozilla.org/en-US/docs/Web/HTML",
    "https://github.com/rust-lang/rust",
    "https://www.bbc.com/news",
]


def fetch():
    BENCH.mkdir(parents=True, exist_ok=True)
    for name, (url, branch) in REPOS.items():
        if not (BENCH / name).exists():
            subprocess.run(["git", "clone", "-q", "--depth", "1", "-b", branch, url, BENCH / name], check=True)
    sites = BENCH / "sites"
    sites.mkdir(exist_ok=True)
    for url in SITES:
        # Saved with their assets, so loads come from localhost, not the network.
        subprocess.run(["wget", "-q", "-E", "-H", "-k", "-p", "-e", "robots=off",
                        "-U", "Mozilla/5.0 (X11; Linux x86_64) Firefox/140.0", url], cwd=sites)


# Memory


def tree(root):
    """`root` and all its descendants."""
    children = {}
    for entry in os.listdir("/proc"):
        if entry.isdigit():
            try:
                stat = Path(f"/proc/{entry}/stat").read_text()
                ppid = int(stat.rsplit(")", 1)[1].split()[1])
                children.setdefault(ppid, []).append(int(entry))
            except (OSError, IndexError, ValueError):
                pass
    found, todo = [], [root]
    while todo:
        pid = todo.pop()
        found.append(pid)
        todo.extend(children.get(pid, []))
    return found


def pss_mb(root):
    total = 0
    for pid in tree(root):
        try:
            for line in Path(f"/proc/{pid}/smaps_rollup").read_text().splitlines():
                if line.startswith("Pss:"):
                    total += int(line.split()[1])
        except OSError:
            pass
    return total / 1024


# Server


class Run:
    """What one browser's run has reported so far."""

    def __init__(self):
        self.root_pid = None
        self.started = time.monotonic()
        self.ready = None
        self.done = threading.Event()
        self.results = {}
        self.marks = {}
        self.peak = 0.0


class Handler(http.server.SimpleHTTPRequestHandler):
    run = None

    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(BENCH), **kwargs)

    def log_message(self, *args):
        pass

    def end_headers(self):
        # No caching, so every browser loads the test pages the same way.
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def do_GET(self):
        if self.path.startswith("/harness.html"):
            return self.reply(HARNESS.read_bytes(), "text/html")
        super().do_GET()

    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0))).decode()
        run = self.run
        if self.path == "/ready":
            run.ready = time.monotonic() - run.started
        elif self.path == "/mark":
            name = json.loads(body)["name"]
            run.marks[name] = round(pss_mb(run.root_pid))
            print(f"    {name}: {run.marks[name]} MB")
        elif self.path == "/result":
            data = json.loads(body)
            run.results[data.pop("suite")] = data
            print(f"    result: {json.dumps(data)[:200]}")
        elif self.path == "/report":
            # JetStream's own report; it runs last, so this ends the run.
            tests = json.loads(body)["JetStream2.0"]["tests"].values()
            scores = [t["metrics"]["Score"]["current"][0] for t in tests]
            run.results["jetstream"] = {"score": statistics.geometric_mean(scores), "tests": len(scores)}
            print(f"    result: {run.results['jetstream']}")
            run.marks["after_jetstream"] = round(pss_mb(run.root_pid))
            print(f"    after_jetstream: {run.marks['after_jetstream']} MB")
            run.done.set()
        elif self.path == "/done":
            run.done.set()
        self.reply(b"")

    def reply(self, body, mime="text/plain", status=200):
        self.send_response(status)
        self.send_header("Content-Type", mime)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


# Browsers


def command(browser, url, profile):
    if browser == "rug":
        return [str(ROOT / "target" / "release" / "rug"), url], {
            # rug keeps everything under the XDG dirs; point them somewhere empty.
            "XDG_DATA_HOME": f"{profile}/data",
            "XDG_CONFIG_HOME": f"{profile}/config",
            "XDG_CACHE_HOME": f"{profile}/cache",
        }
    if browser == "firefox":
        Path(profile, "user.js").write_text("\n".join(f'user_pref("{k}", {json.dumps(v)});' for k, v in {
            "browser.shell.checkDefaultBrowser": False,
            "browser.aboutwelcome.enabled": False,
            "browser.startup.homepage_override.mstone": "ignore",
            "datareporting.policy.dataSubmissionEnabled": False,
            "toolkit.telemetry.reportingpolicy.firstRun": False,
            "trailhead.firstrun.didSeeAboutWelcome": True,
        }.items()))
        return ["firefox", "--no-remote", "--new-instance", "--profile", profile,
                "--width", str(WIDTH), "--height", str(HEIGHT), url], {}
    if browser == "chromium":
        return ["chromium", f"--user-data-dir={profile}", "--no-first-run", "--no-default-browser-check",
                "--password-store=basic", f"--window-size={WIDTH},{HEIGHT}", url], {}
    raise ValueError(browser)


def run_browser(browser, port, suites, runs, timeout):
    run = Run()
    Handler.run = run
    profile = tempfile.mkdtemp(prefix=f"bench-{browser}-")
    url = f"http://127.0.0.1:{port}/harness.html?suites={','.join(suites)}&runs={runs}"
    argv, env = command(browser, url, profile)
    quiet = None if os.environ.get("BENCH_VERBOSE") else subprocess.DEVNULL
    proc = subprocess.Popen(argv, env={**os.environ, **env}, start_new_session=True, stdout=quiet, stderr=quiet)
    run.root_pid = proc.pid

    deadline = time.monotonic() + timeout
    while not run.done.is_set() and time.monotonic() < deadline and proc.poll() is None:
        run.peak = max(run.peak, pss_mb(proc.pid))
        time.sleep(1)
    finished = run.done.is_set()

    try:
        os.killpg(proc.pid, signal.SIGTERM)
        proc.wait(10)
    except (ProcessLookupError, subprocess.TimeoutExpired):
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    time.sleep(2)
    shutil.rmtree(profile, ignore_errors=True)
    return {
        "finished": finished,
        "startup_s": round(run.ready, 2) if run.ready else None,
        "memory_mb": {**run.marks, "peak": round(run.peak)},
        **run.results,
    }


# Report


def table(results):
    browsers = list(results)
    rows = []

    def row(label, get, fmt="{:.0f}"):
        cells = []
        for b in browsers:
            try:
                v = get(results[b])
                cells.append(fmt.format(v) if v is not None else "–")
            except (KeyError, TypeError):
                cells.append("–")
        rows.append(f"| {label} | " + " | ".join(cells) + " |")

    row("Startup to harness loaded (s)", lambda r: r["startup_s"], "{:.2f}")
    for page in next((r["load"]["pages"] for r in results.values() if "pages" in r.get("load", {})), {}):
        row(f"Load {page}, median (ms)", lambda r, p=page: r["load"]["pages"][p]["median"])
    row("Speedometer 3.1 (higher is better)", lambda r: r["speedometer"]["score"], "{:.2f}")
    row("JetStream 2.2 (higher is better)", lambda r: r["jetstream"]["score"], "{:.1f}")
    row("MotionMark 1.3.2 (higher is better)", lambda r: r["motionmark"]["score"], "{:.1f}")
    for m in ["idle", "pages_open", "after_speedometer", "after_motionmark", "after_jetstream", "peak"]:
        row(f"Memory: {m.replace('_', ' ')} (MB)", lambda r, m=m: r["memory_mb"][m])
    return "\n".join([f"| | {' | '.join(browsers)} |", "|---" * (len(browsers) + 1) + "|", *rows])


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--fetch", action="store_true", help="download benchmarks and test pages")
    parser.add_argument("--browsers", default="rug,firefox,chromium")
    parser.add_argument("--suites", default="load,speedometer,motionmark,jetstream")
    parser.add_argument("--runs", type=int, default=5, help="loads per test page")
    parser.add_argument("--repeat", type=int, default=1, help="runs per browser; scores are medians")
    parser.add_argument("--timeout", type=int, default=60 * 60, help="seconds per browser run")
    args = parser.parse_args()
    # Show progress as it happens, even when logging to a file.
    sys.stdout.reconfigure(line_buffering=True)

    if args.fetch:
        return fetch()
    if not (BENCH / "speedometer").exists():
        sys.exit("run with --fetch first")
    if "rug" in args.browsers:
        subprocess.run(["cargo", "build", "--release", "-q"], cwd=ROOT, check=True)

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    port = server.server_address[1]

    suites = args.suites.split(",")
    all_runs = {b: [] for b in args.browsers.split(",")}
    for i in range(args.repeat):
        for browser in all_runs:
            print(f"{browser} (run {i + 1}/{args.repeat})")
            all_runs[browser].append(run_browser(browser, port, suites, args.runs, args.timeout))

    results = {b: merge_runs(runs) for b, runs in all_runs.items()}
    out = BENCH / f"results-{time.strftime('%Y%m%d-%H%M%S')}.json"
    out.write_text(json.dumps({"runs": all_runs, "summary": results}, indent=2))
    print(f"\nRaw results: {out}\n")
    print(table(results))


def merge_runs(runs):
    """Medians across repeated runs, for every number in the results."""
    if len(runs) == 1:
        return runs[0]

    def merge(values):
        if all(isinstance(v, dict) for v in values):
            keys = set().union(*values)
            return {k: merge([v[k] for v in values if k in v]) for k in keys}
        numbers = [v for v in values if isinstance(v, (int, float)) and not isinstance(v, bool)]
        return statistics.median(numbers) if numbers else values[0]

    return merge(runs)


if __name__ == "__main__":
    main()
