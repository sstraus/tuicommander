# /// script
# requires-python = ">=3.11,<3.14"
# dependencies = [
#     "onnxruntime==1.23.0",
#     "numpy",
#     "sentencepiece",
#     "safetensors",
#     "huggingface_hub",
#     "soundfile",
#     "scipy",
# ]
# ///
"""Gate for the Pocket TTS voice catalogue: every voice must render.

Plan `plans/pocket-voices-and-loudness.md`, Step 1. For each of the six
languages TUICommander ships and each of the 26 Kyutai voices at the pinned
voice revision of its language, render one native sentence with the pinned ONNX export and check
that the audio is usable:

- the audio is not empty,
- it holds no NaN and no infinity,
- its duration is 0.5x to 3x the duration of that language's shipped
  default voice on the same sentence.

Each language reads one native sentence. A sentence in another language is a
bad probe: a model reading a foreign sentence can run on for seconds (the
Italian model read an English sentence for 8.4 s with giovanni and 3.0 s for
an Italian one), which measures the sentence, not the voice.

A voice that fails is reported and the script exits non-zero. A failing voice
is not pinned in the catalogue (Step 2).

The renderer is the reference runtime published in the ONNX repository
(`pocket_tts_onnx.py`) at the pinned revision. It is downloaded at run time,
not copied into this repository. It runs the same int8 graphs the app
downloads (`speech/assets.rs`).

Nothing is written into the repository. All downloads go to `--cache-dir`.

Usage:

    uv run scripts/speech-assets/verify_voices.py --cache-dir "${TMPDIR:-/tmp}/pocket-voices"
"""

from __future__ import annotations

import argparse
import importlib.util
import math
import sys
import time
from pathlib import Path

import numpy as np
from huggingface_hub import hf_hub_download, snapshot_download

from pins import (
    DEFAULT_VOICES as LANGUAGES,
    ONNX_REPO,
    ONNX_REVISION,
    VOICES_PER_LANGUAGE,
    VOICES_REPO,
    VOICES_REVISIONS,
)

# The files the reference runtime opens. The int8 graphs are the ones the app
# downloads. `mimi_encoder.onnx` is not in the app: the reference runtime opens
# it at start-up, and nothing here encodes audio with it.
BUNDLE_FILES = (
    "bundle.json",
    "tokenizer.model",
    "bos_before_voice.npy",
    "text_conditioner.onnx",
    "flow_lm_main_int8.onnx",
    "flow_lm_flow_int8.onnx",
    "mimi_decoder_int8.onnx",
    "mimi_encoder.onnx",
)

# One native sentence per language, all saying the same thing.
SENTENCES = {
    "italian": "Ciao, questa è una breve prova della voce.",
    "english_2026-04": "Hello, this is a short check of the voice.",
    "french_24l": "Bonjour, ceci est un court essai de la voix.",
    "german": "Hallo, das ist ein kurzer Test der Stimme.",
    "portuguese": "Olá, este é um breve teste da voz.",
    "spanish": "Hola, esta es una breve prueba de la voz.",
}
# A ratio against a reference that is itself cut short proves nothing: the
# 8843db76 French voices all rendered 0.72 s, ratio 1.00 to each other. Each
# sentence takes about 2-5 s to say, so a reference under this is a failure.
MIN_REFERENCE_SECONDS = 1.5
MIN_RATIO = 0.5
MAX_RATIO = 3.0
SEED = 1234


def load_runtime(snapshot: Path):
    """Import `pocket_tts_onnx.py` from the pinned snapshot."""
    source = snapshot / "pocket_tts_onnx.py"
    spec = importlib.util.spec_from_file_location("pocket_tts_onnx", source)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot import {source}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.PocketTTSOnnx


def fetch_onnx(cache: Path) -> Path:
    patterns = ["pocket_tts_onnx.py"] + [
        f"onnx/{language}/{name}" for language in LANGUAGES for name in BUNDLE_FILES
    ]
    return Path(
        snapshot_download(
            repo_id=ONNX_REPO,
            revision=ONNX_REVISION,
            allow_patterns=patterns,
            cache_dir=str(cache),
        )
    )


def list_voices(language: str) -> list[str]:
    from huggingface_hub import HfApi

    entries = HfApi().list_repo_tree(
        VOICES_REPO, path_in_repo=f"languages/{language}/embeddings", revision=VOICES_REVISIONS[language]
    )
    names = sorted(
        Path(entry.path).stem for entry in entries if entry.path.endswith(".safetensors")
    )
    return names


