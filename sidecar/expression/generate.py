from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.request
import uuid
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

EYE_EXPRESSIONS = {
    "smile": "smile, happy",
    "blink": "(closed eyes:1.5), both eyelids shut, calm",
    "angry": "angry, furrowed brow, frown",
    "sad": "sad, worried eyebrows, watery eyes, downturned mouth",
    "surprised": "surprised, (wide eyes:1.3), raised eyebrows",
}
EXPRESSIONS = {**EYE_EXPRESSIONS, "mouth_open": "neutral eyes and eyebrows"}
VOWELS = {
    "a": "open mouth, vertical oval mouth, Japanese A phoneme",
    "i": "parted lips, wide narrow mouth, Japanese I phoneme",
    "u": "pursed lips, tiny rounded mouth, Japanese U phoneme",
    "e": "parted lips, moderately wide mouth, Japanese E phoneme",
    "o": "open mouth, round mouth, Japanese O phoneme",
    "close": "closed mouth",
}
NEGATIVE = (
    "worst quality, low quality, deformed, extra face, asymmetric eyes, head tilt, "
    "different person, different hairstyle, different hair color, different eye color, "
    "different clothes, changed accessory, mirrored, flipped, profile, watermark, text"
)


def request_json(url: str, payload: dict | None = None, timeout: float = 30) -> dict:
    data = None if payload is None else json.dumps(payload).encode("utf-8")
    request = urllib.request.Request(
        url, data=data, headers={"Content-Type": "application/json"}
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return json.load(response)


def wait_for_server(base_url: str, process: subprocess.Popen, timeout: int) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"ComfyUI exited during startup: {process.returncode}")
        try:
            request_json(f"{base_url}/system_stats", timeout=2)
            return
        except (urllib.error.URLError, TimeoutError):
            time.sleep(1)
    raise TimeoutError(f"ComfyUI did not start within {timeout} seconds")


def make_mask(path: Path, region: str) -> None:
    mask = Image.new("L", (1024, 1024), 0)
    draw = ImageDraw.Draw(mask)
    if region in ("eyes", "both"):
        draw.ellipse((300, 405, 500, 555), fill=255)
        draw.ellipse((524, 405, 724, 555), fill=255)
    if region in ("mouth", "both"):
        draw.ellipse((365, 565, 659, 735), fill=255)
    mask = mask.filter(ImageFilter.GaussianBlur(radius=8))
    Image.merge("RGB", (mask, mask, mask)).save(path)


def prepare_workflow(
    template: dict,
    expression: str,
    vowel: str,
    seed: int,
    denoise: float,
    control_strength: float,
    identity_tags: str,
) -> dict:
    workflow = json.loads(json.dumps(template))
    positive = (
        "masterpiece, best quality, anime portrait, same character, same identity, "
        "exact same frontal camera, exact same head position, preserve hairstyle, hair color, "
        "eye color, clothing, accessory, lighting and drawing style, "
        f"{identity_tags}, {EXPRESSIONS[expression]}, {VOWELS[vowel]}"
    )
    workflow["4"]["inputs"]["text"] = positive
    workflow["5"]["inputs"]["text"] = NEGATIVE
    workflow["7"]["inputs"]["seed"] = seed
    workflow["7"]["inputs"]["denoise"] = denoise
    if control_strength <= 0:
        workflow["7"]["inputs"]["positive"] = ["4", 0]
        workflow["7"]["inputs"]["negative"] = ["5", 0]
        for node in ("12", "13", "14"):
            workflow.pop(node)
    else:
        workflow["14"]["inputs"]["strength"] = control_strength
    workflow["10"]["inputs"]["filename_prefix"] = f"{expression}/{vowel}"
    return workflow


def wait_for_result(base_url: str, prompt_id: str, timeout: int) -> dict:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        history = request_json(f"{base_url}/history/{prompt_id}")
        if prompt_id in history:
            result = history[prompt_id]
            status = result.get("status", {})
            if status.get("status_str") == "error":
                raise RuntimeError(json.dumps(status, ensure_ascii=False))
            if result.get("outputs"):
                return result
        time.sleep(1)
    raise TimeoutError(f"generation timed out: {prompt_id}")


def difference_ratio(neutral_path: Path, result_path: Path) -> float:
    neutral = Image.open(neutral_path).convert("RGBA")
    result = Image.open(result_path).convert("RGBA")
    diff = ImageChops.difference(neutral, result).convert("RGB")
    changed = sum(1 for pixel in diff.get_flattened_data() if pixel != (0, 0, 0))
    return changed / (neutral.width * neutral.height)


def remove_run_directory(path: Path) -> None:
    for attempt in range(10):
        try:
            shutil.rmtree(path)
            return
        except PermissionError:
            if attempt == 9:
                raise
            time.sleep(0.5)


