use std::{
    env, fs,
    path::{Path, PathBuf},
};

use image::RgbaImage;
use local_vtuber_studio::facepatch::{
    CaptureFrame, NeutralMeshSnapshot, ProjectionSettings, ProjectionStats, load_neutral_snapshot,
    project_face_patch,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let model_path = required(&arguments, "--model")?;
    let neutral_path = required(&arguments, "--neutral")?;
    let atlas_path = required(&arguments, "--atlas")?;
    let frame_path = required(&arguments, "--frame")?;
    let diagnostics = required(&arguments, "--diagnostics")?;
    let settings = if let Some(settings_path) = optional(&arguments, "--settings")? {
        serde_json::from_slice(&fs::read(settings_path)?)?
    } else {
        ProjectionSettings::default()
    };
    let frame: CaptureFrame = serde_json::from_slice(&fs::read(frame_path)?)?;
    let mesh = load_neutral_snapshot(&model_path)?;
    let neutral = image::open(neutral_path)?.into_rgba8();
    let atlas = image::open(atlas_path)?.into_rgba8();

    if let Some(expression_dir) = optional(&arguments, "--expression-dir")? {
        let output_dir = required(&arguments, "--output-dir")?;
        run_batch(
            &mesh,
            &neutral,
            &atlas,
            &frame,
            &settings,
            &expression_dir,
            &output_dir,
            &diagnostics,
        )?;
    } else {
        let expression_path = required(&arguments, "--expression")?;
        let output_path = required(&arguments, "--output")?;
        let expression = image::open(expression_path)?.into_rgba8();
        let stats = run_one(
            &mesh,
            &neutral,
            &expression,
            &atlas,
            &frame,
            &settings,
            &output_path,
            &diagnostics,
        )?;
        println!(
            "{}",
            serde_json::json!({
                "event": "facepatch_complete",
                "output": output_path,
                "stats": output_path.with_extension("stats.json"),
                "written_pixels": stats.written_pixels,
                "bounding_box": stats.bounding_box,
            })
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_batch(
    mesh: &NeutralMeshSnapshot,
    neutral: &RgbaImage,
    atlas: &RgbaImage,
    frame: &CaptureFrame,
    settings: &ProjectionSettings,
    expression_dir: &Path,
    output_dir: &Path,
    diagnostics_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut inputs = Vec::new();
    for expression_entry in fs::read_dir(expression_dir)? {
        let expression_entry = expression_entry?;
        if !expression_entry.file_type()?.is_dir() {
            continue;
        }
        for vowel_entry in fs::read_dir(expression_entry.path())? {
            let vowel_entry = vowel_entry?;
            if vowel_entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                == Some("png")
            {
                inputs.push(vowel_entry.path());
            }
        }
    }
    inputs.sort();
    if inputs.len() != 36 {
        return Err(format!("表情入力は36 PNGが必要です: {}枚", inputs.len()).into());
    }

    let total = inputs.len();
    let mut metrics = Vec::new();
    for (index, input) in inputs.into_iter().enumerate() {
        let relative = input.strip_prefix(expression_dir)?;
        let output = output_dir.join(relative);
        let diagnostic = diagnostics_dir.join(relative.with_extension(""));
        let expression = image::open(&input)?.into_rgba8();
        let stats = run_one(
            mesh,
            neutral,
            &expression,
            atlas,
            frame,
            settings,
            &output,
            &diagnostic,
        )?;
        metrics.push(serde_json::json!({
            "key": relative.with_extension("").to_string_lossy().replace('\\', "/"),
            "written_pixels": stats.written_pixels,
            "padded_pixels": stats.padded_pixels,
            "selected_triangles": stats.selected_triangles,
            "bounding_box": stats.bounding_box,
        }));
        println!(
            "{}",
            serde_json::json!({
                "event": "facepatch_generated",
                "index": index + 1,
                "total": total,
                "key": relative.with_extension("").to_string_lossy().replace('\\', "/"),
                "written_pixels": stats.written_pixels,
            })
        );
    }
    fs::create_dir_all(output_dir)?;
    fs::write(
        output_dir.join("metrics.json"),
        serde_json::to_vec_pretty(&metrics)?,
    )?;
    println!(
        "{}",
        serde_json::json!({
            "event": "facepatch_batch_complete",
            "count": metrics.len(),
            "output": output_dir,
        })
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_one(
    mesh: &NeutralMeshSnapshot,
    neutral: &RgbaImage,
    expression: &RgbaImage,
    atlas: &RgbaImage,
    frame: &CaptureFrame,
    settings: &ProjectionSettings,
    output_path: &Path,
    diagnostics: &Path,
) -> Result<ProjectionStats, Box<dyn std::error::Error>> {
    let (projected, stats) = project_face_patch(
        mesh,
        neutral,
        expression,
        atlas,
        frame,
        settings,
        diagnostics,
    )?;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    projected.save(output_path)?;
    fs::write(
        output_path.with_extension("stats.json"),
        serde_json::to_vec_pretty(&stats)?,
    )?;
    Ok(stats)
}

fn optional(
    arguments: &[String],
    name: &str,
) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    let Some(index) = arguments.iter().position(|argument| argument == name) else {
        return Ok(None);
    };
    Ok(Some(
        arguments
            .get(index + 1)
            .ok_or_else(|| format!("{name} の値が必要です"))?
            .into(),
    ))
}

fn required(arguments: &[String], name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    optional(arguments, name)?.ok_or_else(|| format!("{name} が必要です").into())
}
