# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Freeze and run real panel composition trials, then prepare blinded listening pairs.

Run with uv. A passing transport is recorded separately from unscored musical quality.
The executable must be the auris-gpui binary test target, not the desktop application.
"""

from __future__ import annotations

import argparse
import hashlib
import html
import json
import os
import random
import shutil
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = Path(__file__).with_name("references") / "composition-quality-v1.json"
TEST = "ui::agent_chat::composition_eval::codex_composes_and_revises_the_open_document"
AXES = ["request_fit", "melody", "form", "ensemble", "sound"]


def digest(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def write_json(path: Path, value: object) -> None:
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(
        json.dumps(value, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    temporary.replace(path)


def read_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


PROMPT_CONDITION_KEYS = {
    "name",
    "source_commit",
    "expected_prefix",
    "replacement_prefix",
    "skill_context",
}


def prompt_condition(path: Path) -> tuple[dict[str, str], bytes, str]:
    """Validate and freeze an optional before/after prompt condition file."""
    payload = path.read_bytes()
    value = json.loads(payload)
    if not isinstance(value, dict) or set(value) != PROMPT_CONDITION_KEYS:
        raise ValueError(
            "Prompt condition must contain exactly: "
            + ", ".join(sorted(PROMPT_CONDITION_KEYS))
        )
    if not all(isinstance(item, str) for item in value.values()):
        raise ValueError("Prompt condition values must all be strings")
    for key in ("expected_prefix", "replacement_prefix"):
        if not value[key]:
            raise ValueError(f"Prompt condition {key} must be nonempty")
    return value, payload, hashlib.sha256(payload).hexdigest()


def manifest(path: Path) -> dict:
    value = read_json(path)
    if value.get("schema_version") != 1 or value.get("axes") != AXES:
        raise ValueError("Unsupported manifest or evaluation axes")
    rubric = value.get("listening_rubric")
    if rubric is not None:
        if not isinstance(rubric, dict) or set(rubric) - set(AXES):
            raise ValueError("Listening rubric keys must be evaluation axes")
        for axis, questions in rubric.items():
            if not isinstance(questions, list) or not questions or not all(
                isinstance(question, str) and question.strip()
                for question in questions
            ):
                raise ValueError(
                    f"Listening rubric for {axis} needs nonempty string questions"
                )
    cases = value["cases"]
    ids = [case["id"] for case in cases]
    if len(ids) != len(set(ids)):
        raise ValueError("Duplicate case IDs")
    for case in cases:
        if not case["id"] or any(
            c not in "abcdefghijklmnopqrstuvwxyz0123456789_-" for c in case["id"]
        ):
            raise ValueError("Case IDs must be safe directory names")
        if case["split"] not in {"development", "holdout"}:
            raise ValueError("Invalid split")
        if case["fixture"] not in {
            "blank", "melody_six_eight", "melody_six_eight_eight_bars"
        }:
            raise ValueError("Unknown fixture")
        if not case["prompts"] or not all(
            isinstance(p, str) and p.strip() for p in case["prompts"]
        ):
            raise ValueError("Each case needs natural-language prompts")
    return value


def frozen_source(
    reference: Path, runtime: dict[str, str], snapshot: Path | None = None
) -> dict:
    """Bind a reused executable to previously captured source, never today's checkout."""
    payload = reference.read_bytes()
    prior = json.loads(payload)
    if prior.get("runtime") != runtime:
        raise ValueError("Frozen runtime does not match the source suite")
    if snapshot is not None:
        snapshot.write_bytes(payload)
    return {
        key: prior[key]
        for key in ("source_commit", "source_diff", "untracked_rust_sources")
    } | {"source_reference_sha256": hashlib.sha256(payload).hexdigest()}


