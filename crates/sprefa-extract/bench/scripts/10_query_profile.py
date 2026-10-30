"""Profile one `ryii query` run: CPU and RSS sampled from `ps`, spans from the
Chrome trace `HAFLEY_TRACE` writes, rendered as one HTML chart plus a table.

    python3 10_query_profile.py OUT_DIR RYII [query args...] < paths.txt

OUT_DIR receives trace.json, samples.tsv, spans.tsv and profile.html.
"""

import json
import os
import subprocess
import sys
import time
from collections import defaultdict

SAMPLE_SECONDS = 0.02
BUCKET_SECONDS = 0.05


def cpu_seconds(text):
    # ps `time` is [[dd-]hh:]mm:ss.cc
    parts = text.strip().replace("-", ":").split(":")
    seconds = 0.0
    for part in parts:
        seconds = seconds * 60 + float(part)
    return seconds


def run(out_dir, command, stdin_bytes):
    os.makedirs(out_dir, exist_ok=True)
    trace = os.path.join(out_dir, "trace.json")
    env = dict(os.environ, HAFLEY_TRACE=trace, HAFLEY_TRACE_FILTER="debug")
    start = time.monotonic()
    child = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, env=env)
    child.stdin.write(stdin_bytes)
    child.stdin.close()
    samples = []
    while child.poll() is None:
        probe = subprocess.run(
            ["ps", "-o", "rss=,time=", "-p", str(child.pid)], capture_output=True, text=True
        )
        fields = probe.stdout.split()
        # A probe racing the exit reads a zombie: RSS 0 and a reset CPU clock.
        if len(fields) == 2 and int(fields[0]) > 0:
            samples.append((time.monotonic() - start, int(fields[0]) / 1024, cpu_seconds(fields[1])))
        time.sleep(SAMPLE_SECONDS)
    wall = time.monotonic() - start
    with open(os.path.join(out_dir, "samples.tsv"), "w") as out:
        out.write("t_s\trss_mb\tcpu_s\n")
        for t, rss, cpu in samples:
            out.write(f"{t:.3f}\t{rss:.1f}\t{cpu:.2f}\n")
    return trace, samples, wall, child.returncode


def spans(trace):
    events = json.load(open(trace))
    events = events if isinstance(events, list) else events["traceEvents"]
    begun = {}
    closed = []
    for event in events:
        key = (event.get("tid"), event.get("name"))
        if event.get("ph") == "B":
            begun.setdefault(key, []).append(event["ts"])
        elif event.get("ph") == "E" and begun.get(key):
            started = begun[key].pop()
            closed.append((event["name"], event["tid"], started / 1e6, event["ts"] / 1e6))
        elif event.get("ph") == "X":
            closed.append((event["name"], event["tid"], event["ts"] / 1e6, (event["ts"] + event["dur"]) / 1e6))
    origin = min(start for _, _, start, _ in closed)
    return [(name, tid, start - origin, end - origin) for name, tid, start, end in closed]


