//! Changes the colors a frame was drawn in to the terminal's own, so the cat shows on any
//! theme. White becomes the text color, black becomes the background color, & greys land in
//! between. On a light theme that gives a dark cat with light eyes.

use super::still::Still;

/// Bytes per pixel: red, green, blue & how see through it is.
const BYTES_PER_PIXEL: usize = 4;

/// The largest value of one color byte.
const FULL: u32 = 255;

/// The terminal's colors, as red, green & blue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colors {
    /// What white turns into.
    pub text: [u8; 3],
    /// What black turns into.
    pub background: [u8; 3],
}

/// Recolors every pixel of a frame. How see through each pixel is stays the same.
pub fn recolor(still: &mut Still, colors: Colors) {
    let (pixels, _) = still.rgba.as_chunks_mut::<BYTES_PER_PIXEL>();
    for pixel in pixels {
        // How light the pixel was drawn, from 0 (black) to 255 (white).
        let shade = (u32::from(pixel[0]) + u32::from(pixel[1]) + u32::from(pixel[2])) / 3;
        // Zipping with 3 colors leaves the 4th byte, how see through it is, alone.
        let ends = colors.background.into_iter().zip(colors.text);
        for (value, (dark, light)) in pixel.iter_mut().zip(ends) {
            *value = mix(dark, light, shade);
        }
    }
}

/// Goes from `dark` at shade 0 to `light` at shade 255, rounded to the nearest whole number.
fn mix(dark: u8, light: u8, shade: u32) -> u8 {
    let mixed = (u32::from(dark) * (FULL - shade) + u32::from(light) * shade + FULL / 2) / FULL;
    // A mix of two bytes is never more than 255.
    u8::try_from(mixed).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_becomes_text_black_becomes_background_and_greys_land_between() {
        let colors = Colors {
            text: [250, 0, 100],
            background: [0, 250, 100],
        };
        let mut still = Still {
            width: 4,
            height: 1,
            rgba: vec![
                255, 255, 255, 255, // white
                0, 0, 0, 255, // black
                51, 51, 51, 128, // dark grey, half see through
                255, 255, 255, 0, // fully see through
            ],
        };

        recolor(&mut still, colors);

        // The grey is 51 of 255, a fifth of the way from the background to the text color.
        assert_eq!(
            still.rgba,
            vec![
                250, 0, 100, 255, //
                0, 250, 100, 255, //
                50, 200, 100, 128, //
                250, 0, 100, 0, //
            ]
        );
    }
}
