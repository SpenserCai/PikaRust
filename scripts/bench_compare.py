#!/usr/bin/env python3
"""Build and compare the continuous official bench; Python standard library only."""

import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import statistics
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
MOVE = r"[a-i][0-9][a-i][0-9]"
# Only build-related settings are recorded. Never dump the process environment.
BUILD_ENV = (
    "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_RUSTFLAGS",
    "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_TARGET",
    "CXX", "CXXFLAGS", "CPPFLAGS", "LDFLAGS", "COMP", "BUILD_JOBS",
)


class BenchError(Exception):
    """A missing prerequisite, incomplete result, or comparison failure."""


def require(condition, message):
    if not condition:
        raise BenchError(message)


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def command_output(command, *, cwd=ROOT, input_text=None):
    result = subprocess.run(command, cwd=cwd, input=input_text, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
    require(result.returncode == 0,
            f"Command failed ({result.returncode}): {command!r}: {result.stderr[-2000:]}")
    return result.stdout.strip()


def run_logged(command, log, *, cwd=ROOT, env=None, timeout=1200):
    start = time.monotonic()
    with log.open("w") as output:
        try:
            result = subprocess.run(command, cwd=cwd, env=env, stdout=output,
                                    stderr=subprocess.STDOUT, timeout=timeout)
        except subprocess.TimeoutExpired as error:
            raise BenchError(f"Command timed out after {timeout}s; see {log}") from error
    require(result.returncode == 0,
            f"Command failed ({result.returncode}): {command!r}; see {log}")
    return time.monotonic() - start


def read_pin():
    values = {}
    for line in (ROOT / "scripts/reference.lock").read_text().splitlines():
        if line and not line.startswith("#"):
            key, value = line.split("=", 1)
            values[key] = value
    for key in ("PIKAFISH_REPOSITORY", "PIKAFISH_COMMIT", "PIKAFISH_NNUE_SHA256"):
        require(key in values, f"Missing {key} in reference.lock")
    return values


def expected_fens(source):
    text = (source / "src/benchmark.cpp").read_text()
    corpus = re.search(r"const std::vector<std::string> Defaults\s*=\s*\{(.*?)\};", text, re.S)
    require(corpus is not None, "Pinned benchmark.cpp has no default corpus")
    fens = re.findall(r'"([^"\n]+)"', corpus[1])
    require(len(fens) == 49, f"Expected 49 official default positions, got {len(fens)}")
    return fens


def field(line, name):
    matches = re.findall(rf"(?:^|\s){re.escape(name)}\s+(\d+)(?=\s|$)", line)
    require(len(matches) == 1, f"Expected one {name} field: {line}")
    return int(matches[0])


def canonical_fen(fen):
    # Official bench prints defaults expanded by Position::fen(); Rust accepts
    # the original two-field corpus too. Preserve all explicitly given clocks.
    fields = fen.split()
    if len(fields) == 2:
        fields.extend(("-", "-", "0", "1"))
    return " ".join(fields)


def parse_bench(text, depth, fens):
    """Reject missing/truncated/bounded output instead of accepting a partial bench."""
    require(not re.search(r"\b(?:error|failed)\b", text, re.I),
            "Engine log contains an error; inspect the complete log")
    headers = list(re.finditer(r"^Position: (\d+)/(\d+) \(([^\n]+)\)\s*$", text, re.M))
    require(len(headers) == len(fens),
            f"Expected {len(fens)} position records, got {len(headers)}")
    positions = []
    for index, header in enumerate(headers):
        require((int(header[1]), int(header[2]), canonical_fen(header[3])) ==
                (index + 1, len(fens), canonical_fen(fens[index])),
                f"Position {index + 1}: index, count, or FEN differs from pinned corpus")
        end = headers[index + 1].start() if index + 1 < len(headers) else len(text)
        section = text[header.end():end]
        infos = [line for line in section.splitlines() if line.startswith("info depth ")]
        require(bool(infos), f"Position {index + 1}: missing final info")
        final = infos[-1]
        require(field(final, "depth") == depth,
                f"Position {index + 1}: search did not complete depth {depth}")
        require(not re.search(r"\b(?:lowerbound|upperbound)\b", final),
                f"Position {index + 1}: final score is a bound")
        scores = re.findall(r"\bscore (cp|mate) (-?\d+)(?=\s|$)", final)
        require(len(scores) == 1, f"Position {index + 1}: missing or ambiguous score")
        pv_match = re.search(r"\bpv (.+)$", final)
        require(pv_match is not None, f"Position {index + 1}: missing PV")
        pv = pv_match[1].split()
        require(all(re.fullmatch(MOVE, move) for move in pv),
                f"Position {index + 1}: malformed PV")
        best_lines = [line for line in section.splitlines() if line.startswith("bestmove ")]
        require(len(best_lines) == 1, f"Position {index + 1}: expected one bestmove")
        best = re.fullmatch(rf"bestmove ({MOVE})(?: ponder ({MOVE}))?", best_lines[0])
        require(best is not None, f"Position {index + 1}: malformed bestmove")
        require(best[1] == pv[0], f"Position {index + 1}: bestmove and PV disagree")
        nodes = field(final, "nodes")
        require(nodes > 0, f"Position {index + 1}: empty search")
        positions.append(dict(index=index + 1, fen=canonical_fen(fens[index]), depth=depth,
                              nodes=nodes, bestmove=best[1],
                              score=dict(kind=scores[0][0], value=int(scores[0][1])),
                              pv=pv, time_ms=field(final, "time"), nps=field(final, "nps")))
    summary = {}
    for label, key in (("Total time (ms)", "time_ms"), ("Nodes searched", "nodes"),
                       ("Nodes/second", "nps")):
        values = re.findall(rf"^{re.escape(label)}\s*:\s*(\d+)\s*$", text, re.M)
        require(len(values) == 1, f"Missing or duplicate summary: {label}")
        summary[key] = int(values[0])
        require(summary[key] > 0, f"Invalid zero summary: {label}")
    require(summary["nodes"] == sum(position["nodes"] for position in positions),
            "Summary nodes differ from per-position total")
    summary["positions"] = positions
    return summary


def differences(expected, actual):
    result = []
    require(len(expected["positions"]) == len(actual["positions"]), "Different corpus lengths")
    for left, right in zip(expected["positions"], actual["positions"]):
        for key in ("index", "fen", "depth", "nodes", "bestmove", "score", "pv"):
            if left[key] != right[key]:
                result.append(dict(position=left["index"], field=key,
                                   expected=left[key], actual=right[key]))
    return result


def select_reference_arch(build, requested):
    # The Rust binary reports the backend actually selected on this CPU.
    supported = {("x86_64", "AVX2"): "x86-64-avx2", ("aarch64", "NEON"): "armv8"}
    expected = supported.get((build["architecture"], build["nnue_backend"]))
    require(expected is not None,
            f"No matching reference configuration for {build}; performance comparison unsupported")
    require(requested in (None, expected),
            f"Reference architecture {requested!r} does not match candidate backend; use {expected}")
    return expected


def cpu_metadata(cpu):
    metadata = dict(platform=platform.platform(), machine=platform.machine(),
                    logical_cpus=os.cpu_count(), requested_cpu=cpu)
    if hasattr(os, "sched_getaffinity"):
        allowed = sorted(os.sched_getaffinity(0))
        metadata["allowed_cpus"] = allowed
        require(cpu is None or cpu in allowed, f"CPU {cpu} is outside allowed affinity {allowed}")
    if Path("/proc/cpuinfo").is_file():
        first = Path("/proc/cpuinfo").read_text().split("\n\n", 1)[0]
        metadata["cpu"] = dict(line.split(":", 1) for line in first.splitlines() if ":" in line)
        metadata["cpu"] = {key.strip(): value.strip() for key, value in metadata["cpu"].items()}
    elif platform.system() == "Darwin":
        metadata["cpu"] = command_output(["sysctl", "-n", "machdep.cpu.brand_string"])
    else:
        metadata["cpu"] = platform.processor()
    if cpu is not None:
        require(platform.system() == "Linux" and shutil.which("taskset") is not None,
                "--cpu requires Linux taskset; requested affinity cannot be ignored")
    return metadata


def write_report(path, report):
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(report, indent=2) + "\n")
    temporary.replace(path)


