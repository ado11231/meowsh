//! Reads the settings & checks the frames in `assets/animations/`, then writes them as Rust:
//! the settings for `src/anim.rs` & the packed frames for `src/art/library.rs`. A broken setting or frame stops the build with a message that names
//! the file. The frame rules are in docs/ANIMATION.md.

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::Path;

/// The folder that holds `canvas.toml` & one folder per animation.
const ANIMATIONS_FOLDER: &str = "assets/animations";

/// The shared size & floor file, inside the animations folder.
const CANVAS_FILE: &str = "canvas.toml";

/// The settings file inside each animation folder.
const ANIM_FILE: &str = "anim.toml";

/// The file written into the build folder & included by `src/anim.rs`.
const OUTPUT_FILE: &str = "animations.rs";

/// The file written into the build folder & included by `src/art/library.rs`.
const PACKED_FILE: &str = "packed_frames.rs";

/// Bytes per pixel in a frame: red, green, blue & how see through it is.
const BYTES_PER_PIXEL: usize = 4;

/// Where the "how see through" byte sits in each pixel.
const ALPHA_INDEX: usize = 3;

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

/// One animation folder that passed every check.
struct FoundAnimation {
    name: String,
    settings: AnimSettings,
    frame_count: usize,
}

/// `recolor` is on unless an animation turns it off.
fn recolor_default() -> bool {
    true
}

fn main() -> Result<()> {
    println!("cargo::rerun-if-changed={ANIMATIONS_FOLDER}");

    let folder = Path::new(ANIMATIONS_FOLDER);
    let canvas = read_canvas(&folder.join(CANVAS_FILE))?;
    let animations = read_animations(folder, &canvas)?;

    let output_folder = std::env::var_os("OUT_DIR").context("OUT_DIR is not set")?;
    let output_folder = Path::new(&output_folder);
    write_file(
        &output_folder.join(OUTPUT_FILE),
        &write_rust(&canvas, &animations)?,
    )?;
    let package_folder =
        std::env::var_os("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR is not set")?;
    write_file(
        &output_folder.join(PACKED_FILE),
        &write_packed(&Path::new(&package_folder).join(folder), &animations)?,
    )?;
    Ok(())
}

/// Writes one generated file, naming it if that fails.
fn write_file(path: &Path, text: &str) -> Result<()> {
    fs::write(path, text).with_context(|| format!("could not write {}", path.display()))
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

/// Reads & checks the `anim.toml` & frames of every animation folder, sorted by name.
fn read_animations(folder: &Path, canvas: &CanvasSettings) -> Result<Vec<FoundAnimation>> {
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
        let frame_count = check_frames(&path, canvas)?;
        if let Some(times) = &settings.frame_ms_each {
            ensure!(
                times.len() == frame_count,
                "{}: frame_ms_each has {} times but there are {frame_count} frames",
                path.join(ANIM_FILE).display(),
                times.len()
            );
        }
        animations.push(FoundAnimation {
            name,
            settings,
            frame_count,
        });
    }
    animations.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(animations)
}

/// Checks the frames in one animation folder & returns how many there are. They must be
/// named `00.png`, `01.png` & so on with no gaps, & each must pass [`check_frame`].
fn check_frames(folder: &Path, canvas: &CanvasSettings) -> Result<usize> {
    let entries =
        fs::read_dir(folder).with_context(|| format!("could not read {}", folder.display()))?;
    let mut file_names = Vec::new();
    for entry in entries {
        let file_name = entry
            .with_context(|| format!("could not read {}", folder.display()))?
            .file_name();
        let file_name = file_name
            .to_str()
            .with_context(|| format!("{}: a file name is not plain text", folder.display()))?
            .to_owned();
        // Hidden files, such as the .DS_Store macOS leaves behind, are not frames.
        if file_name != ANIM_FILE && !file_name.starts_with('.') {
            file_names.push(file_name);
        }
    }
    ensure!(
        !file_names.is_empty(),
        "{}: has no frames",
        folder.display()
    );
    file_names.sort();
    for (index, file_name) in file_names.iter().enumerate() {
        let expected = frame_file_name(index);
        ensure!(
            *file_name == expected,
            "{}: found {file_name} where {expected} should be. Frames are named 00.png, 01.png & so on, with no gaps",
            folder.display()
        );
        check_frame(&folder.join(file_name), canvas)?;
    }
    Ok(file_names.len())
}