def fetch_voice(cache: Path, language: str, voice: str) -> Path:
    return Path(
        hf_hub_download(
            repo_id=VOICES_REPO,
            revision=VOICES_REVISIONS[language],
            filename=f"languages/{language}/embeddings/{voice}.safetensors",
            cache_dir=str(cache),
        )
    )


def render(tts, sentence: str, voice_file: Path) -> np.ndarray:
    # The runtime samples with `np.random.normal`; a fixed seed makes a run
    # repeatable.
    np.random.seed(SEED)
    return np.asarray(tts.generate(sentence, voice=str(voice_file)), dtype=np.float32)


def problems(audio: np.ndarray, sample_rate: int, reference_seconds: float | None) -> list[str]:
    found: list[str] = []
    if audio.size == 0:
        return ["empty audio"]
    if np.isnan(audio).any():
        found.append("NaN in audio")
    if np.isinf(audio).any():
        found.append("infinity in audio")
    if reference_seconds is not None:
        ratio = (audio.size / sample_rate) / reference_seconds
        if not (MIN_RATIO <= ratio <= MAX_RATIO):
            found.append(f"duration ratio {ratio:.2f} outside {MIN_RATIO}-{MAX_RATIO}")
    return found


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--cache-dir", required=True, type=Path, help="Download cache (outside the repo)")
    args = parser.parse_args()
    cache = args.cache_dir.expanduser().resolve()
    cache.mkdir(parents=True, exist_ok=True)

    revisions = ", ".join(f"{language}@{rev[:8]}" for language, rev in VOICES_REVISIONS.items())
    print(f"ONNX {ONNX_REPO}@{ONNX_REVISION[:8]}  voices {VOICES_REPO}: {revisions}")
    snapshot = fetch_onnx(cache)
    runtime = load_runtime(snapshot)
    models_dir = snapshot / "onnx"

    voices = {language: list_voices(language) for language in LANGUAGES}
    for language, names in voices.items():
        if len(names) != VOICES_PER_LANGUAGE:
            print(f"FAIL {language}: {len(names)} voices at the pinned revision, expected {VOICES_PER_LANGUAGE}")
            return 1
        if LANGUAGES[language] not in names:
            print(f"FAIL {language}: default voice {LANGUAGES[language]} missing")
            return 1

    total = sum(len(names) for names in voices.values())
    passed = 0
    failed: list[str] = []
    per_language: dict[str, tuple[int, int]] = {}

    for language, default in LANGUAGES.items():
        started = time.time()
        tts = runtime(models_dir=str(models_dir), language=language, precision="int8")
        print(f"-- {language}: loaded in {time.time() - started:.1f}s, reference {default}")
        # The shipped default comes first: every other voice of the language
        # is measured against it.
        names = [default] + [name for name in voices[language] if name != default]
        reference_seconds: float | None = None
        language_passed = 0
        for voice in names:
            label = f"{language}/{voice}"
            try:
                voice_file = fetch_voice(cache, language, voice)
                started = time.time()
                audio = render(tts, SENTENCES[language], voice_file)
                elapsed = time.time() - started
            except Exception as error:  # noqa: BLE001 — every failure is a report line
                failed.append(label)
                print(f"FAIL {label}: {type(error).__name__}: {error}")
                if voice == default:
                    print(f"     {language}: stopped, the reference voice failed")
                    break
                continue
            seconds = audio.size / tts.sample_rate
            if voice == default:
                reference_seconds = seconds
            issues = problems(audio, tts.sample_rate, reference_seconds)
            if voice == default and seconds < MIN_REFERENCE_SECONDS:
                issues.append(f"reference is {seconds:.2f}s, under {MIN_REFERENCE_SECONDS}s")
            peak = float(np.max(np.abs(audio))) if audio.size else 0.0
            peak_db = 20 * math.log10(peak) if peak > 0 else float("-inf")
            ratio = seconds / reference_seconds if reference_seconds else float("nan")
            detail = f"{seconds:.2f}s ratio {ratio:.2f} peak {peak_db:.1f} dBFS render {elapsed:.1f}s"
            if issues:
                failed.append(label)
                print(f"FAIL {label}: {'; '.join(issues)} ({detail})")
                if voice == default:
                    print(f"     {language}: stopped, the reference voice failed")
                    break
            else:
                passed += 1
                language_passed += 1
                print(f"OK   {label}: {detail}")
        per_language[language] = (language_passed, len(names))

    for language, (ok, count) in per_language.items():
        print(f"   {language}: {ok}/{count} OK")
    print(f"{passed}/{total} OK")
    if failed:
        print("Failed: " + ", ".join(failed))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
