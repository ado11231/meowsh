//! Reads the settings in `assets/animations/` & writes them as Rust for `src/anim.rs`.
//! A missing or broken setting stops the build with a message that names the file.

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

/// The folder that holds `canvas.toml` & one folder per animation.
const ANIMATIONS_FOLDER: &str = "assets/animations";

/// The shared size & floor file, inside the animations folder.
const CANVAS_FILE: &str = "canvas.toml";

/// The settings file inside each animation folder.
const ANIM_FILE: &str = "anim.toml";

/// The file written into the build folder & included by `src/anim.rs`.
const OUTPUT_FILE: &str = "animations.rs";

/// `canvas.toml`: the size every frame shares & the row the feet stand on.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CanvasSettings {
    width: u32,
    height: u32,
    floor: u32,
}

/// One animation's `anim.toml`. Defaults match the table in docs/ANIMATION.md.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnimSettings {
    frame_ms: u32,
    #[serde(default, rename = "loop")]
    loops: bool,
    #[serde(default)]
    step: u32,
    #[serde(default = "recolor_default")]
    recolor: bool,
    frame_ms_each: Option<Vec<u32>>,
}

/// `recolor` is on unless an animation turns it off.
fn recolor_default() -> bool {
    true
}

fn main() -> Result<()> {
    println!("cargo::rerun-if-changed={ANIMATIONS_FOLDER}");

    let folder = Path::new(ANIMATIONS_FOLDER);
    let canvas = read_canvas(&folder.join(CANVAS_FILE))?;
    let animations = read_animations(folder)?;

    let output_folder = std::env::var_os("OUT_DIR").context("OUT_DIR is not set")?;
    let output_path = Path::new(&output_folder).join(OUTPUT_FILE);
    fs::write(&output_path, write_rust(&canvas, &animations)?)
        .with_context(|| format!("could not write {}", output_path.display()))?;
    Ok(())
}

/// Reads & checks `canvas.toml`.
fn read_canvas(path: &Path) -> Result<CanvasSettings> {
    let canvas: CanvasSettings = read_toml(path)?;
    ensure!(
        canvas.width > 0 && canvas.height > 0,
        "{}: width & height must be more than 0",
        path.display()
    );
    ensure!(
        canvas.floor < canvas.height,
        "{}: floor must be a row inside the height",
        path.display()
    );
    Ok(canvas)
}

/// Reads & checks the `anim.toml` of every animation folder, sorted by name.
fn read_animations(folder: &Path) -> Result<Vec<(String, AnimSettings)>> {
    let entries =
        fs::read_dir(folder).with_context(|| format!("could not read {}", folder.display()))?;
    let mut animations = Vec::new();
    for entry in entries {
        let path = entry
            .with_context(|| format!("could not read {}", folder.display()))?
            .path();
        if !path.is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .with_context(|| format!("{}: folder name is not plain text", path.display()))?
            .to_owned();
        if !is_plain_name(&name) {
            bail!(
                "{}: use only lowercase letters, digits & _ in an animation's name",
                path.display()
            );
        }
        let settings = read_anim(&path.join(ANIM_FILE))?;
        animations.push((name, settings));
    }
    animations.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(animations)
}

/// Reads & checks one `anim.toml`.
fn read_anim(path: &Path) -> Result<AnimSettings> {
    let settings: AnimSettings = read_toml(path)?;
    ensure!(
        settings.frame_ms > 0,
        "{}: frame_ms must be more than 0",
        path.display()
    );
    if let Some(times) = &settings.frame_ms_each {
        ensure!(
            times.iter().all(|&time| time > 0),
            "{}: every time in frame_ms_each must be more than 0",
            path.display()
        );
    }
    Ok(settings)
}

/// Reads a TOML file into `T`, naming the file if it is missing or wrong.
fn read_toml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let text =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("{} is not right", path.display()))
}

/// True when a name is only lowercase letters, digits & `_`, so it is safe to use in code.
fn is_plain_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|letter| letter.is_ascii_lowercase() || letter.is_ascii_digit() || letter == '_')
}

/// Writes the settings as Rust constants of the types in `src/anim.rs`.
fn write_rust(canvas: &CanvasSettings, animations: &[(String, AnimSettings)]) -> Result<String> {
    let mut rust = String::new();
    writeln!(
        rust,
        "pub const CANVAS: Canvas = Canvas {{ width: {}, height: {}, floor: {} }};",
        canvas.width, canvas.height, canvas.floor
    )?;
    writeln!(rust, "pub const ANIMATIONS: &[Animation] = &[")?;
    for (name, settings) in animations {
        let frame_ms_each = match &settings.frame_ms_each {
            Some(times) => format!("Some(&{times:?})"),
            None => "None".to_owned(),
        };
        writeln!(
            rust,
            "    Animation {{ name: {name:?}, frame_ms: {}, loops: {}, step: {}, recolor: {}, frame_ms_each: {frame_ms_each} }},",
            settings.frame_ms, settings.loops, settings.step, settings.recolor
        )?;
    }
    writeln!(rust, "];")?;
    Ok(rust)
}
