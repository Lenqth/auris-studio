"""Run isolated Codex transcription trials, keeping references out of model input."""

from __future__ import annotations

import argparse
import json
import os
import queue
import random
import shutil
import subprocess
import tempfile
import threading
import time
import tomllib
from pathlib import Path

from direct_audio_transcription import PROMPT, parse_notes, save
from piano_transcription import digest

TOOL_PROMPT = """Transcribe every audible pitched note in the anonymous WAV files listed in inputs.json. This is a blind audio-to-note experiment. Work only in this workspace and its .venv; do not read parent folders, other projects, scores, MIDI, saved transcriptions, or reference annotations. Do not use Basic Pitch, any pretrained transcription model, external recognition service, or the internet. Do not infer notes from identities or compose replacement music. You may write and run Python signal-analysis code and inspect plots/numeric evidence, choosing and refining the method yourself from the audio alone. Python is .venv/Scripts/python.exe; numpy, scipy, soundfile, matplotlib are installed. Do not delegate to other agents. There are no reference scores or correctness feedback. Use one reusable method on every file, including silence and overlapping notes. Preserve original seconds and MIDI pitch, with separate events for repeated attacks. Write outputs/<id>.json for every input, each as {"notes":[objects with integer pitch and numeric start/end in seconds],"limitations":"honest explanation"}. Only output finite valid intervals within the audio, MIDI range 21..108; silence should have an empty list. Keep the algorithm and any intermediate analysis in this workspace. Complete a bounded best effort within about ten minutes; prioritize producing every draft over unbounded tuning. In your final response state the actual method, number of files/notes, unresolved errors, and whether you actually heard audio or only analyzed its samples. Do not claim faithful transcription without independent validation."""


def preferences():
    """Read model preferences only; never handle subscription credential files."""
    config = Path.home() / ".codex" / "config.toml"
    return tomllib.loads(config.read_text(encoding="utf-8")) if config.exists() else {}


def child_environment():
    """Keep CLI authentication, but do not inherit the calling host's internal pipes."""
    return {
        k: v
        for k, v in os.environ.items()
        if not (
            k.upper().startswith("OPENAI_")
            or (k.upper().startswith("CODEX_") and k.upper() != "CODEX_HOME")
        )
    }


class Rpc:
    """A short-lived official CLI connection; authentication remains inside Codex."""

    def __init__(self, executable: Path, cwd: Path, tools: bool = False):
        settings = preferences()
        self.model = settings.get("model")
        self.effort = settings.get("model_reasoning_effort", "medium")
        command = [str(executable), "app-server", "--listen", "stdio://"]
        overrides = {
            "model_provider": "openai",
            "approvals_reviewer": "user",
            "web_search": "disabled",
            "project_doc_max_bytes": 0,
            "notify": [],
        }
        for feature in (
            "shell_snapshot",
            "hooks",
            "plugins",
            "apps",
            "multi_agent",
            "code_mode",
            "remote_plugin",
            "memories",
            "skill_mcp_dependency_install",
        ):
            overrides[f"features.{feature}"] = False
        overrides["features.skip_host_skill_discovery"] = True
        for feature in ("shell_tool", "unified_exec", "apply_patch_freeform"):
            overrides[f"features.{feature}"] = tools
        for key, value in overrides.items():
            command.extend(["-c", f"{key}={json.dumps(value)}"])
        servers = settings.get("mcp_servers", {})
        if servers:
            disabled = ",".join(json.dumps(k) + "={enabled=false}" for k in servers)
            command.extend(["-c", "mcp_servers={" + disabled + "}"])
        env = child_environment()
        self.stderr = (cwd / "server.stderr.log").open("w", encoding="utf-8")
        self.process = subprocess.Popen(
            command,
            cwd=cwd,
            env=env,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=self.stderr,
            text=True,
            encoding="utf-8",
            creationflags=(subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0),
        )
        self.messages = queue.Queue()
        self.events = []
        self.next_id = 0
        threading.Thread(target=self._read, daemon=True).start()
        self.request(
            "initialize",
            {
                "clientInfo": {"name": "auris_transcription_eval", "version": "1.0"},
                "capabilities": {"experimentalApi": True},
            },
        )
        self.send({"method": "initialized"})
        account = self.request("account/read", {"refreshToken": False}, record=False)
        if account.get("account", {}).get("type") != "chatgpt":
            self.close()
            raise ValueError("Codex must be signed in with ChatGPT")

    def _read(self):
        for line in self.process.stdout:
            try:
                self.messages.put(json.loads(line))
            except json.JSONDecodeError:
                self.messages.put({"malformed_stdout": line})
        self.messages.put({"process_ended": True})

    def send(self, value):
        self.process.stdin.write(json.dumps(value) + "\n")
        self.process.stdin.flush()

    def read(self, timeout=180, record=True):
        value = self.messages.get(timeout=timeout)
        if record:
            self.events.append(value)
        if value.get("process_ended"):
            raise RuntimeError("Codex process ended")
        return value

    def request(self, method, params, record=True):
        self.next_id += 1
        request_id = self.next_id
        self.send({"id": request_id, "method": method, "params": params})
        while True:
            message = self.read(record=record)
            if message.get("id") == request_id:
                if "error" in message:
                    raise ValueError(json.dumps(message["error"]))
                return message["result"]

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            self.process.wait(timeout=5)
        self.stderr.close()