def run(args) -> None:
    suite = manifest(args.manifest)
    condition = None
    condition_payload = None
    condition_sha256 = None
    if args.prompt_condition is not None:
        condition, condition_payload, condition_sha256 = prompt_condition(
            args.prompt_condition.resolve(strict=True)
        )
    if not 1 <= args.repetitions <= 20 or not 1 <= args.timeout <= 3600:
        raise ValueError("Use 1..20 repetitions and 1..3600 seconds per turn")
    if not 8192 <= args.context_tokens <= 262144:
        raise ValueError("Use 8192..262144 context tokens")
    eligible = {case["id"] for case in suite["cases"] if case["split"] == args.split}
    if args.case and not set(args.case) <= eligible:
        raise ValueError("Requested case is not in the selected split")
    executable = args.executable.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    frozen = output / "runtime"
    frozen.mkdir()
    binary = frozen / executable.name
    shutil.copy2(executable, binary)
    # ORT and platform runtime DLLs may sit beside a Cargo test target or one level above.
    for folder in [executable.parent.parent, executable.parent]:
        for source in folder.glob("*.dll"):
            shutil.copy2(source, frozen / source.name)
    provenance = {
        "schema_version": 1,
        "manifest": suite,
        "manifest_sha256": digest(args.manifest),
        "model": args.model,
        "backend": getattr(args, "backend", "codex"),
        "prompt_profile": getattr(args, "prompt_profile", "legacy"),
        "candidate_tools_enabled": args.candidate_tools,
        "context_tokens": args.context_tokens,
        "runner_sha256": digest(Path(__file__)),
        "split": args.split,
        "repetitions": args.repetitions,
        "source_commit": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
        ).strip(),
        "source_diff": subprocess.check_output(
            ["git", "diff", "HEAD", "--", "crates", "docs"],
            cwd=ROOT,
            text=True,
            encoding="utf-8",
        ),
        "untracked_rust_sources": {
            name: (ROOT / name).read_text(encoding="utf-8")
            for name in subprocess.check_output(
                [
                    "git",
                    "ls-files",
                    "--others",
                    "--exclude-standard",
                    "-z",
                    "--",
                    "crates",
                ],
                cwd=ROOT,
                text=True,
                encoding="utf-8",
            ).split("\0")
            if name.endswith(".rs")
        },
        "selected_cases": [
            case["id"]
            for case in suite["cases"]
            if case["split"] == args.split
            and (not args.case or case["id"] in args.case)
        ],
        "runtime": {p.name: digest(p) for p in sorted(frozen.iterdir()) if p.is_file()},
        "started_unix": time.time(),
        "quality_status": "unscored; transport and structural measurements are not musical preference",
        "trials": [],
    }
    if condition is not None:
        provenance["prompt_condition"] = condition
        provenance["prompt_condition_sha256"] = condition_sha256
        (output / "prompt-condition.json").write_bytes(condition_payload)
    if getattr(args, "source_suite", None):
        provenance.update(
            frozen_source(
                args.source_suite, provenance["runtime"], output / "source-suite.json"
            )
        )
    write_json(output / "suite.json", provenance)
    for case in suite["cases"]:
        if case["split"] != args.split or (args.case and case["id"] not in args.case):
            continue
        for repetition in range(args.repetitions):
            trial_id = f"{case['id']}-{repetition + 1:02}"
            destination = output / trial_id
            config = {
                "model": args.model,
                "fixture": case["fixture"],
                "timeout_seconds": args.timeout,
                "prompts": [
                    suite["conditions"] + "\n" + case["prompts"][0],
                    *case["prompts"][1:],
                    suite["delivery_prompt"],
                ],
                "output": str(destination),
            }
            if condition is not None:
                config["prompt_condition"] = {
                    key: condition[key]
                    for key in ("expected_prefix", "replacement_prefix", "skill_context")
                }
            config_path = output / f"{trial_id}.config.json"
            if provenance["backend"] != "codex":
                config["backend"] = provenance["backend"]
                config["context_tokens"] = args.context_tokens
            write_json(config_path, config)
            environment = dict(
                os.environ,
                AURIS_COMPOSITION_EVAL=str(config_path),
                AURIS_COMPOSITION_WORKFLOWS="1"
                if provenance["prompt_profile"] == "modular"
                else "0",
                AURIS_ARRANGEMENT_HARNESS="0",
                AURIS_COMPOSITION_CANDIDATES="1" if args.candidate_tools else "0",
            )
            started = time.monotonic()
            with (output / f"{trial_id}.log").open("w", encoding="utf-8") as log:
                process = subprocess.run(
                    [
                        str(binary),
                        "--ignored",
                        "--exact",
                        TEST,
                        "--nocapture",
                        "--test-threads=1",
                    ],
                    env=environment,
                    cwd=ROOT,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    check=False,
                )
            report_path = destination / "report.json"
            report = read_json(report_path) if report_path.exists() else {}
            artifacts = {
                str(p.relative_to(destination)): digest(p)
                for p in sorted(destination.rglob("*"))
                if p.is_file()
            }
            provenance["trials"].append(
                {
                    "id": trial_id,
                    "case": case["id"],
                    "repetition": repetition + 1,
                    "elapsed_seconds": time.monotonic() - started,
                    "exit_code": process.returncode,
                    "transport_pass": process.returncode == 0
                    and len(report.get("turns", [])) == len(config["prompts"])
                    and all(t.get("failure") is None for t in report["turns"]),
                    "config_sha256": digest(config_path),
                    "delivery_present": "result.wav" in artifacts
                    and "result.mid" in artifacts,
                    "encoded": report.get("encoded"),
                    "tool_errors": report.get("argument_or_tool_errors"),
                    "artifacts": artifacts,
                    "ratings": {axis: None for axis in AXES},
                }
            )
            write_json(output / "suite.json", provenance)
            print(
                f"{trial_id}: exit={process.returncode}, {time.monotonic() - started:.1f}s",
                flush=True,
            )


