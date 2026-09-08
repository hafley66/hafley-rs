"""Live renderer lifecycle proof. Owns only spawned renderer and relay children."""
import json
import os
from pathlib import Path
import signal
import struct
import subprocess
import sys
import time

LAB = Path(__file__).resolve().parent
BINARY = LAB / "target/debug/falcon-rollback"


def records(path):
    if not path.exists():
        return []
    lines = path.read_text().splitlines(keepends=True)
    return [json.loads(line) for line in lines if line.endswith("\n")]


def progress():
    result = []
    for peer in range(2):
        try:
            result.append(int(Path(f"progress-{peer}").read_text()))
        except (FileNotFoundError, ValueError):
            return None
    return result


def digest(rows):
    value = 0xcbf29ce484222325
    for row in rows:
        for byte in struct.pack("<27d", row["tick"], row["kind"], row["entity"], *row["values"]):
            value = ((value ^ byte) * 0x100000001b3) & ((1 << 64)-1)
    return f"{value:016x}"


def verify_acks(pids):
    source = json.loads(Path("peer-1.json").read_text())
    summaries = []
    for index, pid in enumerate(pids):
        acks = records(Path(f"acks-{index}.jsonl"))
        assert acks
        generations = [a["source_generation"] for a in acks]
        assert generations == sorted(set(generations))
        for ack in acks:
            assert ack["source_pid"] == source["pid"] != pid
            assert ack["consumer_pid"] == pid
            assert ack["ipc_sql_exact"] and ack["row_roundtrip_exact"] and ack["mesh_roundtrip_exact"]
            assert ack["row_digest"] == digest(source["frames"][ack["published_tick"]]["rows"])
        summaries.append(dict(pid=pid, acks=len(acks), first_tick=acks[0]["published_tick"],
                              last_tick=acks[-1]["published_tick"], skipped=sum(a["skipped_generations"] for a in acks)))
    assert summaries[-1]["last_tick"] == 199
    return summaries


def verify_archive():
    report = json.loads(Path("live-verification.json").read_text())
    subprocess.run([str(BINARY), "--process-video", "--verify-only"], check=True)
    assert verify_acks([r["pid"] for r in report["renderers"]]) == report["renderers"]
    if report["lifecycle"]:
        pause = report["events"]["pause"]
        assert min(pause["advanced"]) >= 15 and pause["duration_ms"] >= 800
        assert pause["resumed"]["published_tick"] >= pause["after"][1]
        assert report["events"]["restart"]["first_ack"]["renderer_generation"] == 1
        assert report["renderers"][2]["first_tick"] == 199
    print("ARCHIVE_OK full_states=360 corrected_pairs=200 source_rows_and_mesh_acknowledgements=exact")