def direct(args):
    """Fresh tool-free threads with actual localAudio inputs and a fixed prompt."""
    corpus = json.loads(args.manifest.read_text(encoding="utf-8"))
    cases = [c for c in corpus["cases"] if c["split"] == args.split]
    if args.limit:
        cases = cases[: args.limit]
    args.output.mkdir()
    rpc = Rpc(args.codex, args.output.resolve())
    try:
        models = rpc.request("model/list", {"limit": 100})
        save(args.output / "models.json", models)
        save(
            args.output / "run.json",
            {
                "model": rpc.model,
                "effort": rpc.effort,
                "prompt": PROMPT,
                "manifest_sha256": digest(args.manifest),
                "split": args.split,
                "cases": [c["id"] for c in cases],
                "input": "localAudio",
                "native_tools": False,
            },
        )
        for case in cases:
            source = args.manifest.parent / case["audio"]
            stimulus = args.output / "stimulus.wav"
            shutil.copyfile(source, stimulus)
            begin = len(rpc.events)
            started = time.perf_counter()
            result = {
                "id": case["id"],
                "audio_sha256": digest(source),
                "seconds": case["seconds"],
            }
            try:
                thread = rpc.request(
                    "thread/start",
                    {
                        "model": rpc.model,
                        "cwd": str(args.output.resolve()),
                        "ephemeral": True,
                        "approvalPolicy": "never",
                        "sandbox": "read-only",
                        "baseInstructions": "You are evaluating audio transcription. No tools, reference scores, or outside information may be used. Report uncertainty honestly.",
                    },
                )
                thread_id = thread["thread"]["id"]
                result["actual_model"] = thread.get("model")
                rpc.request(
                    "turn/start",
                    {
                        "threadId": thread_id,
                        "effort": rpc.effort,
                        "input": [
                            {"type": "text", "text": PROMPT},
                            {"type": "localAudio", "path": str(stimulus.resolve())},
                        ],
                    },
                )
                deadline = time.monotonic() + args.timeout
                text = ""
                while time.monotonic() < deadline:
                    try:
                        event = rpc.read(
                            timeout=min(60, max(1, deadline - time.monotonic()))
                        )
                    except queue.Empty:
                        continue
                    method, params = event.get("method"), event.get("params", {})
                    if (
                        method == "item/completed"
                        and params["item"]["type"] == "agentMessage"
                    ):
                        text = params["item"]["text"]
                    if "id" in event and method:
                        rpc.send(
                            {
                                "id": event["id"],
                                "error": {
                                    "code": -32601,
                                    "message": "Tools and approvals unavailable in direct audio trial",
                                },
                            }
                        )
                    if method == "turn/completed":
                        result["turn"] = params["turn"]
                        if params["turn"]["status"] != "completed":
                            raise ValueError(json.dumps(params["turn"]))
                        break
                else:
                    raise TimeoutError("Codex transcription timed out")
                result["content"] = text
                result["notes"] = parse_notes(text, case["seconds"])
                result["status"] = "valid"
            except (
                ValueError,
                TypeError,
                KeyError,
                RuntimeError,
                TimeoutError,
                queue.Empty,
            ) as error:
                result.update(status="failed", error=str(error))
            result["elapsed_seconds"] = time.perf_counter() - started
            result["events"] = rpc.events[begin:]
            save(args.output / f"{case['id']}.json", result)
            print(
                json.dumps(
                    {
                        k: result.get(k)
                        for k in ("id", "status", "error", "elapsed_seconds")
                    }
                ),
                flush=True,
            )
    finally:
        rpc.close()


