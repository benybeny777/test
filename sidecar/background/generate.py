from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.request
import uuid
from pathlib import Path


NEGATIVE = (
    "worst quality, low quality, blurry, character, person, human, face, body, "
    "text, logo, watermark, signature, frame, border"
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
            raise RuntimeError(f"ComfyUIが起動中に終了しました: {process.returncode}")
        try:
            request_json(f"{base_url}/system_stats", timeout=2)
            return
        except (urllib.error.URLError, TimeoutError):
            time.sleep(1)
    raise TimeoutError(f"ComfyUIが{timeout}秒以内に起動しませんでした")


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
    raise TimeoutError(f"背景生成が{timeout}秒以内に終わりませんでした")


def replace_atomic(source: Path, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_name(f".{destination.name}.{uuid.uuid4().hex}.part")
    shutil.copyfile(source, temporary)
    with temporary.open("r+b") as handle:
        os.fsync(handle.fileno())
    os.replace(temporary, destination)


def remove_run_directory(path: Path) -> None:
    for attempt in range(10):
        try:
            shutil.rmtree(path)
            return
        except PermissionError:
            if attempt == 9:
                raise
            time.sleep(0.5)


def prepare_workflow(template: dict, prompt: str, seed: int) -> dict:
    workflow = json.loads(json.dumps(template))
    workflow["2"]["inputs"]["text"] = (
        "masterpiece, best quality, anime background, environment only, no people, "
        f"wide establishing shot, {prompt}"
    )
    workflow["3"]["inputs"]["text"] = NEGATIVE
    workflow["5"]["inputs"]["seed"] = seed
    return workflow


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--prompt", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--port", type=int, default=58120)
    parser.add_argument("--startup-timeout", type=int, default=600)
    parser.add_argument("--generation-timeout", type=int, default=900)
    parser.add_argument("--seed", type=int, default=712904)
    parser.add_argument("--workflow", type=Path)
    args = parser.parse_args()
    if not args.prompt.strip():
        raise ValueError("背景プロンプトを入力してください")
    if not 1 <= args.port <= 65535 or args.port == 8188:
        raise ValueError("port must be 1..65535 and must not use ComfyUI default 8188")

    repo = Path(__file__).resolve().parents[2]
    comfy = repo / "ComfyUI"
    python = repo / "sidecar" / ".venv" / "Scripts" / "python.exe"
    workflow_path = args.workflow or repo / "workflows" / "background-txt2img-api.json"
    if not comfy.joinpath("main.py").is_file() or not python.is_file():
        raise FileNotFoundError("cargo xtask setup sidecar を先に実行してください")
    template = json.loads(workflow_path.read_text(encoding="utf-8"))
    run_dir = repo / "temp" / f"background-{uuid.uuid4().hex}"
    output_dir, temp_dir = run_dir / "output", run_dir / "tmp"
    output_dir.mkdir(parents=True)
    temp_dir.mkdir()
    log_path = run_dir / "comfy.log"
    log = log_path.open("w", encoding="utf-8")
    command = [
        str(python), str(comfy / "main.py"), "--listen", "127.0.0.1", "--port", str(args.port),
        "--output-directory", str(output_dir), "--temp-directory", str(temp_dir),
        "--disable-auto-launch", "--disable-all-custom-nodes", "--lowvram", "--preview-method", "none",
    ]
    process = subprocess.Popen(command, cwd=comfy, stdout=log, stderr=subprocess.STDOUT)
    base_url = f"http://127.0.0.1:{args.port}"
    completed = False
    try:
        print(json.dumps({"event": "starting_comfy", "port": args.port}), flush=True)
        wait_for_server(base_url, process, args.startup_timeout)
        started = time.monotonic()
        queued = request_json(
            f"{base_url}/prompt",
            {"prompt": prepare_workflow(template, args.prompt.strip(), args.seed)},
        )
        result = wait_for_result(base_url, queued["prompt_id"], args.generation_timeout)
        images = result["outputs"]["7"]["images"]
        if len(images) != 1:
            raise RuntimeError(f"背景出力が1枚ではありません: {len(images)}")
        item = images[0]
        generated = output_dir / item.get("subfolder", "") / item["filename"]
        replace_atomic(generated, args.output)
        seconds = round(time.monotonic() - started, 2)
        print(json.dumps({"event": "background_complete", "seconds": seconds, "output": str(args.output)}, ensure_ascii=False), flush=True)
        completed = True
    finally:
        process.terminate()
        try:
            process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
        log.close()
        if completed:
            remove_run_directory(run_dir)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
