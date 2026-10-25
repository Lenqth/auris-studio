#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Build an anonymous A/B listening bundle from real rendered WAV files.

The bundle keeps source audio and hashes, creates non-destructive loudness-matched
copies with ffmpeg, and exposes mix/foreground/backing views without revealing
the before/after identity to the listener. It does not score audio automatically.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import random
import re
import shutil
import subprocess
import wave
from pathlib import Path

VIEWS = ("mix", "foreground", "backing")
TARGET_LUFS = -16.0
TARGET_TRUEPEAK = -1.0


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_rms_peak(path: Path) -> tuple[float, float]:
    """Return RMS and peak as linear full-scale values for PCM WAV input."""
    with wave.open(str(path), "rb") as source:
        if source.getcomptype() != "NONE" or source.getsampwidth() not in (2, 3, 4):
            raise ValueError(f"{path}: expected uncompressed 16/24/32-bit PCM WAV")
        width = source.getsampwidth()
        maximum = float(1 << (width * 8 - 1))
        channels = source.getnchannels()
        total = 0
        squares = 0.0
        peak = 0.0
        while raw := source.readframes(65536):
            for offset in range(0, len(raw), width):
                sample = int.from_bytes(
                    raw[offset : offset + width], "little", signed=True
                )
                value = sample / maximum
                squares += value * value
                peak = max(peak, abs(value))
                total += 1
    if not total or not channels:
        raise ValueError(f"{path}: WAV contains no samples")
    return math.sqrt(squares / total), peak


def _run_gain(source: Path, destination: Path, gain_db: float) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    command = [
        "ffmpeg",
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-i",
        str(source),
        "-af",
        f"volume={gain_db:.6f}dB",
        "-c:a",
        "pcm_s24le",
        str(destination),
    ]
    try:
        subprocess.run(command, check=True, capture_output=True, text=True)
    except FileNotFoundError as error:
        raise RuntimeError("ffmpeg is required to create matched copies") from error
    except subprocess.CalledProcessError as error:
        raise RuntimeError(
            f"ffmpeg failed for {source}: {error.stderr.strip()}"
        ) from error


def _measure_loudness(source: Path) -> tuple[float, float]:
    """Measure integrated LUFS and input true peak with ffmpeg loudnorm."""
    command = [
        "ffmpeg",
        "-hide_banner",
        "-i",
        str(source),
        "-af",
        f"loudnorm=I={TARGET_LUFS}:TP={TARGET_TRUEPEAK}:LRA=7:print_format=json",
        "-f",
        "null",
        os.devnull,
    ]
    try:
        result = subprocess.run(command, check=True, capture_output=True, text=True)
    except FileNotFoundError as error:
        raise RuntimeError("ffmpeg is required to measure loudness") from error
    except subprocess.CalledProcessError as error:
        raise RuntimeError(
            f"ffmpeg loudness measurement failed for {source}: {error.stderr.strip()}"
        ) from error
    blocks = re.findall(r"\{[^{}]+\}", result.stderr, re.DOTALL)
    if not blocks:
        raise ValueError(f"{source}: loudnorm returned no measurement")
    data = json.loads(blocks[-1])
    try:
        integrated = float(data["input_i"])
        true_peak = float(data["input_tp"])
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError(f"{source}: loudnorm returned non-numeric values") from error
    if not math.isfinite(integrated) or not math.isfinite(true_peak):
        raise ValueError(
            f"{source}: LUFS/true-peak is non-finite; matched copy unavailable"
        )
    return integrated, true_peak


def _path(value: str, base: Path) -> Path:
    candidate = Path(value)
    return candidate if candidate.is_absolute() else (base / candidate).resolve()