/// The file name of a frame, such as `03.png`.
fn frame_file_name(index: usize) -> String {
    format!("{index:02}.png")
}

/// Checks one frame: 8 bit RGBA, the canvas size & nothing drawn below the floor.
fn check_frame(path: &Path, canvas: &CanvasSettings) -> Result<()> {
    let file = File::open(path).with_context(|| format!("could not open {}", path.display()))?;
    let mut reader = png::Decoder::new(BufReader::new(file))
        .read_info()
        .with_context(|| format!("{} is not a PNG", path.display()))?;
    let info = reader.info();
    ensure!(
        info.color_type == png::ColorType::Rgba && info.bit_depth == png::BitDepth::Eight,
        "{}: must be 8 bit RGBA, with a see through background",
        path.display()
    );
    ensure!(
        info.width == canvas.width && info.height == canvas.height,
        "{}: is {} by {} but the canvas is {} by {}",
        path.display(),
        info.width,
        info.height,
        canvas.width,
        canvas.height
    );
    let buffer_size = reader
        .output_buffer_size()
        .with_context(|| format!("{} is too big", path.display()))?;
    let mut pixels = vec![0; buffer_size];
    let frame = reader
        .next_frame(&mut pixels)
        .with_context(|| format!("could not read the pixels of {}", path.display()))?;
    // Rows below the floor must be fully see through, so the cat never sinks into the bar.
    let below_floor = (canvas.floor as usize + 1) * frame.line_size;
    let drawn_below_floor = pixels
        .get(below_floor..frame.buffer_size())
        .with_context(|| format!("{}: has fewer rows than the canvas", path.display()))?
        .as_chunks::<BYTES_PER_PIXEL>()
        .0
        .iter()
        .any(|pixel| pixel[ALPHA_INDEX] != 0);
    ensure!(
        !drawn_below_floor,
        "{}: something is drawn below the floor row {}",
        path.display(),
        canvas.floor
    );
    Ok(())
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
fn write_rust(canvas: &CanvasSettings, animations: &[FoundAnimation]) -> Result<String> {
    let mut rust = String::new();
    writeln!(
        rust,
        "pub const CANVAS: Canvas = Canvas {{ width: {}, height: {}, floor: {} }};",
        canvas.width, canvas.height, canvas.floor
    )?;
    writeln!(rust, "pub const ANIMATIONS: &[Animation] = &[")?;
    for FoundAnimation {
        name,
        settings,
        frame_count,
    } in animations
    {
        let frame_ms_each = match &settings.frame_ms_each {
            Some(times) => format!("Some(&{times:?})"),
            None => "None".to_owned(),
        };
        writeln!(
            rust,
            "    Animation {{ name: {name:?}, frame_count: {frame_count}, frame_ms: {}, loops: {}, step: {}, recolor: {}, frame_ms_each: {frame_ms_each} }},",
            settings.frame_ms, settings.loops, settings.step, settings.recolor
        )?;
    }
    writeln!(rust, "];")?;
    Ok(rust)
}

/// Writes every frame as `include_bytes!`, so the PNGs are packed into the program.
fn write_packed(folder: &Path, animations: &[FoundAnimation]) -> Result<String> {
    let mut rust = String::new();
    writeln!(rust, "const PACKED: &[PackedAnimation] = &[")?;
    for animation in animations {
        writeln!(
            rust,
            "    PackedAnimation {{ name: {:?}, pngs: &[",
            animation.name
        )?;
        for index in 0..animation.frame_count {
            let path = folder.join(&animation.name).join(frame_file_name(index));
            let path = path
                .to_str()
                .with_context(|| format!("{}: path is not plain text", path.display()))?;
            writeln!(rust, "        include_bytes!({path:?}),")?;
        }
        writeln!(rust, "    ] }},")?;
    }
    writeln!(rust, "];")?;
    Ok(rust)
}
