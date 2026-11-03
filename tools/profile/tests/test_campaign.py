"""The campaign's parser and report on synthetic log lines (no game, no game data)."""
import sys
import tomllib
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

import campaign  # noqa: E402


def at(secs: float, text: str) -> str:
    h, rest = divmod(secs, 3600)
    m, s = divmod(rest, 60)
    return f"2026-11-03T{int(h):02d}:{int(m):02d}:{s:09.6f}Z  INFO nvforever::graphics: {text}"


LOG = [
    at(36000, "frame time: 40.00 ms mean, 900.0 ms slowest over 100 frames; graphics: enhanced"),
    at(36005, "profile: 5.0 s, 100 frames, frame mean 40.0 ms; per frame ms: Main 30.00, loading 20.00"),
    at(36020, "frame time: 10.00 ms mean, 14.0 ms slowest over 500 frames; graphics: enhanced"),
    at(36020, "profile: 5.0 s, 500 frames, frame mean 10.0 ms, cpu 50.0 ms a frame; per frame ms: Main 8.00, Render 9.00, "
              "phys::tick 1.50, gpu_preprocessing::write_binned_instance_buffers<bevy_pbr::Shadow, Opaque> 1.20, update 9.90"),
    at(36020, "profile gpu: ms a frame: per_view_shadow_pass 1.80, main_opaque_pass_3d 1.20"),
    at(36020, "profile scene: 5000 mesh entities (3000 visible, 300 moved this frame), 2000 distinct meshes, 1200 distinct materials in use (1900 material assets)"),
    at(36020, "profile transforms: 900 of 20000 written this frame (400 with a mesh; 3 not yet propagated), most: Bip 300"),
    at(36020, "profile shadows: casters per cascade, nearest first: light 12v3: 100/700/1500"),
    at(36025, "frame time: 12.00 ms mean, 20.0 ms slowest over 400 frames; graphics: enhanced"),
    at(36025, "profile: 5.0 s, 400 frames, frame mean 12.0 ms, cpu 54.0 ms a frame; per frame ms: Main 9.00, Render 11.00, phys::tick 2.50"),
    at(36025, "profile gpu: ms a frame: per_view_shadow_pass 2.20, main_opaque_pass_3d 1.20"),
    at(36026, "profile hitch: 150.0 ms; spans ms: update 149.00, world::spawn_cells 120.00 (3), Main 140.00; pipelines 900 (12 new since the frame before, 0 waiting)"),
    at(36027, "profile hitch (behind a loading screen): 600.0 ms; spans ms: world::spawn_cells 500.00 (1); pipelines 900 (0 new since the frame before, 0 waiting)"),
    at(36028, "stress sample: t 28.0 s, frames 400, frame mean 12.00 ms, frame worst 20.00 ms, entities 9000, refs drawn 1500, meshes 5000, images 2000, materials 1900, models cached 600, textures cached 1500, audio 3, cells 25, bodies 900, actors 42, nan actors 0, ragdolls 0, projectiles 0, at (100, 200, 300)"),
]


