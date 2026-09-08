"""Summarize existing tracing-subscriber JSON close events; stdlib only."""
import collections
import json
import re
import sys


def milliseconds(value):
    match = re.fullmatch(r"([\d.]+)(ns|µs|us|ms|s)", value)
    if not match:
        raise ValueError(value)
    return float(match[1]) * {"ns": 1e-6, "µs": 1e-3, "us": 1e-3,
                             "ms": 1, "s": 1000}[match[2]]


def summarize(lines):
    groups = collections.defaultdict(list)
    for line in lines:
        try:
            row = json.loads(line)
        except json.JSONDecodeError:
            continue  # Decoder diagnostics share stderr.
        fields = row.get("fields", {})
        if fields.get("message") != "close" or "time.busy" not in fields:
            continue
        name = row["target"] + "::" + row["span"]["name"]
        elapsed = milliseconds(fields["time.busy"])
        groups[name].append(elapsed)
        if row["target"] == "falcon::runtime":
            tick = row["span"]["tick"]
            groups[f"runtime bucket: {'delivery tick 97' if tick == 97 else 'other ticks'}"].append(elapsed)
    return groups


if __name__ == "__main__":
    for path in sys.argv[1:]:
        with open(path) as source:
            groups = summarize(source)
        if not groups:
            raise SystemExit(f"No timed close events: {path}")
        print(f"\n## {path}\n")
        print("Inclusive busy milliseconds. Nested rows overlap. Debug + synchronous tracing overhead.\n")
        print("| Span | Calls | Total ms | Mean ms | Max ms |")
        print("| --- | ---: | ---: | ---: | ---: |")
        for name, times in sorted(groups.items(), key=lambda item: sum(item[1]), reverse=True):
            print(f"| {name} | {len(times)} | {sum(times):.3f} | {sum(times)/len(times):.3f} | {max(times):.3f} |")