def candidate_provenance(output):
    revision = command_output(["git", "rev-parse", "HEAD"])
    status = command_output(["git", "status", "--porcelain"])
    diff = subprocess.run(["git", "diff", "HEAD", "--binary"], cwd=ROOT,
                          stdout=subprocess.PIPE, check=True).stdout
    (output / "candidate.patch").write_bytes(diff)
    untracked = command_output(["git", "ls-files", "--others", "--exclude-standard", "-z"])
    return dict(revision=revision, dirty=bool(status), status=status,
                patch_sha256=hashlib.sha256(diff).hexdigest(),
                untracked_sha256={name: sha256(ROOT / name) for name in untracked.split("\0") if name},
                cargo_lock_sha256=sha256(ROOT / "Cargo.lock"),
                rustc=command_output(["rustc", "--version", "--verbose"]),
                cargo=command_output(["cargo", "--version"]))


def prepare(args, output, report):
    pin = read_pin()
    model = Path(os.environ.get("PIKARUST_NNUE_MODEL", ROOT / "models/pikafish.nnue")).resolve()
    require(model.is_file(), f"Missing NNUE model: {model}; run git lfs pull")
    require(sha256(model) == pin["PIKAFISH_NNUE_SHA256"], "NNUE SHA-256 differs from reference.lock")
    report.update(pin=pin, machine=cpu_metadata(args.cpu), candidate=candidate_provenance(output),
                  build_environment={key: os.environ[key] for key in BUILD_ENV if key in os.environ})
    inputs = output / "inputs"
    inputs.mkdir()
    frozen_model = inputs / "pikafish.nnue"
    shutil.copyfile(model, frozen_model)
    require(sha256(frozen_model) == pin["PIKAFISH_NNUE_SHA256"], "NNUE changed during snapshot")
    report["model"] = dict(source=str(model), path=str(frozen_model), sha256=sha256(frozen_model))
    # Isolate this build from concurrent workspace feature combinations / artifacts.
    target = ROOT / "target/bench/build"
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(target)
    env["PIKARUST_NNUE_MODEL"] = str(frozen_model)
    require(not env.get("CARGO_BUILD_TARGET"), "Cross compilation is unsupported for a local performance run")
    build = ["cargo", "build", "--locked", "--release", "--verbose", "-p", "pikarust-bench"]
    report["candidate"]["build_command"] = build
    report["candidate"]["cargo_target_dir"] = str(target)
    print("Building candidate (target/bench/build); see candidate-build.log", flush=True)
    run_logged(build, output / "candidate-build.log", env=env)
    candidate = inputs / "pikarust-bench"
    shutil.copy2(target / "release/pikarust-bench", candidate)
    build_info = json.loads(command_output([str(candidate), "--build-info"]))
    report["candidate"].update(build_info, binary_sha256=sha256(candidate))
    requested = args.reference_arch or os.environ.get("PIKAFISH_ARCH")
    architecture = select_reference_arch(build_info, requested)
    env["PIKAFISH_ARCH"] = architecture
    setup = [str(ROOT / "scripts/setup-pikafish.sh")]
    print(f"Building pinned reference ({architecture}); see reference-build.log", flush=True)
    run_logged(setup, output / "reference-build.log", env=env)
    fixtures = ROOT / "tests/fixtures/pikafish"
    reference_meta = json.loads((fixtures / "reference.json").read_text())
    require(reference_meta["commit"] == pin["PIKAFISH_COMMIT"] and
            reference_meta["nnue_sha256"] == pin["PIKAFISH_NNUE_SHA256"] and
            reference_meta["architecture"] == architecture, "Reference build provenance differs from requested pin/ISA")
    reference = inputs / "pikafish"
    shutil.copy2(fixtures / "bin/pikafish", reference)
    require(sha256(reference) == reference_meta["binary_sha256"], "Reference binary changed after build")
    require(sha256(fixtures / "bin/pikafish.nnue") == sha256(frozen_model), "Reference model differs")
    compiler = command_output([str(reference)], cwd=inputs, input_text="compiler\nquit\n")
    require(re.search(rf"Compilation architecture\s*:\s*{re.escape(architecture)}\s*$", compiler, re.M),
            "Reference binary does not report the requested compilation architecture")
    source = Path(env.get("PIKAFISH_SOURCE_DIR", fixtures / "source")).resolve()
    report["reference"] = dict(reference_meta, repository=pin["PIKAFISH_REPOSITORY"],
                               source=str(source), compiler=compiler, build_command=setup)
    report["comparison_scope"] = (
        "Same NNUE backend ISA, pinned model, single-thread release builds. Compiler implementations "
        "and flags differ; inspect build logs. Timing is observational, not an Elo or speed threshold gate."
    )
    return candidate, reference, expected_fens(source)


