//! Turns the cat's scene into pictures the painter can draw, by looking up each frame.

use crate::anim::ANIMATIONS;
use crate::art::library::Library;
use crate::scene::{Facing, Scene, ScenePicture};
use crate::term::painter::BarPicture;
use crate::term::picture::Pixels;
use anyhow::{Context, Result};

/// The pictures for a scene, in the same order.
pub fn bar_pictures<'a>(scene: &Scene, library: &'a Library) -> Result<Vec<BarPicture<'a>>> {
    scene
        .pictures
        .iter()
        .map(|picture| bar_picture(picture, library))
        .collect()
}

fn bar_picture<'a>(picture: &ScenePicture, library: &'a Library) -> Result<BarPicture<'a>> {
    let missing = || format!("{} has no frame {}", picture.animation, picture.frame);
    let animation_index = ANIMATIONS
        .iter()
        .position(|animation| animation.name == picture.animation)
        .with_context(missing)?;
    let still = library
        .frame(picture.animation, picture.frame, picture.facing)
        .with_context(missing)?;
    Ok(BarPicture {
        content: content_number(animation_index, picture.frame, picture.facing),
        pixels: Pixels {
            width: still.width,
            height: still.height,
            rgba: &still.rgba,
        },
        x: picture.x,
    })
}

/// A number that is different for every animation, frame & facing, so the painter can tell
/// when a picture changed.
fn content_number(animation_index: usize, frame: usize, facing: Facing) -> u64 {
    let facing_bit = match facing {
        Facing::Right => 0,
        Facing::Left => 1,
    };
    // An animation has far fewer than 2^31 frames, so the parts never overlap.
    (animation_index as u64) << 32 | (frame as u64) << 1 | facing_bit
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::flip::flip;
    use anyhow::ensure;
    use std::collections::HashSet;

    #[test]
    fn each_picture_gets_its_frame_and_place_in_order() -> Result<()> {
        let library = Library::load(None)?;
        let scene = Scene {
            pictures: vec![
                ScenePicture {
                    animation: "sit",
                    frame: 1,
                    x: 40,
                    facing: Facing::Right,
                },
                ScenePicture {
                    animation: "sit",
                    frame: 1,
                    x: 7,
                    facing: Facing::Left,
                },
            ],
        };
        let right = library
            .frame("sit", 1, Facing::Right)
            .context("sit frame 1 is missing")?;

        let pictures = bar_pictures(&scene, &library)?;

        ensure!(pictures.len() == 2);
        ensure!(pictures[0].x == 40 && pictures[1].x == 7);
        ensure!(pictures[0].pixels.rgba == right.rgba.as_slice());
        ensure!(pictures[1].pixels.rgba == flip(right).rgba.as_slice());
        ensure!(pictures[0].content != pictures[1].content);
        Ok(())
    }

    #[test]
    fn a_frame_that_does_not_exist_is_an_error() -> Result<()> {
        let library = Library::load(None)?;
        let unknown = ScenePicture {
            animation: "not_an_animation",
            frame: 0,
            x: 0,
            facing: Facing::Right,
        };
        let past_the_end = ScenePicture {
            animation: "sit",
            frame: 1000,
            ..unknown
        };

        for picture in [unknown, past_the_end] {
            let scene = Scene {
                pictures: vec![picture],
            };
            ensure!(bar_pictures(&scene, &library).is_err(), "{picture:?}");
        }
        Ok(())
    }

    #[test]
    fn content_numbers_differ_for_every_animation_frame_and_facing() -> Result<()> {
        let mut seen = HashSet::new();
        for animation_index in 0..3 {
            for frame in 0..3 {
                for facing in [Facing::Right, Facing::Left] {
                    let content = content_number(animation_index, frame, facing);
                    ensure!(seen.insert(content), "{content} was used twice");
                }
            }
        }
        Ok(())
    }
}
