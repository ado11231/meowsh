//! A plain stand in for the cat: a white oval, made in code, used until real frames exist.

use super::still::Still;

/// The placeholder's width in pixels.
pub const PLACEHOLDER_WIDTH: u32 = 24;

/// The placeholder's height in pixels.
pub const PLACEHOLDER_HEIGHT: u32 = 16;

/// The color inside the oval: white & fully solid, in RGBA order.
const INSIDE: [u8; 4] = [255, 255, 255, 255];

/// The color outside the oval: fully see through.
const OUTSIDE: [u8; 4] = [0, 0, 0, 0];

/// The placeholder picture: a white oval that fills its box, on a see through background.
pub fn placeholder() -> Still {
    let mut rgba = Vec::with_capacity(PLACEHOLDER_WIDTH as usize * PLACEHOLDER_HEIGHT as usize * 4);
    for y in 0..PLACEHOLDER_HEIGHT {
        for x in 0..PLACEHOLDER_WIDTH {
            let color = if inside_oval(x, y) { INSIDE } else { OUTSIDE };
            rgba.extend_from_slice(&color);
        }
    }
    Still {
        width: PLACEHOLDER_WIDTH,
        height: PLACEHOLDER_HEIGHT,
        rgba,
    }
}

/// Whether the middle of pixel `x`, `y` is inside the oval that fills the picture.
fn inside_oval(x: u32, y: u32) -> bool {
    // Doubled, so the middle of a pixel & the middle of the picture are whole numbers.
    let width = i64::from(PLACEHOLDER_WIDTH);
    let height = i64::from(PLACEHOLDER_HEIGHT);
    let from_middle_x = 2 * i64::from(x) + 1 - width;
    let from_middle_y = 2 * i64::from(y) + 1 - height;
    // (dx / w)^2 + (dy / h)^2 <= 1, with both sides multiplied by (w * h)^2.
    from_middle_x.pow(2) * height.pow(2) + from_middle_y.pow(2) * width.pow(2)
        <= width.pow(2) * height.pow(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Result, ensure};

    /// The color of pixel `x`, `y`.
    fn color_at(still: &Still, x: u32, y: u32) -> Result<[u8; 4]> {
        let start = usize::try_from((y * still.width + x) * 4)?;
        let pixel = still
            .rgba
            .get(start..start + 4)
            .ok_or_else(|| anyhow::anyhow!("pixel {x}, {y} is outside the picture"))?;
        Ok(<[u8; 4]>::try_from(pixel)?)
    }

    #[test]
    fn the_placeholder_has_one_color_for_each_pixel() -> Result<()> {
        let still = placeholder();
        let pixel_count = usize::try_from(still.width * still.height)?;

        ensure!(still.rgba.len() == pixel_count * 4);
        Ok(())
    }

    #[test]
    fn the_middle_is_white_and_the_corners_see_through() -> Result<()> {
        let still = placeholder();
        let last_x = still.width - 1;
        let last_y = still.height - 1;

        assert_eq!(color_at(&still, still.width / 2, still.height / 2)?, INSIDE);
        for (x, y) in [(0, 0), (last_x, 0), (0, last_y), (last_x, last_y)] {
            assert_eq!(color_at(&still, x, y)?, OUTSIDE, "at {x}, {y}");
        }
        Ok(())
    }

    #[test]
    fn the_oval_reaches_each_side() -> Result<()> {
        let still = placeholder();

        assert_eq!(color_at(&still, 0, still.height / 2)?, INSIDE);
        assert_eq!(color_at(&still, still.width - 1, still.height / 2)?, INSIDE);
        assert_eq!(color_at(&still, still.width / 2, 0)?, INSIDE);
        assert_eq!(color_at(&still, still.width / 2, still.height - 1)?, INSIDE);
        Ok(())
    }
}
