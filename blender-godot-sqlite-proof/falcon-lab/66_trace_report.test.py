import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("report", Path(__file__).with_name("66_trace_report.py"))
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)


class ReportTests(unittest.TestCase):
    def test_units(self):
        self.assertEqual([report.milliseconds(x) for x in
                          ["1000ns", "1µs", "1us", "1ms", "1s"]],
                         [.001, .001, .001, 1, 1000])
        with self.assertRaises(ValueError):
            report.milliseconds("unknown")

    def test_close_events_and_delivery_bucket(self):
        rows = ['DECODE fixture', '{}',
                '{"target":"falcon::runtime","span":{"name":"advance","tick":97},'
                '"fields":{"message":"close","time.busy":"2ms"}}',
                '{"target":"falcon::runtime","span":{"name":"advance","tick":96},'
                '"fields":{"message":"close","time.busy":"500µs"}}']
        self.assertEqual(dict(report.summarize(rows)), {
            "falcon::runtime::advance": [2, .5],
            "runtime bucket: delivery tick 97": [2],
            "runtime bucket: other ticks": [.5],
        })


if __name__ == "__main__":
    unittest.main()