class Parse(unittest.TestCase):
    def setUp(self):
        self.p = campaign.parse(LOG, warmup=15)

    def test_the_warm_up_is_left_out(self):
        self.assertEqual(self.p["periods"], 2)
        self.assertEqual(self.p["frame_mean"], 11.0)
        self.assertEqual(self.p["frame_worst_period"], 12.0)
        self.assertEqual(self.p["slowest_frame"], 20.0)
        self.assertNotIn("loading", self.p["spans"])

    def test_spans_are_averaged_and_wrappers_kept_apart(self):
        self.assertAlmostEqual(self.p["spans"]["phys::tick"], 2.0)
        self.assertNotIn("update", self.p["spans"])
        self.assertNotIn("Main", self.p["spans"])
        self.assertEqual(self.p["split"]["Main"], 8.5)
        self.assertEqual(self.p["split"]["Render"], 10.0)
        # A generic type's comma does not split the name.
        self.assertIn("gpu_preprocessing::write_binned_instance_buffers<bevy_pbr::Shadow, Opaque>", self.p["spans"])
        self.assertEqual(self.p["cpu_ms"], 52.0)

    def test_gpu_scene_and_counts(self):
        self.assertAlmostEqual(self.p["gpu"]["per_view_shadow_pass"], 2.0)
        self.assertAlmostEqual(self.p["gpu_total"], 3.2)
        self.assertEqual(self.p["scene"]["visible"], 3000)
        self.assertEqual(self.p["transforms"], {"written": 900, "of": 20000, "with_mesh": 400})
        self.assertEqual(self.p["shadow_casters"], "light 12v3: 100/700/1500")
        self.assertEqual(self.p["actors"], 42)
        self.assertEqual(self.p["last_at"], "(100, 200, 300)")

    def test_hitches_seen_and_behind_loading_screens(self):
        self.assertEqual(len(self.p["hitches"]), 1)
        h = self.p["hitches"][0]
        self.assertEqual(h["ms"], 150.0)
        self.assertEqual(h["spans"][0], ("world::spawn_cells", 120.0))
        self.assertEqual(h["new_pipelines"], 12)
        self.assertEqual(h["t"], 26.0)
        self.assertEqual(self.p["loading_hitches"], 1)
        # What grew in the slow frame against the mean frame (phys::tick: 2.0 a frame, none here).
        self.assertEqual(self.p["slow_frame_excess"]["world::spawn_cells"], 120.0)
        self.assertNotIn("update", self.p["slow_frame_excess"])

    def test_hitches_in_the_warm_up_are_left_out(self):
        early = [at(36000, "frame time: 40.00 ms mean, 900.0 ms slowest over 100 frames; graphics: enhanced"),
                 at(36003, "profile hitch: 500.0 ms; spans ms: world::spawn_cells 400.00 (3); pipelines 1 (1 new since the frame before, 0 waiting)")]
        self.assertEqual(campaign.parse(early, warmup=15)["hitches"], [])
        # Lines before the first frame-time line (start-up) are not counted either.
        before = [at(36000, "frame time: 10.00 ms mean, 20.0 ms slowest over 100 frames; graphics: enhanced"),
                  at(35998, "profile hitch: 500.0 ms; spans ms: world::spawn_cells 400.00 (3)")]
        self.assertEqual(campaign.parse(before, warmup=15)["hitches"], [])

    def test_an_empty_log(self):
        p = campaign.parse([], warmup=15)
        self.assertIsNone(p["frame_mean"])
        self.assertEqual(p["hitches"], [])


class Report(unittest.TestCase):
    def test_report_ranks_scenarios_and_spans(self):
        a = {**campaign.parse(LOG, 15), "name": "a", "group": "stand", "status": "ok", "command": "x", "others_cpu": 0.01}
        b = {**a, "name": "b", "frame_mean": 30.0, "spans": {"phys::tick": 9.0}}
        text = "\n".join(campaign.report({"name": "run", "exe": "nvf", "scenarios": [a, b]}))
        self.assertLess(text.index("| b |"), text.index("| a |"))
        self.assertIn("| `phys::tick` | 9.00 | b | 2 |", text)
        self.assertIn("**a** at 26 s: 150 ms; world::spawn_cells 120", text)


class Scenarios(unittest.TestCase):
    def test_the_scenario_file(self):
        with open(HERE.parent / "campaign.toml", "rb") as f:
            doc = tomllib.load(f)
        names = [s["name"] for s in doc["scenario"]]
        self.assertEqual(len(names), len(set(names)), "names are unique")
        groups = {s["group"] for s in doc["scenario"]}
        self.assertEqual(groups, {"stand", "crowd", "interior", "combat", "move", "stream"})
        for s in doc["scenario"]:
            self.assertTrue("args" in s or "battle" in s, s["name"])
            args, aids, secs = campaign.launch_args(doc["defaults"], s)
            self.assertIn("--log-frame-times", args)
            self.assertIn("NVF_PLAY_TEST_PROFILE", aids)
            self.assertGreater(secs, doc["defaults"]["warmup"])


if __name__ == "__main__":
    unittest.main()
