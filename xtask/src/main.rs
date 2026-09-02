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

fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let task = args.first().map(String::as_str).unwrap_or("help");
    match task {
        "dev" => run("cargo", &["tauri", "dev"]),
        "build" => run("cargo", &["tauri", "build"]),
        "setup" if args.get(1).map(String::as_str) == Some("comfy") => setup_comfy(),
        "setup" if args.get(1).map(String::as_str) == Some("sidecar") => setup_sidecar(),
        "setup" if args.get(1).map(String::as_str) == Some("models") => setup_models(),
        "expression" => run_expression(&args[1..]),
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
                "usage: cargo xtask <dev|build|verify|expression|mesh|rig|facepatch|setup comfy|setup sidecar|setup models>"
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
    )
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
