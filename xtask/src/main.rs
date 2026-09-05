use std::{
    env,
    ffi::OsStr,
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

const COMFY_TAG: &str = "v0.34.0";
const COMFY_COMMIT: &str = "12d5279438bfefc058a269eae805ceab6047777f";
const ANIMAGINE_SHA256: &str = "6327eca98bfb6538dd7a4edce22484a1bbc57a8cff6b11d075d40da1afb847ac";
const CONTROLNET_SHA256: &str = "ea99040544a999f814fd854575a3aee069a005d026864c8d321b82576706a221";
const TRIPOSR_COMMIT: &str = "107cefdc244c39106fa830359024f6a2f1c78871";
const TRIPOSR_MODEL_SHA256: &str =
    "429e2c6b22a0923967459de24d67f05962b235f79cde6b032aa7ed2ffcd970ee";
const TRIPOSR_CONFIG_SHA256: &str =
    "74ca708ce086bf68e97709ea6b3d91f14717921c04691e84043f0eb8fcc68e62";
const ISNET_ANIME_SHA256: &str = "f15622d853e8260172812b657053460e20806f04b9e05147d49af7bed31a6e99";
const DINO_CONFIG_SHA256: &str = "b87c0270b97db085fd82cf114a761fd0f62ae7914fbd407c752a2260646b689c";
const SAM2_REVISION: &str = "de431c4043854a71d8101e17995dfe596bf101a5";
const SAM2_MODEL_SHA256: &str = "48c14467e5cf9e51870511feb72c89688e82dd74523142c0538b663e193ac2a7";
const SAM2_CONFIG_SHA256: &str = "860aff9751b139d83a4ad7df1e5535416fded533e0ead02625edbefcb9953cce";
const SAM2_PREPROCESSOR_SHA256: &str =
    "6ebf229ee259368ce4a8d4f2fe893a72b053023710853e257253939e601f583d";
const SAM2_PROCESSOR_SHA256: &str =
    "f8a68e865cfad115c1c2763f3d93eca7b1c622da06da2a9273eb437fb2389b6d";
const LLAMA_ZIP_SHA256: &str = "81c2ff62e14b549cd5c766ccdd5c61f09e821a171655c3047bdccfddc2d1a1e2";
const LLAMA_CUDART_SHA256: &str =
    "8c79a9b226de4b3cacfd1f83d24f962d0773be79f1e7b75c6af4ded7e32ae1d6";
const WHISPER_ZIP_SHA256: &str = "c1b17166e1e31a91cc8e9c1f910d3785e3ce757bb2958bf9dce13fdb4880005f";
const QWEN_SHA256: &str = "6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e";
const WHISPER_MODEL_SHA256: &str =
    "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b";

fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let task = args.first().map(String::as_str).unwrap_or("help");
    match task {
        "dev" => run("cargo", &["tauri", "dev"]),
        "build" => run("cargo", &["tauri", "build"]),
        "setup" if args.get(1).map(String::as_str) == Some("comfy") => setup_comfy(),
        "setup" if args.get(1).map(String::as_str) == Some("sidecar") => setup_sidecar(),
        "setup" if args.get(1).map(String::as_str) == Some("models") => setup_models(),
        "setup" if args.get(1).map(String::as_str) == Some("sam2") => setup_sam2(),
        "setup" if args.get(1).map(String::as_str) == Some("grounding") => setup_grounding(),
        "setup" if args.get(1).map(String::as_str) == Some("engines") => setup_engines(),
        "expression" => run_expression(&args[1..]),
        "expression-import" => run_expression_import(&args[1..]),
        "mesh" => run_mesh(&args[1..]),
        "rig" => run_rig(&args[1..]),
        "facepatch" => run_facepatch(&args[1..]),
        "verify" => {
            run("cargo", &["fmt", "--all", "--", "--check"])?;
            run(
                "cargo",
                &[
                    "clippy",
                    "--workspace",
                    "--all-targets",
                    "--",
                    "-D",
                    "warnings",
                ],
            )?;
            run("cargo", &["test", "--workspace"])?;
            verify_python_sidecars()
        }
        _ => {
            eprintln!(
                "usage: cargo xtask <dev|build|verify|expression|expression-import|mesh|rig|facepatch|setup comfy|setup sidecar|setup models|setup sam2|setup grounding|setup engines>"
            );
            Ok(())
        }
    }
}

