use std::{env, path::PathBuf, process::Command};

use anyhow::{Context, Result, bail};

fn main() -> Result<()> {
    let task = env::args().nth(1).unwrap_or_else(|| "help".to_owned());
    match task.as_str() {
        "dev" => run("cargo", &["tauri", "dev"]),
        "build" => run("cargo", &["tauri", "build"]),
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
            run("cargo", &["test", "--workspace"])
        }
        _ => {
            eprintln!("usage: cargo xtask <dev|build|verify>");
            Ok(())
        }
    }
}

fn run(program: &str, args: &[&str]) -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("xtask must be inside the workspace")?
        .to_owned();
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