def generation_plan(
    expressions: list[str] | None = None,
    vowels: list[str] | None = None,
) -> list[tuple[str, str, str, str]]:
    selected_expressions = expressions or list(EYE_EXPRESSIONS)
    selected_vowels = vowels or list(VOWELS)
    plan = [("eyes", key, key, "close") for key in selected_expressions]
    plan.extend(("mouth", key, "mouth_open", key) for key in selected_vowels)
    return plan


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--port", type=int, default=58120)
    parser.add_argument("--startup-timeout", type=int, default=600)
    parser.add_argument("--generation-timeout", type=int, default=900)
    parser.add_argument("--denoise", type=float, default=0.65)
    parser.add_argument("--blink-denoise", type=float, default=0.85)
    parser.add_argument("--control-strength", type=float, default=0.0)
    parser.add_argument("--identity-tags", default="")
    parser.add_argument("--limit", type=int)
    parser.add_argument("--expression", choices=EYE_EXPRESSIONS, action="append")
    parser.add_argument("--vowel", choices=VOWELS, action="append")
    args = parser.parse_args()
    if not 0.0 <= args.denoise <= 1.0 or not 0.0 <= args.blink_denoise <= 1.0:
        raise ValueError("denoise values must be between 0.0 and 1.0")
    if not 0.0 <= args.control_strength <= 1.0:
        raise ValueError("control strength must be between 0.0 and 1.0")
    if not 1 <= args.port <= 65535 or args.port == 8188:
        raise ValueError("port must be 1..65535 and must not use ComfyUI default 8188")

    repo = Path(__file__).resolve().parents[2]
    comfy = repo / "ComfyUI"
    python = repo / "sidecar" / ".venv" / "Scripts" / "python.exe"
    workflow_path = repo / "workflows" / "expression-inpaint-api.json"
    if not comfy.joinpath("main.py").is_file() or not python.is_file():
        raise FileNotFoundError("Run the T2 setup before expression generation")

    source = Image.open(args.input).convert("RGBA")
    if source.size != (1024, 1024):
        raise ValueError("input must be exactly 1024x1024 RGBA")

    run_id = uuid.uuid4().hex
    run_dir = repo / "temp" / f"expression-{run_id}"
    input_dir, comfy_output, temp_dir = (
        run_dir / name for name in ("input", "output", "tmp")
    )
    for directory in (input_dir, comfy_output, temp_dir, args.output):
        directory.mkdir(parents=True, exist_ok=True)
    source.save(input_dir / "neutral.png")
    template = json.loads(workflow_path.read_text(encoding="utf-8"))
    log_path = run_dir / "comfy.log"
    log = log_path.open("w", encoding="utf-8")
    command = [
        str(python),
        str(comfy / "main.py"),
        "--listen",
        "127.0.0.1",
        "--port",
        str(args.port),
        "--input-directory",
        str(input_dir),
        "--output-directory",
        str(comfy_output),
        "--temp-directory",
        str(temp_dir),
        "--disable-auto-launch",
        "--disable-all-custom-nodes",
        "--lowvram",
        "--preview-method",
        "none",
    ]
    print(json.dumps({"event": "starting_comfy", "port": args.port}), flush=True)
    process = subprocess.Popen(command, cwd=comfy, stdout=log, stderr=subprocess.STDOUT)
    base_url = f"http://127.0.0.1:{args.port}"
    metrics = []
    completed = False
    try:
        wait_for_server(base_url, process, args.startup_timeout)
        print(json.dumps({"event": "comfy_ready", "port": args.port}), flush=True)
        combinations = generation_plan(args.expression, args.vowel)
        if args.limit is not None:
            combinations = combinations[: args.limit]
        for index, (kind, key, expression, vowel) in enumerate(combinations):
            region = "eyes" if kind == "eyes" else "mouth"
            make_mask(input_dir / "mask.png", region)
            workflow = prepare_workflow(
                template,
                expression,
                vowel,
                1000 + index,
                args.blink_denoise if key == "blink" else args.denoise,
                args.control_strength,
                args.identity_tags,
            )
            workflow["10"]["inputs"]["filename_prefix"] = f"{kind}/{key}"
            started = time.monotonic()
            queued = request_json(f"{base_url}/prompt", {"prompt": workflow})
            result = wait_for_result(
                base_url, queued["prompt_id"], args.generation_timeout
            )
            images = result["outputs"]["10"]["images"]
            if len(images) != 1:
                raise RuntimeError(f"expected one output, got {len(images)}")
            item = images[0]
            generated = comfy_output / item.get("subfolder", "") / item["filename"]
            destination = args.output / kind / f"{key}.png"
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(generated, destination)
            ratio = difference_ratio(args.input, destination)
            metric = {
                "kind": kind,
                "key": key,
                "expression": expression,
                "vowel": vowel,
                "seconds": round(time.monotonic() - started, 2),
                "difference_ratio": round(ratio, 6),
            }
            metrics.append(metric)
            print(
                json.dumps({"event": "generated", **metric}, ensure_ascii=False),
                flush=True,
            )
        args.output.joinpath("metrics.json").write_text(
            json.dumps(metrics, ensure_ascii=False, indent=2), encoding="utf-8"
        )
        print(json.dumps({"event": "complete", "count": len(metrics)}), flush=True)
        completed = True
    finally:
        process.terminate()
        try:
            process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
        log.close()
        if process.returncode not in (0, 1, -15):
            print(
                f"ComfyUI exit code: {process.returncode}; log: {log_path}",
                file=sys.stderr,
            )
        if completed:
            remove_run_directory(run_dir)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