def compare(args, output, report):
    candidate, reference, fens = prepare(args, output, report)
    commands = {
        "pikarust": [str(candidate), "bench", "--depth", str(args.depth), "--hash", str(args.hash),
                     "--eval-file", str(output / "inputs/pikafish.nnue")],
        "pikafish": [str(reference), "bench", str(args.hash), "1", str(args.depth), "default", "depth"],
    }
    if args.cpu is not None:
        commands = {name: ["taskset", "-c", str(args.cpu), *command] for name, command in commands.items()}
    report["commands"] = commands
    report["corpus_sha256"] = hashlib.sha256(("\n".join(fens) + "\n").encode()).hexdigest()
    report["runs"] = []
    expected = None
    for trial in range(1, args.rounds + 1):
        order = ("pikarust", "pikafish") if trial % 2 else ("pikafish", "pikarust")
        for engine in order:
            log = output / f"{engine}-{trial}.log"
            print(f"Round {trial}/{args.rounds}: {engine}", flush=True)
            wall = run_logged(commands[engine], log, cwd=output / "inputs", timeout=args.timeout)
            run = dict(engine=engine, round=trial, log=log.name, process_wall_s=wall,
                       **parse_bench(log.read_text(), args.depth, fens))
            report["runs"].append(run)
            if expected is not None:
                report["differences"] = differences(expected, run)
                require(not report["differences"],
                        f"Search identity mismatch in round {trial} {engine}; see report.json")
            else:
                expected = run
            write_report(output / "report.json", report)
            print(f"  nodes={run['nodes']} time={run['time_ms']}ms NPS={run['nps']}", flush=True)
    summary = {}
    for engine in commands:
        rows = [row for row in report["runs"] if row["engine"] == engine]
        summary[engine] = {f"median_{key}": statistics.median(row[key] for row in rows)
                           for key in ("time_ms", "nps", "process_wall_s")}
    summary["candidate_reference_nps_ratio"] = summary["pikarust"]["median_nps"] / summary["pikafish"]["median_nps"]
    summary["candidate_reference_time_ratio"] = summary["pikarust"]["median_time_ms"] / summary["pikafish"]["median_time_ms"]
    report.update(status="passed", exact_search_identity=True, summary=summary)
    print(json.dumps(summary, indent=2), flush=True)