fn root() -> Result<PathBuf> {
    Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("xtask must be inside the workspace")?
        .to_owned())
}

fn setup_comfy() -> Result<()> {
    let root = root()?;
    let comfy = root.join("ComfyUI");
    if !comfy.exists() {
        run_at(
            &root,
            "git",
            [
                OsStr::new("clone"),
                OsStr::new("--branch"),
                OsStr::new(COMFY_TAG),
                OsStr::new("--depth"),
                OsStr::new("1"),
                OsStr::new("https://github.com/Comfy-Org/ComfyUI.git"),
                comfy.as_os_str(),
            ],
        )?;
    }
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&comfy)
        .output()
        .context("failed to inspect ComfyUI revision")?;
    let actual = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !output.status.success() || actual != COMFY_COMMIT {
        bail!("ComfyUI revision mismatch: expected {COMFY_COMMIT}, got {actual}");
    }
    println!("ComfyUI {COMFY_TAG} ({COMFY_COMMIT}) is ready");
    Ok(())
}

fn setup_sidecar() -> Result<()> {
    let root = root()?;
    check_cuda_gpu(&root)?;
    let python = root.join("sidecar/.venv/Scripts/python.exe");
    if !python.exists() {
        run_at(
            &root,
            "uv",
            [
                OsStr::new("venv"),
                OsStr::new("--python"),
                OsStr::new("3.12.13"),
                OsStr::new("sidecar/.venv"),
            ],
        )?;
    }
    run_at(
        &root,
        "uv",
        [
            OsStr::new("pip"),
            OsStr::new("sync"),
            OsStr::new("--python"),
            python.as_os_str(),
            OsStr::new("sidecar/requirements-comfy.lock"),
            OsStr::new("--extra-index-url"),
            OsStr::new("https://download.pytorch.org/whl/cu128"),
            OsStr::new("--index-strategy"),
            OsStr::new("unsafe-best-match"),
            OsStr::new("--require-hashes"),
        ],
    )?;
    run_at(
        &root,
        &python,
        [
            OsStr::new("-c"),
            OsStr::new(
                "import torch; assert torch.cuda.is_available(); print(torch.__version__, torch.cuda.get_device_name(0))",
            ),
        ],
    )?;
    setup_triposr_runtime(&root)
}

fn setup_triposr_runtime(root: &Path) -> Result<()> {
    let runtime = root.join("sidecar/runtime/TripoSR");
    if !runtime.exists() {
        std::fs::create_dir_all(runtime.parent().context("TripoSR runtime parent missing")?)?;
        run_at(
            root,
            "git",
            [
                OsStr::new("clone"),
                OsStr::new("https://github.com/VAST-AI-Research/TripoSR.git"),
                runtime.as_os_str(),
            ],
        )?;
        run_at(
            &runtime,
            "git",
            [OsStr::new("checkout"), OsStr::new(TRIPOSR_COMMIT)],
        )?;
    }
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&runtime)
        .output()
        .context("failed to inspect TripoSR revision")?;
    let actual = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !output.status.success() || actual != TRIPOSR_COMMIT {
        bail!("TripoSR revision mismatch: expected {TRIPOSR_COMMIT}, got {actual}");
    }
    println!("TripoSR {TRIPOSR_COMMIT} is ready");
    Ok(())
}

