"""Fail-closed parser and orchestration checks; no model or engine build required."""

import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("bench_compare", Path(__file__).parents[1] / "bench_compare.py")
bench = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bench)
FEN = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w"


def log(score="cp 42", nodes=10, total=None):
    return f"""Position: 1/1 ({FEN})
info depth 2 score cp 0 nodes 1 time 1 nps 1000 pv b0c2
info depth 13 seldepth 17 nodes {nodes} score {score} nps 1000 time 10 pv b0c2 b9c7
bestmove b0c2 ponder b9c7
===========================
Total time (ms) : 10
Nodes searched  : {nodes if total is None else total}
Nodes/second    : 1000
"""


class ParseTests(unittest.TestCase):
    def test_official_expanded_fen_and_original_corpus_are_equivalent(self):
        original = bench.parse_bench(log(), 13, [FEN])
        expanded = bench.parse_bench(log().replace(FEN, FEN + " - - 0 1"), 13, [FEN])
        self.assertEqual(bench.differences(original, expanded), [])
        with self.assertRaises(bench.BenchError):
            bench.parse_bench(log().replace(FEN, FEN + " - - 3 1"), 13, [FEN])

    def test_cp_and_mate_remain_distinct(self):
        cp = bench.parse_bench(log(), 13, [FEN])
        mate = bench.parse_bench(log("mate -3"), 13, [FEN])
        self.assertEqual(mate["positions"][0]["score"], {"kind": "mate", "value": -3})
        self.assertEqual(bench.differences(cp, mate)[0]["field"], "score")

    def test_different_field_order_and_pv_tail(self):
        original = bench.parse_bench(log(), 13, [FEN])
        changed = bench.parse_bench(log().replace("nodes 10 score cp 42", "score cp 42 nodes 10")
                                   .replace("pv b0c2 b9c7", "pv b0c2 b9a7"), 13, [FEN])
        self.assertEqual(bench.differences(original, changed)[0]["field"], "pv")

    def test_incomplete_or_ambiguous_records_fail(self):
        for changed in (
            "", log().replace("Position: 1/1", "Position: 2/1"),
            log().replace(FEN, "another FEN"), log().replace("depth 13", "depth 12"),
            log().replace("score cp 42", "score cp 42 lowerbound"),
            log().replace("score cp 42", "score cp 42 upperbound"),
            log().replace("score cp 42", ""),
            log().replace("bestmove b0c2 ponder b9c7\n", ""),
            log() + "bestmove b0c2\n", log().replace("bestmove b0c2", "bestmove 0000"),
            log().replace("bestmove b0c2", "bestmove h0g2"),
            log().replace("pv b0c2 b9c7", "pv nonsense"),
            log().replace("pv b0c2 b9c7", ""),
            log().replace("nodes 10", "nodes 10 nodes 10"),
            log(nodes=0), log(total=11), log().replace("Total time (ms) : 10", ""),
            log() + "Nodes searched : 10\n", log() + "info string error: failed\n",
        ):
            with self.subTest(log=changed):
                with self.assertRaises(bench.BenchError):
                    bench.parse_bench(changed, 13, [FEN])

    def test_timing_is_not_an_identity_field(self):
        left = bench.parse_bench(log(), 13, [FEN])
        right = bench.parse_bench(log().replace("time 10", "time 50").replace("nps 1000", "nps 200"), 13, [FEN])
        self.assertEqual(bench.differences(left, right), [])

    def test_generic_reference_cannot_masquerade_as_avx2(self):
        info = dict(architecture="x86_64", nnue_backend="AVX2")
        self.assertEqual(bench.select_reference_arch(info, None), "x86-64-avx2")
        with self.assertRaises(bench.BenchError):
            bench.select_reference_arch(info, "x86-64")
        with self.assertRaises(bench.BenchError):
            bench.select_reference_arch(dict(architecture="x86_64", nnue_backend="Scalar"), None)


class OrchestrationTests(unittest.TestCase):
    def test_three_rounds_alternate_and_validate_every_result(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            args = bench.argparse.Namespace(rounds=3, depth=13, hash=16, cpu=None, timeout=1)
            observed = []

            def run(command, path, **kwargs):
                observed.append(Path(command[0]).name)
                path.write_text(log())
                return 0.25

            report = {}
            with patch.object(bench, "prepare", return_value=(Path("pikarust"), Path("pikafish"), [FEN])), patch.object(bench, "run_logged", side_effect=run), contextlib.redirect_stdout(io.StringIO()):
                bench.compare(args, output, report)
            self.assertEqual(observed, ["pikarust", "pikafish", "pikafish", "pikarust", "pikarust", "pikafish"])
            self.assertEqual(report["status"], "passed")
            self.assertEqual(len(report["runs"]), 6)
            self.assertEqual(report["summary"]["candidate_reference_nps_ratio"], 1)

    def test_cross_engine_difference_fails_before_performance_summary(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            args = bench.argparse.Namespace(rounds=3, depth=13, hash=16, cpu=None, timeout=1)

            def run(command, path, **kwargs):
                path.write_text(log(nodes=10 if command[0] == "pikarust" else 11))
                return 0.25

            report = {}
            with patch.object(bench, "prepare", return_value=(Path("pikarust"), Path("pikafish"), [FEN])), patch.object(bench, "run_logged", side_effect=run), contextlib.redirect_stdout(io.StringIO()):
                with self.assertRaises(bench.BenchError):
                    bench.compare(args, output, report)
            self.assertEqual(report["differences"][0]["field"], "nodes")
            self.assertEqual(len(report["runs"]), 2)
            self.assertNotIn("summary", report)

    def invoke(self, directory, *, failure=None):
        def compare(args, output, report):
            report["runs"] = []
            if failure:
                raise bench.BenchError(failure)
            report["status"] = "passed"
        with patch.object(bench, "compare", side_effect=compare), contextlib.redirect_stderr(io.StringIO()), contextlib.redirect_stdout(io.StringIO()):
            return bench.main(["--output", str(directory)])

    def test_failure_produces_nonzero_and_report(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "run"
            self.assertEqual(self.invoke(output, failure="missing final info"), 1)
            report = json.loads((output / "report.json").read_text())
            self.assertEqual(report["status"], "failed")
            self.assertIn("missing final info", report["error"])

    def test_existing_directory_is_never_modified(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            self.assertEqual(self.invoke(output), 1)
            self.assertEqual(list(output.iterdir()), [])
            report = output / "report.json"
            report.write_text("existing evidence")
            self.assertEqual(self.invoke(output), 1)
            self.assertEqual(report.read_text(), "existing evidence")

    def test_missing_model_fails_before_build(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "run"
            with patch.dict(bench.os.environ, {"PIKARUST_NNUE_MODEL": str(output / "missing.nnue")}), patch.object(bench, "run_logged") as run, contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(bench.main(["--output", str(output)]), 1)
                run.assert_not_called()
            self.assertEqual(json.loads((output / "report.json").read_text())["status"], "failed")

    def test_nonzero_process_is_not_parsed_as_success(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(bench.BenchError):
                bench.run_logged([bench.sys.executable, "-c", "raise SystemExit(9)"], Path(directory) / "failed.log")

    def test_timeout_is_a_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(bench.BenchError):
                bench.run_logged([bench.sys.executable, "-c", "import time; time.sleep(5)"], Path(directory) / "timeout.log", timeout=0.01)


if __name__ == "__main__":
    unittest.main()