def table(closed, wall):
    totals = defaultdict(list)
    for name, _, start, end in closed:
        totals[name].append(end - start)
    rows = []
    for name, durations in totals.items():
        durations.sort()
        total = sum(durations)
        rows.append((
            name, len(durations), total, total / len(durations) * 1e3,
            durations[len(durations) // 2] * 1e3, durations[int(len(durations) * 0.99)] * 1e3,
            durations[-1] * 1e3,
        ))
    rows.sort(key=lambda row: -row[2])
    return rows


def concurrency(closed, wall):
    buckets = int(wall / BUCKET_SECONDS) + 1
    series = defaultdict(lambda: [0.0] * buckets)
    for name, _, start, end in closed:
        if name == "query.file":
            continue  # the parent of every worker phase; its children carry the split
        first, last = int(start / BUCKET_SECONDS), int(end / BUCKET_SECONDS)
        for bucket in range(first, min(last, buckets - 1) + 1):
            low = max(start, bucket * BUCKET_SECONDS)
            high = min(end, (bucket + 1) * BUCKET_SECONDS)
            if high > low:
                series[name][bucket] += (high - low) / BUCKET_SECONDS
    return [bucket * BUCKET_SECONDS for bucket in range(buckets)], dict(series)


def render(out_dir, command, samples, wall, closed, rows):
    times, series = concurrency(closed, wall)
    cpu_cores = []
    for (t0, _, c0), (t1, _, c1) in zip(samples, samples[1:]):
        cpu_cores.append(((t0 + t1) / 2, (c1 - c0) / (t1 - t0) if t1 > t0 else 0))
    traces = [
        {"x": [t for t, _ in cpu_cores], "y": [c for _, c in cpu_cores], "name": "CPU (cores, ps)",
         "type": "scatter", "mode": "lines", "line": {"color": "black", "width": 2}, "xaxis": "x", "yaxis": "y"},
        {"x": [t for t, _, _ in samples], "y": [r for _, r, _ in samples], "name": "RSS MB (ps)",
         "type": "scatter", "mode": "lines", "line": {"color": "crimson", "width": 2}, "xaxis": "x", "yaxis": "y2"},
    ]
    for name in sorted(series, key=lambda name: -sum(series[name])):
        traces.append({"x": times, "y": series[name], "name": name, "type": "scatter", "mode": "lines",
                       "stackgroup": "phases", "xaxis": "x", "yaxis": "y3"})
    layout = {
        "title": " ".join(command[1:])[:160], "height": 820, "hovermode": "x unified",
        "xaxis": {"title": "seconds since start", "anchor": "y3"},
        "yaxis": {"title": "CPU cores", "domain": [0.62, 1.0]},
        "yaxis2": {"title": "RSS MB", "overlaying": "y", "side": "right"},
        "yaxis3": {"title": "threads in span (50 ms buckets)", "domain": [0.0, 0.56]},
        "legend": {"orientation": "h", "y": -0.12},
    }
    head = "<tr><th>span</th><th>count</th><th>total s</th><th>mean ms</th><th>p50 ms</th><th>p99 ms</th><th>max ms</th></tr>"
    body = "".join(
        f"<tr><td>{name}</td><td>{count}</td><td>{total:.3f}</td><td>{mean:.3f}</td><td>{p50:.3f}</td><td>{p99:.3f}</td><td>{top:.1f}</td></tr>"
        for name, count, total, mean, p50, p99, top in rows
    )
    peak = max(r for _, r, _ in samples) if samples else 0
    cpu = samples[-1][2] if samples else 0
    html = f"""<!doctype html><meta charset=utf-8><title>ryii query profile</title>
<script src="https://cdn.plot.ly/plotly-2.35.2.min.js"></script>
<style>body{{font:13px system-ui;margin:16px}}table{{border-collapse:collapse}}td,th{{border:1px solid #ccc;padding:3px 8px;text-align:right}}td:first-child,th:first-child{{text-align:left}}</style>
<p>wall {wall:.2f} s · cpu {cpu:.2f} s (ps) · peak RSS {peak:.0f} MB · {len(closed)} spans</p>
<div id=chart></div><table>{head}{body}</table>
<script>Plotly.newPlot("chart", {json.dumps(traces)}, {json.dumps(layout)});</script>"""
    path = os.path.join(out_dir, "profile.html")
    open(path, "w").write(html)
    with open(os.path.join(out_dir, "spans.tsv"), "w") as out:
        out.write("span\tcount\ttotal_s\tmean_ms\tp50_ms\tp99_ms\tmax_ms\n")
        for row in rows:
            out.write("\t".join(str(round(value, 3)) if isinstance(value, float) else str(value) for value in row) + "\n")
    return path


def main():
    out_dir, command = sys.argv[1], sys.argv[2:]
    trace, samples, wall, code = run(out_dir, command, sys.stdin.buffer.read())
    closed = spans(trace)
    rows = table(closed, wall)
    print(render(out_dir, command, samples, wall, closed, rows), f"exit {code}", f"wall {wall:.2f}s")
    for name, count, total, mean, p50, p99, top in rows:
        print(f"{name:16} {count:7} {total:8.3f}s mean {mean:7.3f}ms p99 {p99:7.3f}ms max {top:7.1f}ms")


if __name__ == "__main__":
    main()