def main(lifecycle):
    root = Path.cwd()
    renderers, logs = [], []
    relay = None
    stopped = None
    events = {}
    start = time.monotonic()

    def note(message):
        Path("note.pending").write_text(message)
        Path("note.pending").replace("note.txt")

    def spawn_renderer():
        index = len(renderers)
        log = open(f"godot-{index}.log", "wb")
        logs.append(log)
        child = subprocess.Popen([
            "godot", "--path", str(LAB / "godot"), "--fixed-fps", "60",
            "--disable-vsync", "--quit-after", "1800", "--write-movie", str(root / f"godot-{index}.avi"),
            "--", "--external"], stdout=log, stderr=log, env={**os.environ,
                "FALCON_LIVE_PATH": str(root / "live-1.bin"),
                "FALCON_LIVE_AUDIT": str(root / f"acks-{index}.jsonl"),
                "FALCON_LIVE_NOTE": str(root / "note.txt"),
                "FALCON_LIVE_STOP": str(root / f"stop-{index}"), "RUST_LOG": "warn"})
        renderers.append(child)
        return child

    try:
        note("LIVE ATTACH / UDP DELAY + JITTER + PACKET LOSS")
        spawn_renderer()
        while "GDEXT_STAGE_READY" not in Path("godot-0.log").read_text(errors="replace"):
            assert renderers[0].poll() is None, "Godot failed before readiness"
            assert time.monotonic()-start < 15, "Godot readiness timeout"
            time.sleep(.02)
        relay_log = open("relay.log", "wb")
        logs.append(relay_log)
        relay = subprocess.Popen([sys.executable, str(LAB / "75_udp_lab.py"), str(BINARY)],
            stdout=relay_log, stderr=relay_log, env={**os.environ, "FALCON_LIVE_DIR": str(root)})
        while True:
            assert time.monotonic()-start < 40, "live lifecycle watchdog"
            assert relay.poll() in (None, 0), "relay or peer failed"
            for renderer in renderers:
                assert renderer.poll() in (None, 0), f"Godot {renderer.pid} failed"
            first = records(Path("acks-0.jsonl"))
            ticks = progress()
            if lifecycle and first and first[-1]["published_tick"] >= 50 and "pause" not in events:
                os.kill(renderers[0].pid, signal.SIGSTOP)
                stopped = renderers[0]
                state = subprocess.check_output(["ps", "-o", "state=", "-p", str(stopped.pid)], text=True).strip()
                assert "T" in state, state
                events["pause"] = dict(pid=stopped.pid, os_state=state, before=progress(),
                    ack_count=len(records(Path("acks-0.jsonl"))), begin=time.monotonic())
            if stopped is not None and time.monotonic()-events["pause"]["begin"] >= .8:
                pause = events["pause"]
                pause["after"] = progress()
                pause["duration_ms"] = (time.monotonic()-pause["begin"])*1000
                assert pause["before"] is not None and pause["after"] is not None
                pause["advanced"] = [a-b for a,b in zip(pause["after"],pause["before"])]
                assert min(pause["advanced"]) >= 15
                assert len(records(Path("acks-0.jsonl"))) == pause["ack_count"]
                os.kill(stopped.pid, signal.SIGCONT)
                stopped = None
                note("GODOT PAUSE VERIFIED / PEERS ADVANCED %d + %d TICKS" % tuple(pause["advanced"]))
            if lifecycle and "pause" in events and "after" in events["pause"]:
                pause = events["pause"]
                if len(first) > pause["ack_count"] and "resumed" not in pause:
                    pause["resumed"] = first[pause["ack_count"]]
                    assert pause["resumed"]["published_tick"] >= pause["after"][1]
            if lifecycle and first and first[-1]["published_tick"] >= 120 and "restart" not in events:
                events["restart"] = dict(before=progress(), old_pid=renderers[0].pid)
                Path("stop-0").touch()
            if lifecycle and "restart" in events and len(renderers) == 1 and renderers[0].poll() == 0:
                note("NEW GODOT PROCESS / ATTACHING TO LATEST SOURCE GENERATION")
                events["restart"]["new_pid"] = spawn_renderer().pid
            if lifecycle and len(renderers) == 2:
                second = records(Path("acks-1.jsonl"))
                if second and "first_ack" not in events["restart"]:
                    events["restart"]["first_ack"] = second[0]
                    events["restart"]["after"] = progress()
                    assert second[0]["renderer_generation"] == 1
                    assert second[0]["published_tick"] >= events["restart"]["before"][1]
                    assert second[0]["published_tick"] < 199, "restart missed the live timeline"
            if relay.poll() == 0 and all(r.poll() == 0 for r in renderers):
                if lifecycle and len(renderers) == 2:
                    note("PEERS HAVE EXITED / COLD ATTACH TO FINAL SNAPSHOT")
                    events["cold_attach"] = dict(peer_exit_verified=True, renderer_pid=spawn_renderer().pid)
                else:
                    break
            time.sleep(.01)
        subprocess.run([str(BINARY), "--process-video", "--verify-only"], check=True)
        summaries = verify_acks([renderer.pid for renderer in renderers])
        if lifecycle:
            assert len(renderers) == 3
            assert len({r.pid for r in renderers}) == 3
            assert summaries[2]["first_tick"] == 199 and summaries[2]["acks"] == 1
        Path("live-verification.json").write_text(json.dumps(dict(lifecycle=lifecycle, events=events, renderers=summaries),indent=2))
        print(json.dumps(summaries))
    finally:
        if stopped is not None and stopped.poll() is None:
            os.kill(stopped.pid, signal.SIGCONT)
        for child in renderers + ([relay] if relay else []):
            if child.poll() is None:
                child.terminate()
        for child in renderers + ([relay] if relay else []):
            try:
                child.wait(timeout=3)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
        for log in logs:
            log.close()


if __name__ == "__main__":
    if "--verify-archive" in sys.argv:
        verify_archive()
    else:
        main("--lifecycle" in sys.argv)