fn setup_models() -> Result<()> {
    let root = root()?;
    check_cuda_gpu(&root)?;
    let checkpoints = root.join("ComfyUI/models/checkpoints");
    let controlnet = root.join("ComfyUI/models/controlnet");
    let triposr = root.join("models/triposr");
    let rembg = root.join("models/rembg");
    let dino = root.join("models/dino-vitb16");
    std::fs::create_dir_all(&checkpoints)?;
    std::fs::create_dir_all(&controlnet)?;
    std::fs::create_dir_all(&triposr)?;
    std::fs::create_dir_all(&rembg)?;
    std::fs::create_dir_all(&dino)?;
    download_verified(
        &root,
        "https://huggingface.co/cagliostrolab/animagine-xl-4.0/resolve/2b7c1b397761bf5bd3cc42e5b39ec99314a75a96/animagine-xl-4.0-opt.safetensors?download=true",
        &checkpoints.join("animagine-xl-4.0-opt.safetensors"),
        ANIMAGINE_SHA256,
    )?;
    download_verified(
        &root,
        "https://huggingface.co/diffusers/controlnet-canny-sdxl-1.0/resolve/eb115a19a10d14909256db740ed109532ab1483c/diffusion_pytorch_model.safetensors?download=true",
        &controlnet.join("diffusion_pytorch_model.safetensors"),
        CONTROLNET_SHA256,
    )?;
    download_verified(
        &root,
        "https://huggingface.co/stabilityai/TripoSR/resolve/5b521936b01fbe1890f6f9baed0254ab6351c04a/model.ckpt?download=true",
        &triposr.join("model.ckpt"),
        TRIPOSR_MODEL_SHA256,
    )?;
    download_verified(
        &root,
        "https://huggingface.co/stabilityai/TripoSR/resolve/5b521936b01fbe1890f6f9baed0254ab6351c04a/config.yaml?download=true",
        &triposr.join("config.yaml"),
        TRIPOSR_CONFIG_SHA256,
    )?;
    download_verified(
        &root,
        "https://huggingface.co/skytnt/anime-seg/resolve/493cb60893f47441b26ec4fb9a306bce9e342982/isnetis.onnx?download=true",
        &rembg.join("isnetis.onnx"),
        ISNET_ANIME_SHA256,
    )?;
    download_verified(
        &root,
        "https://huggingface.co/facebook/dino-vitb16/resolve/f205d5d8e640a89a2b8ef0369670dfc37cc07fc2/config.json?download=true",
        &dino.join("config.json"),
        DINO_CONFIG_SHA256,
    )?;
    setup_sam2()?;
    setup_grounding()
}

fn setup_grounding() -> Result<()> {
    let root = root()?;
    check_cuda_gpu(&root)?;
    let destination = root.join("models/grounding-dino-base");
    std::fs::create_dir_all(&destination)?;
    let base = "https://huggingface.co/IDEA-Research/grounding-dino-base/resolve/12bdfa3120f3e7ec7b434d90674b3396eccf88eb";
    for (name, hash) in [
        (
            "README.md",
            "a0d03193076262a585dcb1edfe4b3b72fac678055008b688feb188cceb7f977d",
        ),
        (
            "config.json",
            "eda416dae6f49419ff831b1c190ec430a060b19aae688dbaf2425a075b650608",
        ),
        (
            "model.safetensors",
            "5548f844c928c4b6f411fa8cbcc2bfa8dbbba437cb1d513975519f93c2a9ed21",
        ),
        (
            "preprocessor_config.json",
            "8454179ba95e2ad22947835aad7b45862a601fc0055ab88bf1ee70892d3aea60",
        ),
        (
            "special_tokens_map.json",
            "b6d346be366a7d1d48332dbc9fdf3bf8960b5d879522b7799ddba59e76237ee3",
        ),
        (
            "tokenizer.json",
            "d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66",
        ),
        (
            "tokenizer_config.json",
            "d40ab645b68211910b9170d22433d43186a6ec8ee6fd10ba170524b25bf4fb56",
        ),
        (
            "vocab.txt",
            "07eced375cec144d27c900241f3e339478dec958f92fddbc551f295c992038a3",
        ),
    ] {
        download_verified(
            &root,
            &format!("{base}/{name}?download=true"),
            &destination.join(name),
            hash,
        )?;
    }
    Ok(())
}

fn setup_sam2() -> Result<()> {
    let root = root()?;
    check_cuda_gpu(&root)?;
    let sam2 = root.join("models/sam2.1-hiera-tiny");
    std::fs::create_dir_all(&sam2)?;
    let sam2_base =
        format!("https://huggingface.co/facebook/sam2.1-hiera-tiny/resolve/{SAM2_REVISION}");
    for (name, hash) in [
        ("config.json", SAM2_CONFIG_SHA256),
        ("preprocessor_config.json", SAM2_PREPROCESSOR_SHA256),
        ("processor_config.json", SAM2_PROCESSOR_SHA256),
        ("model.safetensors", SAM2_MODEL_SHA256),
    ] {
        download_verified(
            &root,
            &format!("{sam2_base}/{name}?download=true"),
            &sam2.join(name),
            hash,
        )?;
    }
    Ok(())
}