def pairs(before: Path, after: Path, seed: int) -> list[dict]:
    left, right = read_json(before / "suite.json"), read_json(after / "suite.json")
    for field in ["manifest_sha256", "model", "backend", "split", "repetitions"]:
        if left[field] != right[field]:
            raise ValueError(f"Comparison conditions differ: {field}")
    for suite in [left, right]:
        selected = suite.get("pilot_selection", {}).get(
            "cases", suite.get("selected_cases")
        )
        if selected is None:
            selected = [
                case["id"]
                for case in suite["manifest"]["cases"]
                if case["split"] == suite["split"]
            ]
        expected = {
            f"{case}-{repetition:02}"
            for case in selected
            for repetition in range(1, suite["repetitions"] + 1)
        }
        if not expected or expected != {trial["id"] for trial in suite["trials"]}:
            raise ValueError(
                "Incomplete trial cohorts; collect or explicitly record every selected trial"
            )
    lookup = {trial["id"]: trial for trial in right["trials"]}
    if len(lookup) != len(right["trials"]) or len(
        {t["id"] for t in left["trials"]}
    ) != len(left["trials"]):
        raise ValueError("Duplicate trial IDs")
    if set(lookup) != {trial["id"] for trial in left["trials"]}:
        raise ValueError("Trial cohorts differ; do not silently discard missing runs")
    rng = random.Random(seed)
    result = []
    for trial in left["trials"]:
        other = lookup[trial["id"]]
        if not all(
            t["transport_pass"] and t["delivery_present"] for t in [trial, other]
        ):
            result.append(
                {"trial": trial["id"], "status": "failed_or_missing_delivery"}
            )
            continue
        sources = [(before, trial), (after, other)]
        rng.shuffle(sources)
        audio = []
        for root, record in sources:
            path = root / record["id"] / "result.wav"
            if digest(path) != record["artifacts"]["result.wav"]:
                raise ValueError(f"Changed listening artifact: {path}")
            audio.append(
                {
                    "path": str(path.resolve()),
                    "sha256": digest(path),
                    "condition": "before" if root == before else "after",
                }
            )
        result.append({"trial": trial["id"], "status": "ready", "audio": audio})
    rng.shuffle(result)
    return result