def _validate_entry(entry: dict) -> None:
    if not isinstance(entry.get("id"), str) or not entry["id"]:
        raise ValueError("each pair needs a nonempty id")
    if entry.get("status") not in (None, "ready"):
        if not isinstance(entry.get("reason"), str) or not entry["reason"]:
            raise ValueError(f"{entry['id']}: failed entries need a reason")
        return
    files = entry.get("conditions")
    if not isinstance(files, dict) or set(files) != {"before", "after"}:
        raise ValueError(f"{entry['id']}: conditions must contain before and after")
    for condition, views in files.items():
        if not isinstance(views, dict) or not views.get("mix"):
            raise ValueError(f"{entry['id']}/{condition}: mix WAV is required")
        unknown = set(views) - set(VIEWS)
        if unknown:
            raise ValueError(f"{entry['id']}/{condition}: unknown views {unknown}")


def _copy_view(source: Path, destination: Path) -> dict:
    if not source.is_file():
        raise FileNotFoundError(source)
    rms, peak = read_rms_peak(source)
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination)
    return {"source": str(source), "sha256": digest(source), "rms": rms, "peak": peak}


def _error_code(error: Exception) -> str:
    text = str(error).lower()
    if "non-finite" in text or "loudnorm" in text or "measurement" in text:
        return "measurement_unavailable"
    if isinstance(error, FileNotFoundError):
        return "source_unavailable"
    return "audio_processing_failed"


def build(pair_file: Path, output: Path, seed: int) -> dict:
    if output.exists():
        raise FileExistsError(f"output already exists: {output}")
    spec = json.loads(pair_file.read_text(encoding="utf-8"))
    pairs = spec.get("pairs")
    if not isinstance(pairs, list) or not pairs:
        raise ValueError("pair file needs a nonempty pairs list")
    for entry in pairs:
        _validate_entry(entry)
    output.mkdir(parents=True)
    rng = random.Random(seed)
    pairs = list(pairs)
    rng.shuffle(pairs)
    public_pairs = []
    private_pairs = []
    base = pair_file.parent
    for index, entry in enumerate(pairs, 1):
        pair_id = f"pair-{index:03}"
        pair_dir = output / pair_id
        pair_dir.mkdir()
        if entry.get("status") not in (None, "ready"):
            public_pairs.append(
                {
                    "id": pair_id,
                    "status": "delivery_failed",
                }
            )
            private_pairs.append(
                {
                    "id": pair_id,
                    "source_id": entry["id"],
                    "status": entry["status"],
                    "reason": entry["reason"],
                }
            )
            continue
        conditions = [(name, entry["conditions"][name]) for name in ("before", "after")]
        rng.shuffle(conditions)
        metadata = entry.get("metadata", {})
        public = {
            "id": pair_id,
            "views": {},
            "matched": {},
            "failures": {},
            "metadata": {
                key: metadata[key]
                for key in ("genre", "vocal_mode", "instrumental", "tempo", "meter")
                if key in metadata
            },
        }
        private = {"id": pair_id, "source_id": entry["id"], "labels": {}, "assets": {}}
        resolved = {}
        for label, (condition, views) in zip(("A", "B"), conditions, strict=True):
            private["labels"][label] = condition
            private["assets"][label] = {}
            resolved[label] = {}
            public["views"][label] = {}
            public["matched"][label] = {}
            for view, raw_path in views.items():
                try:
                    source = _path(raw_path, base)
                    original = pair_dir / "original" / label / f"{view}.wav"
                    measured = _copy_view(source, original)
                except (FileNotFoundError, OSError, ValueError, RuntimeError) as error:
                    public["failures"].setdefault(label, {})[view] = _error_code(error)
                    private.setdefault("failure_details", {}).setdefault(label, {})[
                        view
                    ] = str(error)
                    continue
                relative = original.relative_to(output).as_posix()
                public["views"][label][view] = relative
                resolved[label][view] = measured
                private["assets"][label][view] = {
                    "source": str(Path(measured["source"]).resolve()),
                    "input_sha256": measured["sha256"],
                    "input_rms": measured["rms"],
                    "input_peak": measured["peak"],
                }
        for view in VIEWS:
            if not all(view in resolved[label] for label in ("A", "B")):
                continue
            try:
                loudness = {
                    label: _measure_loudness(output / public["views"][label][view])
                    for label in ("A", "B")
                }
                reachable = min(
                    TARGET_LUFS,
                    *(
                        loudness[label][0] + TARGET_TRUEPEAK - loudness[label][1]
                        for label in ("A", "B")
                    ),
                )
                public.setdefault("loudness", {})[view] = {
                    label: {
                        "input_lufs": loudness[label][0],
                        "input_truepeak": loudness[label][1],
                    }
                    for label in ("A", "B")
                }
                public.setdefault("matched_target_lufs", {})[view] = reachable
                for label in ("A", "B"):
                    gain = reachable - loudness[label][0]
                    relative = public["views"][label][view]
                    matched = pair_dir / "matched" / label / f"{view}.wav"
                    _run_gain(output / relative, matched, gain)
                    public["matched"][label][view] = matched.relative_to(
                        output
                    ).as_posix()
                    output_lufs, output_tp = _measure_loudness(matched)
                    if abs(output_lufs - reachable) > 0.15 or output_tp > -0.9:
                        raise ValueError(
                            f"matched {view}/{label} failed output loudness verification"
                        )
                    private["assets"][label][view].update(
                        {
                            "matched_sha256": digest(matched),
                            "gain_db": gain,
                            "output_lufs": output_lufs,
                            "output_truepeak": output_tp,
                        }
                    )
            except (ValueError, RuntimeError) as error:
                public["failures"].setdefault("matching", {})[view] = _error_code(error)
                private.setdefault("failure_details", {}).setdefault("matching", {})[
                    view
                ] = str(error)
        if not any(public["matched"][label] for label in ("A", "B")):
            public["failures"].setdefault("matching", {})["all"] = "no_common_view"
        public["status"] = "partial_failure" if public["failures"] else "ready"
        public["available_views"] = [
            view
            for view in VIEWS
            if view in public["views"]["A"] or view in public["views"]["B"]
        ]
        private["status"] = public["status"]
        private["failures"] = public["failures"]
        public_pairs.append(public)
        private_pairs.append(private)
    manifest = {
        "schema_version": 2,
        "seed": seed,
        "target_lufs": TARGET_LUFS,
        "target_truepeak": TARGET_TRUEPEAK,
        "pairs": public_pairs,
    }
    (output / "manifest.json").write_text(
        json.dumps(manifest, indent=2), encoding="utf-8"
    )
    (output / "private-key.json").write_text(
        json.dumps(
            {"schema_version": 2, "seed": seed, "pairs": private_pairs}, indent=2
        ),
        encoding="utf-8",
    )
    (output / "listening.html").write_text(_page(manifest), encoding="utf-8")
    return manifest