def positive(value):
    value = int(value)
    if value <= 0:
        raise argparse.ArgumentTypeError("must be positive")
    return value


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rounds", type=positive, default=3, help="paired rounds, alternating engine order (default: 3)")
    parser.add_argument("--depth", type=positive, default=13)
    parser.add_argument("--hash", type=positive, default=16, help="hash MiB per engine (default: 16)")
    parser.add_argument("--cpu", type=int, help="pin both processes to this Linux CPU")
    parser.add_argument("--reference-arch", help="must match the candidate's actual NNUE backend; default: automatic")
    parser.add_argument("--timeout", type=positive, default=300, help="seconds per complete engine bench (default: 300)")
    parser.add_argument("--output", type=Path, help="new report directory (default: target/bench/<UTC timestamp>)")
    args = parser.parse_args(argv)
    timestamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    output = (args.output or ROOT / "target/bench" / timestamp).resolve()
    report = dict(schema_version=1, status="running", started_utc=timestamp,
                  parameters=dict(rounds=args.rounds, depth=args.depth, hash_mib=args.hash,
                                  threads=1, positions=49, reset="once before each continuous 49-position run"))
    created = False
    try:
        require(not output.exists(), f"Refusing to overwrite existing report directory: {output}")
        output.mkdir(parents=True)
        created = True
        compare(args, output, report)
    except (BenchError, OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        report.update(status="failed", error=str(error))
        print(f"Benchmark comparison failed: {error}", file=sys.stderr)
        # Never overwrite an existing run when setup itself was refused.
        if created:
            write_report(output / "report.json", report)
        return 1
    write_report(output / "report.json", report)
    print(f"Verified search identity; timing report: {output / 'report.json'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
