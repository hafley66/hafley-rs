import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("controller", Path(__file__).with_name("84_live_controller.py"))
controller = importlib.util.module_from_spec(spec)
spec.loader.exec_module(controller)


class AuditTests(unittest.TestCase):
    def test_exact_ack_and_changed_source_detection(self):
        original = Path.cwd()
        with tempfile.TemporaryDirectory(prefix="falcon-live-audit-") as directory:
            try:
                os.chdir(directory)
                row = dict(tick=199, kind=0, entity=0, values=[0.0]*24)
                source = dict(pid=10, frames=[None]*199+[dict(rows=[row])])
                Path("peer-1.json").write_text(json.dumps(source))
                ack = dict(source_pid=10, consumer_pid=20, source_generation=200,
                           published_tick=199, skipped_generations=199, ipc_sql_exact=True,
                           row_roundtrip_exact=True, mesh_roundtrip_exact=True,
                           row_digest=controller.digest([row]))
                Path("acks-0.jsonl").write_text(json.dumps(ack)+"\n"+'{"partial":')
                self.assertEqual(controller.verify_acks([20]), [dict(
                    pid=20,acks=1,first_tick=199,last_tick=199,skipped=199)])
                row["values"][0] = 1.0
                Path("peer-1.json").write_text(json.dumps(source))
                with self.assertRaises(AssertionError):
                    controller.verify_acks([20])
            finally:
                os.chdir(original)


if __name__ == "__main__":
    unittest.main()