def prepare_inputs(manifests, workspace: Path):
    """Copy anonymous audio only; keep the private label mapping outside the model cwd."""
    (workspace / "inputs").mkdir()
    items = []
    for manifest, split in manifests:
        corpus = json.loads(manifest.read_text(encoding="utf-8"))
        items.extend((manifest, c) for c in corpus["cases"] if c["split"] == split)
    if not items:
        raise ValueError("empty evaluation cohort")
    random.Random(218173).shuffle(items)
    mapping = []
    for index, (manifest, case) in enumerate(items, 1):
        anonymous = f"s{index:02d}"
        destination = workspace / "inputs" / f"{anonymous}.wav"
        shutil.copyfile(manifest.parent / case["audio"], destination)
        mapping.append(
            {
                "anonymous_id": anonymous,
                "id": case["id"],
                "split": case["split"],
                "seconds": case["seconds"],
                "audio_sha256": digest(destination),
                "input_file": str(destination),
                "manifest": str(manifest.resolve()),
            }
        )
    with (workspace / "inputs.json").open("x", encoding="utf-8") as stream:
        json.dump(
            [
                {
                    "id": m["anonymous_id"],
                    "file": f"inputs/{m['anonymous_id']}.wav",
                    "seconds": m["seconds"],
                }
                for m in mapping
            ],
            stream,
            indent=2,
        )
    return mapping


def tools(args):
    """Let Codex build its own DSP method in a fresh workspace with no score access."""
    args.output.mkdir()
    workspace = Path(tempfile.mkdtemp(prefix="auris-codex-transcription-"))
    manifests = [(args.manifest, args.split)]
    if args.controls:
        manifests.append((args.controls, "control"))
    mapping = prepare_inputs(manifests, workspace)
    save(
        args.output / "private-mapping.json",
        {"workspace": str(workspace), "cases": mapping},
    )
    interpreter = (
        workspace
        / ".venv"
        / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
    )
    with (args.output / "setup.log").open("w", encoding="utf-8") as log:
        subprocess.run(
            ["uv", "venv", "--python", args.python, str(workspace / ".venv")],
            check=True,
            stdout=log,
            stderr=log,
        )
        subprocess.run(
            [
                "uv",
                "pip",
                "install",
                "--python",
                str(interpreter),
                "numpy",
                "scipy",
                "soundfile",
                "matplotlib",
            ],
            check=True,
            stdout=log,
            stderr=log,
        )
    settings = preferences()
    command = [
        str(args.codex),
        "exec",
        "--ignore-user-config",
        "--skip-git-repo-check",
        "--ephemeral",
        "--json",
        "--approve-for-me",
        "-C",
        str(workspace),
        "-o",
        str(workspace / "final.txt"),
    ]
    if settings.get("model"):
        command.extend(["-m", settings["model"]])
    overrides = {
        "model_reasoning_effort": settings.get("model_reasoning_effort", "medium"),
        "web_search": "disabled",
        "project_doc_max_bytes": 0,
        "features.skip_host_skill_discovery": True,
    }
    for feature in (
        "shell_snapshot",
        "hooks",
        "plugins",
        "apps",
        "multi_agent",
        "code_mode",
        "remote_plugin",
        "memories",
        "skill_mcp_dependency_install",
    ):
        overrides[f"features.{feature}"] = False
    for key, value in overrides.items():
        command.extend(["-c", f"{key}={json.dumps(value)}"])
    command.append("-")
    env = child_environment()
    env["PYTHONUTF8"] = "1"
    (args.output / "prompt.txt").write_text(TOOL_PROMPT, encoding="utf-8")
    started = time.perf_counter()
    with (
        (args.output / "events.jsonl").open("w", encoding="utf-8") as stdout,
        (args.output / "stderr.txt").open("w", encoding="utf-8") as stderr,
    ):
        process = subprocess.Popen(
            command,
            cwd=workspace,
            env=env,
            stdin=subprocess.PIPE,
            stdout=stdout,
            stderr=stderr,
            text=True,
            encoding="utf-8",
            creationflags=(subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0),
        )
        save(
            args.output / "launch.json",
            {
                "model": settings.get("model"),
                "effort": settings.get("model_reasoning_effort"),
                "command": command,
                "workspace": str(workspace),
                "pid": process.pid,
            },
        )
        timed_out = False
        try:
            process.communicate(TOOL_PROMPT, timeout=args.timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            process.terminate()
            process.wait(timeout=10)
    save(
        args.output / "completion.json",
        {
            "exit_code": process.returncode,
            "timed_out": timed_out,
            "elapsed_seconds": time.perf_counter() - started,
        },
    )


def main():
    """Supply paths explicitly; never read authentication files in this harness."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--codex", type=Path, required=True)
    parser.add_argument("--split", default="held_out")
    parser.add_argument("--limit", type=int)
    parser.add_argument("--mode", choices=("direct", "tools"), default="direct")
    parser.add_argument("--controls", type=Path)
    parser.add_argument("--python", default="3.14")
    parser.add_argument("--timeout", type=float)
    args = parser.parse_args()
    args.timeout = args.timeout or (900 if args.mode == "tools" else 180)
    (tools if args.mode == "tools" else direct)(args)


if __name__ == "__main__":
    main()
