"""Bounded loopback UDP fault relay and exact-child lifecycle controller."""
import heapq
import json
import os
from pathlib import Path
import selectors
import signal
import socket
import subprocess
import sys
import time


def policy(peer, sequence, elapsed_ms):
    if elapsed_ms < 0:
        return 0, False
    delay = 20 + (sequence % 3) * 10
    if peer == 0 and 3050 <= elapsed_ms < 4070:
        delay = max(delay, 4070 - elapsed_ms)
    return delay, sequence % 17 == 0


def run(binary):
    relay = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    relay.bind(("127.0.0.1", 0))
    relay.setblocking(False)
    reserved = [socket.socket(socket.AF_INET, socket.SOCK_DGRAM) for _ in range(2)]
    for sock in reserved:
        sock.bind(("127.0.0.1", 0))
    addresses = [sock.getsockname() for sock in reserved]
    for sock in reserved:
        sock.close()  # GGRS owns its socket; a port collision fails the bounded run.
    children, logs = [], []
    selector = selectors.DefaultSelector()
    selector.register(relay, selectors.EVENT_READ)
    queued, audit = [], []
    counts = [0, 0]
    start = None
    deadline = time.monotonic() + 25
    peak_queue = 0
    try:
        for peer, address in enumerate(addresses):
            log = open(f"peer-{peer}.log", "wb")
            logs.append(log)
            children.append(subprocess.Popen(
                [binary, "--process-peer", str(peer), str(address[1]),
                 f"127.0.0.1:{relay.getsockname()[1]}"], stdout=log, stderr=log,
                env={**os.environ, "RUST_LOG": "warn"}))
        while any(child.poll() is None for child in children):
            assert time.monotonic() < deadline, "25-second child watchdog"
            for child in children:
                assert child.poll() in (None, 0), f"peer {child.pid} exited {child.returncode}"
            if start is None and all(Path(f"ready-{i}").exists() for i in range(2)):
                start_ms = int(time.time() * 1000) + 500
                start = time.monotonic() + (start_ms / 1000 - time.time())
                Path("start-ms.pending").write_text(str(start_ms))
                Path("start-ms.pending").replace("start-ms")
            for _key, _event in selector.select(.001):
                while True:
                    try:
                        data, source = relay.recvfrom(65535)
                    except BlockingIOError:
                        break
                    assert source in addresses, f"unexpected UDP source {source}"
                    peer = addresses.index(source)
                    counts[peer] += 1
                    sequence = counts[peer]
                    now = time.monotonic()
                    elapsed = -1 if start is None else (now - start) * 1000
                    delay, drop = policy(peer, sequence, elapsed)
                    row = dict(peer=peer, sequence=sequence, received_ms=elapsed,
                               delay_ms=delay, dropped=drop, bytes=len(data))
                    audit.append(row)
                    if not drop:
                        heapq.heappush(queued, (now + delay / 1000, len(audit), peer, data, row))
                        peak_queue = max(peak_queue, len(queued))
                        assert peak_queue <= 4096, "relay queue capacity"
            now = time.monotonic()
            while queued and queued[0][0] <= now:
                _due, _order, peer, data, row = heapq.heappop(queued)
                relay.sendto(data, addresses[1-peer])
                row["delivered_ms"] = -1 if start is None else (now-start)*1000
        assert all(child.wait() == 0 for child in children)
        assert len({child.pid for child in children}) == 2
        assert any(row["dropped"] for row in audit)
        assert any(row["delay_ms"] > 500 for row in audit)
        Path("network.json").write_text(json.dumps(dict(
            pids=[child.pid for child in children], counts=counts, peak_queue=peak_queue,
            pending_at_exit=len(queued), packets=audit), indent=2))
        print(json.dumps(dict(pids=[c.pid for c in children], packets=counts,
                              dropped=sum(r["dropped"] for r in audit), peak_queue=peak_queue)))
    finally:
        for child in children:
            if child.poll() is None:
                child.terminate()
        for child in children:
            try:
                child.wait(timeout=2)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
        for log in logs:
            log.close()
        selector.close()
        relay.close()


if __name__ == "__main__":
    signal.signal(signal.SIGTERM, lambda *_args: sys.exit(143))
    run(str(Path(sys.argv[1]).resolve()))
