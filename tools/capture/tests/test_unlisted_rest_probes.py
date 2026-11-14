"""The #2326 probe sets rebuild the jobs that ran in the original (unlisted_rest_probes.py)."""
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

import jip_probes as jp  # noqa: E402
import unlisted_rest_probes as urp  # noqa: E402

# Steps per job as run on 2026-11-13 (the capture's job files).
COUNTS = {"json1": 52, "json2": 33, "knv1": 64, "knv2": 49, "knv3": 27, "rest4": 68, "rest5": 59}
COUNTS.update({f"json2r{i}": 4 for i in range(6)})
COUNTS.update({f"knv2r{i}": 1 for i in range(3)})
COUNTS.update({f"rest4r{i}": 2 for i in range(6)})
COUNTS.update({f"rest5r{i:02d}": 2 for i in range(12)})


class UnlistedRestProbes(unittest.TestCase):
    def test_every_job_that_ran_is_a_set(self):
        self.assertEqual({k: len(v) for k, v in urp.SETS.items()}, COUNTS)
        for name in COUNTS:
            self.assertIn(name, jp.SETS)
            self.assertIn(name, jp.WRITING_SETS)

    def test_labels_are_unique_within_a_set(self):
        for name, rows in urp.SETS.items():
            ids = [pid for pid, *_ in rows]
            self.assertEqual(len(ids), len(set(ids)), name)

    def test_risky_jobs_start_as_their_set_does(self):
        # A rerun places Doc Mitchell (rest4/rest5) or caches the file (json2) first.
        for name, rows in urp.SETS.items():
            if name.startswith(("rest4r", "rest5r")):
                self.assertEqual(rows[0][1], urp.SETS[name[:5]][0][1], name)
            if name.startswith("json2r"):
                self.assertEqual(rows[0][0], f"json2.{name}.cache", name)

    def test_shape_run(self):
        self.assertEqual(len(urp.SHAPE_NAMES), 205)
        self.assertEqual(len(urp.SHAPE_PATTERNS), 23)
        steps = urp.shape_steps()
        self.assertEqual(len(steps), 4715)
        self.assertTrue(all("if 0\n" in s["query"] for s in steps), "compiled, never run")
        with tempfile.TemporaryDirectory() as d:
            paths = urp.shape_jobs(d, "fnv.capture.steps/1")
            self.assertEqual([os.path.basename(p) for p in paths[:2]], ["shp00.steps.json", "shp01.steps.json"])
            self.assertEqual(len(paths), 32)


if __name__ == "__main__":
    unittest.main()