fn setup_engines() -> Result<()> {
    let root = root()?;
    check_cuda_gpu(&root)?;
    let downloads = root.join("temp/engine-downloads");
    let staging = root.join("engines/.staging");
    let llama = root.join("engines/llama");
    let whisper = root.join("engines/whisper");
    let models_llm = root.join("models/llm");
    let models_stt = root.join("models/stt");
    let llama_cli = llama.join("llama-cli.exe");
    let whisper_cli = whisper.join("whisper-cli.exe");
    let binaries_ready = command_output_contains(&llama_cli, "--version", "build 10621")
        && command_output_contains(&whisper_cli, "--version", "1.9.3");
    if binaries_ready {
        std::fs::create_dir_all(&models_llm)?;
        std::fs::create_dir_all(&models_stt)?;
        download_engine_models(&root, &models_llm, &models_stt)?;
        println!("llama.cpp b10621 と whisper.cpp b4938 は検証済みです");
        return Ok(());
    }
    if llama.exists() || whisper.exists() {
        bail!(
            "既存エンジンが固定版と一致しません。engines/llama と engines/whisper を退避してから再実行してください"
        );
    }
    std::fs::create_dir_all(&downloads)?;
    let llama_zip = downloads.join("llama-cuda.zip");
    let cudart_zip = downloads.join("llama-cudart.zip");
    let whisper_zip = downloads.join("whisper-cuda.zip");
    download_verified(
        &root,
        "https://github.com/ggml-org/llama.cpp/releases/download/b10621/llama-b10621-bin-win-cuda-12.4-x64.zip",
        &llama_zip,
        LLAMA_ZIP_SHA256,
    )?;
    download_verified(
        &root,
        "https://github.com/ggml-org/llama.cpp/releases/download/b10621/cudart-llama-bin-win-cuda-12.4-x64.zip",
        &cudart_zip,
        LLAMA_CUDART_SHA256,
    )?;
    download_verified(
        &root,
        "https://github.com/ggml-org/whisper.cpp/releases/download/b4938/whisper-cublas-12.4.0-bin-x64.zip",
        &whisper_zip,
        WHISPER_ZIP_SHA256,
    )?;
    if staging.exists() {
        ensure_child(&root, &staging)?;
        std::fs::remove_dir_all(&staging)?;
    }
    let llama_staging = staging.join("llama");
    let whisper_staging = staging.join("whisper");
    std::fs::create_dir_all(&llama_staging)?;
    std::fs::create_dir_all(&whisper_staging)?;
    extract_flat(&root, &llama_zip, &llama_staging)?;
    extract_flat(&root, &cudart_zip, &llama_staging)?;
    extract_flat(&root, &whisper_zip, &whisper_staging)?;
    if !llama_staging.join("llama-cli.exe").is_file()
        || !whisper_staging.join("whisper-cli.exe").is_file()
    {
        bail!("推論エンジンの展開物にCLIがありません");
    }
    std::fs::rename(&llama_staging, &llama)?;
    std::fs::rename(&whisper_staging, &whisper)?;
    std::fs::create_dir_all(&models_llm)?;
    std::fs::create_dir_all(&models_stt)?;
    download_engine_models(&root, &models_llm, &models_stt)?;
    if staging.exists() {
        ensure_child(&root, &staging)?;
        std::fs::remove_dir_all(&staging)?;
    }
    for file in [llama_zip, cudart_zip, whisper_zip] {
        std::fs::remove_file(file)?;
    }
    println!("llama.cpp b10621 と whisper.cpp b4938 を検証して配置しました");
    Ok(())
}

fn download_engine_models(root: &Path, models_llm: &Path, models_stt: &Path) -> Result<()> {
    download_verified(
        root,
        "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/91cad51170dc346986eccefdc2dd33a9da36ead9/qwen2.5-1.5b-instruct-q4_k_m.gguf?download=true",
        &models_llm.join("qwen2.5-1.5b-instruct-q4_k_m.gguf"),
        QWEN_SHA256,
    )?;
    download_verified(
        root,
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-small.bin?download=true",
        &models_stt.join("ggml-small.bin"),
        WHISPER_MODEL_SHA256,
    )?;
    Ok(())
}

