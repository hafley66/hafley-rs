"""Bounded fault injection into the exact Godot child spawned by this harness."""
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import time

ROOT = Path(__file__).resolve().parent


def main():
    command = ["godot", "--path", "godot", "--render-thread", "separate",
               "--fixed-fps", "60", "--disable-vsync", "--quit-after", "1800",
               "--write-movie", str(ROOT / "62_faults.avi"), "--", "--faults"]
    child = subprocess.Popen(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    selector = selectors.DefaultSelector()
    selector.register(child.stdout, selectors.EVENT_READ)
    start = time.monotonic()
    buffer = b""
    ticks = []
    events = {}
    thread_ids = {}
    lines = []
    stopped = False
    resume_at = None
    stop_tick_count = None
    sync_warnings = 0

    def line_received(line):
        nonlocal stopped, resume_at, stop_tick_count, sync_warnings
        if "RenderingServer synchronizations on every frame" in line:
            sync_warnings += 1
            return
        if "at:" in line and "rendering_server_default.h:" in line:
            return
        lines.append(line)
        if not line.startswith("WORKER_TICK "):
            print(line, flush=True)
        fields = line.split()
        if line.startswith("WORKER_TICK "):
            ticks.append({"tick": int(fields[1]), "worker_us": int(fields[2]),
                          "observer_s": time.monotonic() - start})
        elif line.startswith("MAIN_THREAD_ID ") or line.startswith("RENDER_THREAD_ID "):
            thread_ids[fields[0]] = int(fields[1])
        elif line.startswith("FAULT_BEGIN "):
            events[fields[1]] = {"begin_s": time.monotonic() - start, "before": len(ticks)}
        elif line.startswith("FAULT_END "):
            events[fields[1]].update(end_s=time.monotonic() - start, after=len(ticks))
        elif line == "PROCESS_STOP_READY":
            assert resume_at is None
            os.kill(child.pid, signal.SIGSTOP)
            stopped = True
            # Confirm this child reached stopped state before starting the interval.
            for _ in range(100):
                state = subprocess.check_output(["ps", "-o", "state=", "-p", str(child.pid)], text=True).strip()
                if "T" in state:
                    break
                time.sleep(0.005)
            assert "T" in state, state
            events["process"] = {"begin_s": time.monotonic() - start, "pid": child.pid,
                                 "os_state": state}
            resume_at = time.monotonic() + 0.8
            stop_tick_count = None  # Drain output queued before the signal first.

    try:
        while time.monotonic() - start < 35:
            for key, _ in selector.select(0.01):
                chunk = os.read(key.fd, 65536)
                if not chunk:
                    selector.unregister(key.fileobj)
                    continue
                buffer += chunk
                while b"\n" in buffer:
                    line, buffer = buffer.split(b"\n", 1)
                    line_received(line.decode(errors="replace").rstrip())
            if stopped and time.monotonic() > resume_at - 0.6:
                if stop_tick_count is None:
                    stop_tick_count = len(ticks)
                assert len(ticks) == stop_tick_count, "worker emitted a tick while OS reports stopped"
            if stopped and time.monotonic() >= resume_at:
                events["process"].update(end_s=time.monotonic() - start,
                                         worker_ticks_during_quiet_interval=len(ticks) - stop_tick_count)
                os.kill(child.pid, signal.SIGCONT)
                stopped = False
            if child.poll() is not None and not selector.get_map():
                break
        else:
            raise TimeoutError("Godot fault capture exceeded 35 seconds")
        assert child.returncode == 0, child.returncode
        assert [t["tick"] for t in ticks] == list(range(180))
        for kind in ("main", "render"):
            event = events[kind]
            event["worker_ticks_during_stall"] = event["after"] - event["before"]
            assert event["end_s"] - event["begin_s"] >= 0.75, event
            assert event["worker_ticks_during_stall"] >= 10, event
        assert events["process"]["worker_ticks_during_quiet_interval"] == 0
        assert thread_ids["MAIN_THREAD_ID"] != thread_ids["RENDER_THREAD_ID"], thread_ids
        assert any(line.startswith("FAULT_RUNTIME_OK") for line in lines)
        assert any(line.startswith("SCHEDULE_CAPTURE_OK") for line in lines)
        report = {"events": events, "threads": thread_ids, "ticks": ticks,
                  "repeated_render_sync_warnings": sync_warnings,
                  "scope": "main thread delay, process SIGSTOP, render thread delay; no driver hang"}
        (ROOT / "58_faults.json").write_text(json.dumps(report, indent=2) + "\n")
        print("FAULT_CONTROLLER_OK main=progress render=progress process=paused recovered=exact")
    finally:
        if child.poll() is None:
            if stopped:
                os.kill(child.pid, signal.SIGCONT)
            child.terminate()
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
        selector.close()
        lines.append(f"REPEATED_RENDER_SYNC_WARNINGS {sync_warnings}")
        (ROOT / "59_fault_run.log").write_text("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