def _page(manifest: dict) -> str:
    payload = json.dumps(manifest, ensure_ascii=False).replace("</", "<\\/")
    return f"""<!doctype html><meta charset="utf-8"><title>匿名作曲比較リスニング</title>
<style>body{{font:16px system-ui;max-width:1000px;margin:32px auto}}section{{border-top:1px solid #aaa;padding:20px 0}}audio{{width:min(700px,100%)}}label{{display:block;margin:8px 0}}textarea{{width:95%;min-height:70px}}.hint{{color:#555}}button{{margin:8px 4px 8px 0}}</style>
<h1>匿名作曲比較リスニング</h1>
<p class="hint">最初はA/Bを切り替えず、1回だけ聴いてください。30秒ほど待って、覚えている主旋律の輪郭やモチーフを書いてから、A/Bを再生します。A/Bが強化前・強化後のどちらかは表示しません。音量一致版はゲインだけを使い、コンプレッサーやリミッターは使いません。</p>
<button onclick="setAll('matched')">音量一致版を使う</button><button onclick="setAll('original')">ペア音量合わせ前を使う</button><button onclick="save()">評価をダウンロード</button>
<div id="pairs"></div><script>
const data={payload}; let mode='matched';
function viewName(v){{return {{mix:'全体（ミックス）',foreground:'主旋律・歌声',backing:'伴奏'}}[v]||v;}}
function metadataText(m){{return Object.entries(m||{{}}).map(([k,v])=>({{genre:'ジャンル',vocal_mode:'声／インスト',instrumental:'インスト',tempo:'テンポ',meter:'拍子'}}[k]||k)+': '+v).join('　');}}
function render(){{document.querySelector('#pairs').innerHTML=data.pairs.map(p=>p.status==='delivery_failed'?`<section data-id="${{p.id}}"><h2>${{p.id}}</h2><p class="hint">音源未納品（delivery_failed）</p></section>`:`<section data-id="${{p.id}}"><h2>${{p.id}}</h2><p class="hint">${{metadataText(p.metadata)}}</p>${{p.status==='partial_failure'?`<p class="hint">一部音源がないため採点できません。未作成: ${{JSON.stringify(p.failures)}}</p>`:''}}<p class="hint">初回聴取では切り替えず、30秒待って記憶を書いてください。</p>${{['A','B'].map(l=>`<label>記憶した輪郭・モチーフ ${{l}}: <input data-memory="${{l}}"></label>`).join('')}}<label>聴く範囲: <select data-view onchange="changeView(this)">${{(p.available_views||[]).map(v=>`<option value="${{v}}">${{viewName(v)}}</option>`).join('')}}</select></label>${{['A','B'].map(l=>`<div><strong>${{l}}</strong> <span data-state="${{l}}"></span><br><audio controls data-audio="${{l}}"></audio></div>`).join('')}}${{p.status==='ready'?['naturalness','memorability','breathing','arrangement'].map(a=>`<label>${{({{naturalness:'自然さ',memorability:'覚えやすさ',breathing:'息継ぎ・フレーズの間',arrangement:'編曲・音色'}})[a]}} <select data-axis="${{a}}"><option value="">未評価</option><option value="A">A</option><option value="B">B</option><option value="tie">同程度</option><option value="cannot_judge">判断できない</option></select></label>`).join(''):''}}<label>根拠・タイムスタンプ・気になった点<textarea data-evidence></textarea></label></section>`).join(''); document.querySelectorAll('[data-view]').forEach(changeView); document.querySelectorAll('audio').forEach(a=>a.addEventListener('play',()=>{{document.querySelectorAll('audio').forEach(other=>{{if(other!==a)other.pause();}})}}));}}
function changeView(select){{const s=select.closest('section'),p=data.pairs.find(x=>x.id===s.dataset.id),v=select.value,files=mode==='matched'?p.matched:p.views,position=Math.max(...[...s.querySelectorAll('audio')].map(a=>a.currentTime)); s.querySelectorAll('[data-audio]').forEach(a=>{{const src=files[a.dataset.audio]?.[v]||''; a.src=src; a.currentTime=Number.isFinite(position)?position:0; s.querySelector(`[data-state="${{a.dataset.audio}}"]`).textContent=src?'':'（この範囲は未作成）';}});}}
function setAll(next){{mode=next;document.querySelectorAll('[data-view]').forEach(changeView);}}
function save(){{const ratings=[...document.querySelectorAll('section')].filter(s=>s.querySelector('[data-memory]')&&s.querySelector('[data-axis]')).map(s=>({{id:s.dataset.id,mode,remembered_contour:Object.fromEntries([...s.querySelectorAll('[data-memory]')].map(x=>[x.dataset.memory,x.value])),ratings:Object.fromEntries([...s.querySelectorAll('[data-axis]')].map(x=>[x.dataset.axis,x.value||null])),evidence:s.querySelector('[data-evidence]').value}})); const a=document.createElement('a');a.href=URL.createObjectURL(new Blob([JSON.stringify({{schema_version:2,ratings}},null,2)],{{type:'application/json'}}));a.download='ratings.json';a.click();setTimeout(()=>URL.revokeObjectURL(a.href),1000);}}
render();</script>"""


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("pair_file", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--seed", type=int, default=20261025)
    args = parser.parse_args()
    build(args.pair_file, args.output, args.seed)


if __name__ == "__main__":
    main()