fn command_output_contains(program: &Path, argument: &str, expected: &str) -> bool {
    Command::new(program)
        .arg(argument)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| {
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            combined.contains(expected)
        })
}

fn extract_flat(root: &Path, archive: &Path, destination: &Path) -> Result<()> {
    let unpacked = destination.join(".unpacked");
    std::fs::create_dir_all(&unpacked)?;
    run_at(
        root,
        "tar.exe",
        [
            OsStr::new("-xf"),
            archive.as_os_str(),
            OsStr::new("-C"),
            unpacked.as_os_str(),
        ],
    )?;
    copy_files_flat(&unpacked, destination)?;
    ensure_child(root, &unpacked)?;
    std::fs::remove_dir_all(unpacked)?;
    Ok(())
}

fn copy_files_flat(source: &Path, destination: &Path) -> Result<()> {
    for entry in std::fs::read_dir(source)? {
        let path = entry?.path();
        if path.is_dir() {
            copy_files_flat(&path, destination)?;
        } else if let Some(name) = path.file_name() {
            std::fs::copy(&path, destination.join(name))?;
        }
    }
    Ok(())
}

fn ensure_child(root: &Path, path: &Path) -> Result<()> {
    let root = root.canonicalize()?;
    let target = path.canonicalize()?;
    if !target.starts_with(&root) || target == root {
        bail!(
            "削除対象がリポジトリ配下ではありません: {}",
            target.display()
        );
    }
    Ok(())
}

fn run_mesh(args: &[String]) -> Result<()> {
    let root = root()?;
    check_cuda_gpu(&root)?;
    let python = root.join("sidecar/.venv/Scripts/python.exe");
    let script = root.join("sidecar/mesh/generate.py");
    let status = Command::new(&python)
        .arg(script)
        .args(args)
        .current_dir(&root)
        .status()
        .context("failed to start mesh generator")?;
    if !status.success() {
        bail!("mesh generator failed with {status}");
    }
    Ok(())
}

fn run_expression(args: &[String]) -> Result<()> {
    let root = root()?;
    let python = root.join("sidecar/.venv/Scripts/python.exe");
    let script = root.join("sidecar/expression/generate.py");
    let mut command = Command::new(&python);
    command.arg(script).args(args).current_dir(&root);
    let status = command
        .status()
        .context("failed to start expression generator")?;
    if !status.success() {
        bail!("expression generator failed with {status}");
    }
    Ok(())
}

fn run_expression_import(args: &[String]) -> Result<()> {
    let root = root()?;
    let python = root.join("sidecar/.venv/Scripts/python.exe");
    let script = root.join("sidecar/expression/import_image.py");
    let status = Command::new(&python)
        .arg(script)
        .args(args)
        .current_dir(&root)
        .status()
        .context("failed to start expression importer")?;
    if !status.success() {
        bail!("expression importer failed with {status}");
    }
    Ok(())
}

fn run_rig(args: &[String]) -> Result<()> {
    let root = root()?;
    let python = root.join("sidecar/.venv/Scripts/python.exe");
    let script = root.join("sidecar/rigging/generate.py");
    let status = Command::new(&python)
        .arg(script)
        .args(args)
        .current_dir(&root)
        .status()
        .context("failed to start rigging generator")?;
    if !status.success() {
        bail!("rigging generator failed with {status}");
    }
    Ok(())
}

fn run_facepatch(args: &[String]) -> Result<()> {
    let root = root()?;
    let status = Command::new("cargo")
        .args([
            "run",
            "-p",
            "local-vtuber-studio",
            "--bin",
            "facepatch",
            "--",
        ])
        .args(args)
        .current_dir(root)
        .status()
        .context("failed to start facepatch projector")?;
    if !status.success() {
        bail!("facepatch projector failed with {status}");
    }
    Ok(())
}

