import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("relay", Path(__file__).with_name("75_udp_lab.py"))
relay = importlib.util.module_from_spec(spec)
spec.loader.exec_module(relay)


class PolicyTests(unittest.TestCase):
    def test_fault_boundaries(self):
        self.assertEqual([relay.policy(*args) for args in [
            (0, 17, -1), (0, 1, 3049), (0, 1, 3050), (1, 1, 3050),
            (0, 17, 3500), (0, 1, 4070), (0, 2, 5000), (0, 3, 5000),
        ]], [(0, False), (30, False), (1020, False), (30, False),
             (570, True), (30, False), (40, False), (20, False)])


if __name__ == "__main__":
    unittest.main()