def blind(args) -> None:
    comparisons = pairs(args.before, args.after, args.seed)
    args.output.mkdir(parents=True, exist_ok=False)
    suite = read_json(args.before / "suite.json")
    rubric = suite["manifest"].get("listening_rubric", {})
    sections = []
    failed = sum(pair["status"] != "ready" for pair in comparisons)
    for index, pair in enumerate(comparisons, 1):
        if pair["status"] != "ready":
            continue
        pair["blind_id"] = f"pair-{index:03}"
        players = []
        for label, source in zip("AB", pair["audio"], strict=True):
            filename = f"{pair['blind_id']}-{label}.wav"
            shutil.copy2(source["path"], args.output / filename)
            players.append(f'<p>{label} <audio controls src="{filename}"></audio></p>')
        questions = "".join(
            f'<label><span class="axis">{html.escape(axis)}</span>'
            + (
                "<ul>"
                + "".join(
                    f"<li>{html.escape(question)}</li>"
                    for question in rubric.get(axis, [])
                )
                + "</ul>"
                if rubric.get(axis)
                else ""
            )
            + f' <select data-axis="{html.escape(axis, quote=True)}"><option value="">Unrated</option>'
            "<option>A</option><option>B</option><option>tie</option><option>cannot_judge</option></select></label> "
            for axis in AXES
        )
        case = next(
            c
            for c in suite["manifest"]["cases"]
            if pair["trial"].rsplit("-", 1)[0] == c["id"]
        )
        brief = html.escape("\n".join(case["prompts"]))
        sections.append(
            f'<section data-id="{pair["blind_id"]}"><h2>{pair["blind_id"]}</h2><pre>{brief}</pre>{"".join(players)}{questions}<p><textarea placeholder="Evidence, timestamps, unresolved issues"></textarea></p></section>'
        )
    page = (
        """<!doctype html><meta charset="utf-8"><title>Composition comparison</title>
<style>body{font:16px system-ui;max-width:1000px;margin:40px auto}section{border-top:1px solid #aaa;padding:20px 0}pre{white-space:pre-wrap}textarea{width:95%;min-height:60px}audio{vertical-align:middle}</style>
<h1>Composition comparison</h1><p>Independent model runs under matching conditions. Original output levels are retained; assess loudness separately from musical preference. A/B identities are in the separate key file.</p>
<button onclick="save()">Download ratings</button>"""
        + f"<p>{len(comparisons)} total trials; {failed} failed or missing delivery. Failures remain in the separate key and must be included in reporting.</p>"
        + "".join(sections)
        + """<script>
function save(){const ratings=Array.from(document.querySelectorAll('section')).map(s=>({id:s.dataset.id,ratings:Object.fromEntries(Array.from(s.querySelectorAll('select')).map(x=>[x.dataset.axis,x.value||null])),evidence:s.querySelector('textarea').value}));const a=document.createElement('a');a.href=URL.createObjectURL(new Blob([JSON.stringify({schema_version:1,ratings},null,2)],{type:'application/json'}));a.download='ratings.json';a.click();setTimeout(()=>URL.revokeObjectURL(a.href),1000)}
</script>"""
    )
    (args.output / "listening.html").write_text(page, encoding="utf-8")
    write_json(
        args.output / "private-key.json", {"seed": args.seed, "pairs": comparisons}
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    runner = commands.add_parser("run")
    runner.add_argument("--executable", type=Path, required=True)
    runner.add_argument("--output", type=Path, required=True)
    runner.add_argument("--manifest", type=Path, default=MANIFEST)
    runner.add_argument("--model", default="gpt-6.1-sol")
    runner.add_argument(
        "--backend", choices=["codex", "claude", "rig"], default="codex"
    )
    runner.add_argument(
        "--prompt-profile", choices=["legacy", "modular"], default="legacy"
    )
    runner.add_argument(
        "--candidate-tools",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="Advertise candidate tools (use --no-candidate-tools for the default app catalog)",
    )
    runner.add_argument(
        "--source-suite",
        type=Path,
        help="Previous suite binding an identical frozen runtime to its original source",
    )
    runner.add_argument(
        "--prompt-condition",
        type=Path,
        help="JSON file freezing the before/after prompt condition and its provenance",
    )
    runner.add_argument(
        "--split", choices=["development", "holdout"], default="development"
    )
    runner.add_argument("--repetitions", type=int, default=3)
    runner.add_argument("--timeout", type=int, default=600)
    runner.add_argument("--context-tokens", type=int, default=65536)
    runner.add_argument("--case", action="append")
    comparison = commands.add_parser("blind")
    comparison.add_argument("--before", type=Path, required=True)
    comparison.add_argument("--after", type=Path, required=True)
    comparison.add_argument("--output", type=Path, required=True)
    comparison.add_argument("--seed", type=int, default=20261024)
    args = parser.parse_args()
    (run if args.command == "run" else blind)(args)


if __name__ == "__main__":
    main()
