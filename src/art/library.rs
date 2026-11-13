//! Every frame of every animation, read from the PNGs packed into the program by `build.rs`.
//! Loaded once at startup, then looked up by animation name & frame number.

use super::flip::flip;
use super::recolor::{Colors, recolor};
use super::still::Still;
use crate::anim;
use crate::scene::Facing;
use anyhow::{Context, Result, ensure};
use std::collections::HashMap;
use std::io::Cursor;

/// One animation's frames as PNG files, in play order.
struct PackedAnimation {
    /// The folder name, such as `sit`.
    name: &'static str,
    /// The bytes of `00.png`, `01.png` & so on.
    pngs: &'static [&'static [u8]],
}

include!(concat!(env!("OUT_DIR"), "/packed_frames.rs"));

/// One frame, facing each way.
#[derive(Debug)]
struct Frame {
    right: Still,
    left: Still,
}

/// Every frame, ready to show.
#[derive(Debug)]
pub struct Library {
    /// Each animation's frames, in play order, by name.
    animations: HashMap<&'static str, Vec<Frame>>,
}

impl Library {
    /// Reads every packed frame & makes its left facing copy. With the terminal's colors, frames of animations that have
    /// `recolor` on are changed to them. Without, every frame keeps the colors it was drawn in.
    pub fn load(colors: Option<Colors>) -> Result<Self> {
        let mut animations = HashMap::new();
        for packed in PACKED {
            let settings = anim::find(packed.name)
                .with_context(|| format!("{} has frames but no settings", packed.name))?;
            let mut frames = packed
                .pngs
                .iter()
                .enumerate()
                .map(|(index, png)| {
                    read_png(png)
                        .with_context(|| format!("could not read frame {index} of {}", packed.name))
                })
                .collect::<Result<Vec<Still>>>()?;
            if let Some(colors) = colors
                && settings.recolor
            {
                for frame in &mut frames {
                    recolor(frame, colors);
                }
            }
            let frames = frames
                .into_iter()
                .map(|right| Frame {
                    left: flip(&right),
                    right,
                })
                .collect();
            animations.insert(packed.name, frames);
        }
        Ok(Self { animations })
    }

    /// One frame of one animation facing one way, or `None` if there is no such animation
    /// or frame.
    pub fn frame(&self, name: &str, index: usize, facing: Facing) -> Option<&Still> {
        let frame = self.animations.get(name)?.get(index)?;
        Some(match facing {
            Facing::Right => &frame.right,
            Facing::Left => &frame.left,
        })
    }
}

/// Reads one PNG into its pixels. The build only lets in 8 bit RGBA, so anything else is
/// refused instead of converted.
fn read_png(png: &[u8]) -> Result<Still> {
    let mut reader = png::Decoder::new(Cursor::new(png))
        .read_info()
        .context("not a PNG")?;
    let info = reader.info();
    ensure!(
        info.color_type == png::ColorType::Rgba && info.bit_depth == png::BitDepth::Eight,
        "not 8 bit RGBA"
    );
    let buffer_size = reader.output_buffer_size().context("too big")?;
    let mut rgba = vec![0; buffer_size];
    let frame = reader
        .next_frame(&mut rgba)
        .context("could not read pixels")?;
    rgba.truncate(frame.buffer_size());
    Ok(Still {
        width: frame.width,
        height: frame.height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::{ANIMATIONS, CANVAS};

    /// Writes a small PNG of the given kind, for tests.
    fn make_png(width: u32, height: u32, color: png::ColorType, pixels: &[u8]) -> Result<Vec<u8>> {
        let mut png = Vec::new();
        let mut encoder = png::Encoder::new(&mut png, width, height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(pixels)?;
        writer.finish()?;
        Ok(png)
    }

    #[test]
    fn a_png_is_read_row_by_row_from_the_top_left() -> Result<()> {
        // 3 wide & 2 tall, every byte different, so a swapped width & height or a wrong
        // order would show.
        let pixels: Vec<u8> = (0..3 * 2 * 4).collect();
        let png = make_png(3, 2, png::ColorType::Rgba, &pixels)?;

        let still = read_png(&png)?;

        assert_eq!(
            still,
            Still {
                width: 3,
                height: 2,
                rgba: pixels
            }
        );
        Ok(())
    }

    #[test]
    fn a_png_with_no_see_through_layer_is_refused() -> Result<()> {
        let png = make_png(1, 1, png::ColorType::Rgb, &[255, 255, 255])?;

        ensure!(read_png(&png).is_err());
        Ok(())
    }

    #[test]
    fn every_animation_loads_all_its_frames_at_the_canvas_size_facing_each_way() -> Result<()> {
        let library = Library::load(None)?;

        for animation in ANIMATIONS {
            for index in 0..animation.frame_count {
                let right = library
                    .frame(animation.name, index, Facing::Right)
                    .with_context(|| format!("{} frame {index} is missing", animation.name))?;
                ensure!(
                    right.width == CANVAS.width && right.height == CANVAS.height,
                    "{} frame {index} is {} by {}",
                    animation.name,
                    right.width,
                    right.height
                );
                ensure!(
                    library.frame(animation.name, index, Facing::Left) == Some(&flip(right)),
                    "{} frame {index} facing left is not the mirror of facing right",
                    animation.name
                );
            }
            ensure!(
                library
                    .frame(animation.name, animation.frame_count, Facing::Right)
                    .is_none(),
                "{} has a frame past its last one",
                animation.name
            );
        }
        Ok(())
    }

    #[test]
    fn colors_change_the_frames_of_animations_that_ask_for_it() -> Result<()> {
        let colors = Colors {
            text: [250, 0, 100],
            background: [0, 250, 100],
        };
        let as_drawn = Library::load(None)?;
        let colored = Library::load(Some(colors))?;

        for animation in ANIMATIONS {
            for index in 0..animation.frame_count {
                let mut expected = as_drawn
                    .frame(animation.name, index, Facing::Right)
                    .with_context(|| format!("{} frame {index} is missing", animation.name))?
                    .clone();
                if animation.recolor {
                    recolor(&mut expected, colors);
                }
                ensure!(
                    colored.frame(animation.name, index, Facing::Right) == Some(&expected),
                    "{} frame {index} has the wrong colors",
                    animation.name
                );
            }
        }
        Ok(())
    }

    #[test]
    fn an_unknown_animation_has_no_frames() -> Result<()> {
        let library = Library::load(None)?;

        ensure!(
            library
                .frame("not_an_animation", 0, Facing::Right)
                .is_none()
        );
        Ok(())
    }
}