fn verify_python_sidecars() -> Result<()> {
    let root = root()?;
    let python = root.join("sidecar/.venv/Scripts/python.exe");
    if !python.exists() {
        bail!("sidecar environment is missing; run `cargo xtask setup sidecar`");
    }
    run_at(
        &root,
        &python,
        [
            OsStr::new("-m"),
            OsStr::new("unittest"),
            OsStr::new("discover"),
            OsStr::new("-s"),
            OsStr::new("sidecar/expression"),
            OsStr::new("-p"),
            OsStr::new("test_*.py"),
        ],
    )?;
    run_at(
        &root,
        &python,
        [
            OsStr::new("-m"),
            OsStr::new("unittest"),
            OsStr::new("discover"),
            OsStr::new("-s"),
            OsStr::new("sidecar/isolate"),
            OsStr::new("-p"),
            OsStr::new("test_*.py"),
        ],
    )?;
    run_at(
        &root,
        &python,
        [
            OsStr::new("-m"),
            OsStr::new("unittest"),
            OsStr::new("discover"),
            OsStr::new("-s"),
            OsStr::new("sidecar/rig2d"),
            OsStr::new("-p"),
            OsStr::new("test_*.py"),
        ],
    )?;
    run_at(
        &root,
        &python,
        [
            OsStr::new("-m"),
            OsStr::new("unittest"),
            OsStr::new("discover"),
            OsStr::new("-s"),
            OsStr::new("sidecar/decompose"),
            OsStr::new("-p"),
            OsStr::new("test_*.py"),
        ],
    )?;
    run_at(
        &root,
        &python,
        [
            OsStr::new("-m"),
            OsStr::new("unittest"),
            OsStr::new("discover"),
            OsStr::new("-s"),
            OsStr::new("sidecar/background"),
            OsStr::new("-p"),
            OsStr::new("test_*.py"),
        ],
    )?;
    run_at(
        &root,
        &python,
        [
            OsStr::new("-m"),
            OsStr::new("unittest"),
            OsStr::new("discover"),
            OsStr::new("-s"),
            OsStr::new("sidecar/mesh"),
            OsStr::new("-p"),
            OsStr::new("test_*.py"),
        ],
    )?;
    run_at(
        &root,
        python,
        [
            OsStr::new("-m"),
            OsStr::new("unittest"),
            OsStr::new("discover"),
            OsStr::new("-s"),
            OsStr::new("sidecar/rigging"),
            OsStr::new("-p"),
            OsStr::new("test_*.py"),
        ],
    )
}

fn check_cuda_gpu(root: &Path) -> Result<()> {
    let status = Command::new("nvidia-smi")
        .arg("--query-gpu=name,memory.total")
        .arg("--format=csv,noheader")
        .current_dir(root)
        .status()
        .context("CUDA対応NVIDIA GPUを確認できません。nvidia-smi が必要です")?;
    if !status.success() {
        bail!("CUDA対応NVIDIA GPUが必要です。CPUフォールバックはありません");
    }
    Ok(())
}

fn download_verified(root: &Path, url: &str, destination: &Path, expected: &str) -> Result<()> {
    if destination.exists() && sha256(destination)? == expected {
        println!("verified {}", destination.display());
        return Ok(());
    }
    let partial = destination.with_extension("download");
    run_at(
        root,
        "curl.exe",
        [
            OsStr::new("--location"),
            OsStr::new("--fail"),
            OsStr::new("--retry"),
            OsStr::new("3"),
            OsStr::new("--output"),
            partial.as_os_str(),
            OsStr::new(url),
        ],
    )?;
    let actual = sha256(&partial)?;
    if actual != expected {
        let _ = std::fs::remove_file(&partial);
        bail!(
            "SHA-256 mismatch for {}: expected {expected}, got {actual}",
            destination.display()
        );
    }
    if destination.exists() {
        std::fs::remove_file(destination)?;
    }
    std::fs::rename(&partial, destination)?;
    println!("downloaded and verified {}", destination.display());
    Ok(())
}

fn sha256(path: &Path) -> Result<String> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn run_at<I, S>(root: &Path, program: impl AsRef<OsStr>, args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let status = Command::new(&program)
        .args(args)
        .current_dir(root)
        .status()
        .with_context(|| format!("failed to start {}", program.as_ref().to_string_lossy()))?;
    if !status.success() {
        bail!(
            "{} failed with {status}",
            program.as_ref().to_string_lossy()
        );
    }
    Ok(())
}

fn run(program: &str, args: &[&str]) -> Result<()> {
    let root = root()?;
    let status = Command::new(program)
        .args(args)
        .current_dir(root)
        .status()
        .with_context(|| format!("failed to start {program}"))?;
    if !status.success() {
        bail!("{program} {} failed with {status}", args.join(" "));
    }
    Ok(())
}
