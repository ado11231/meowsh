//! The kitty picture codes that show & remove pictures. They only build the codes. Where
//! & when to send them is the painter's job.
//!
//! A picture is sent as base64 inside one or more codes: `ESC _ G settings ; data ESC \`.
//! Every code tells the terminal not to answer, because an answer would reach the shell as
//! if you typed it.

use super::escape::{PICTURE_START, STRING_END};
use anyhow::{Context, Result, ensure};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

/// The most base64 bytes one code may carry. Longer pictures are split over several codes.
/// It is a multiple of 4, so every piece is whole base64.
const MAX_CODE_DATA: usize = 4096;

/// Bytes in one pixel: red, green, blue & how see through it is.
const BYTES_PER_PIXEL: u64 = 4;

/// Shows a picture right away, sending it & placing it in one go.
const SEND_AND_SHOW: &str = "a=T";

/// Removes a picture by its number & frees what the terminal kept of it.
const REMOVE_BY_ID: &str = "a=d,d=I";

/// The pixels are 8 bit RGBA.
const RGBA_FORMAT: &str = "f=32";

/// The terminal never answers, not even with errors.
const NO_ANSWER: &str = "q=2";

/// The cursor stays where it is after the picture is placed.
const KEEP_CURSOR: &str = "C=1";

/// Says more pieces of the same picture follow.
const MORE_FOLLOWS: &str = "m=1";

/// Says this is the last piece of a picture.
const LAST_PIECE: &str = "m=0";

/// A picture's pixels, ready to send.
#[derive(Clone, Copy, Debug)]
pub struct Pixels<'a> {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height` pixels in RGBA order, row by row from the top left.
    pub rgba: &'a [u8],
}

/// Where & how big a picture is shown, starting at the cell the cursor is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    /// How many columns the picture is stretched over.
    pub columns: u16,
    /// How many rows the picture is stretched over.
    pub rows: u16,
    /// How many pixels right of the cell's left edge the picture starts. Smaller than one
    /// cell, so the picture can move in steps smaller than a character.
    pub offset_x: u16,
}

/// The codes that show `pixels` as picture number `id` at the cursor, as `placement` says.
pub fn show_codes(id: u32, pixels: Pixels<'_>, placement: Placement) -> Result<String> {
    let expected_length = u64::from(pixels.width) * u64::from(pixels.height) * BYTES_PER_PIXEL;
    ensure!(
        u64::try_from(pixels.rgba.len()) == Ok(expected_length),
        "a {} by {} picture needs {expected_length} bytes of RGBA, not {}",
        pixels.width,
        pixels.height,
        pixels.rgba.len()
    );

    let settings = format!(
        "{SEND_AND_SHOW},{RGBA_FORMAT},s={},v={},i={id},c={},r={},X={},{KEEP_CURSOR},{NO_ANSWER}",
        pixels.width, pixels.height, placement.columns, placement.rows, placement.offset_x
    );
    let data = BASE64.encode(pixels.rgba);
    let pieces: Vec<&[u8]> = data.as_bytes().chunks(MAX_CODE_DATA).collect();
    let mut codes = String::new();
    for (index, piece) in pieces.iter().enumerate() {
        let more = if index + 1 < pieces.len() {
            MORE_FOLLOWS
        } else {
            LAST_PIECE
        };
        // Only the first piece carries the settings. The rest only say whether more follow.
        let piece_settings = if index == 0 {
            format!("{settings},{more}")
        } else {
            format!("{more},{NO_ANSWER}")
        };
        codes.push_str(PICTURE_START);
        codes.push_str(&piece_settings);
        codes.push(';');
        // Base64 is plain ASCII, so splitting its bytes never cuts a character.
        codes.push_str(std::str::from_utf8(piece).context("base64 should be ASCII")?);
        codes.push_str(STRING_END);
    }
    Ok(codes)
}

/// The code that removes picture number `id` from the screen & from the terminal's memory.
pub fn remove_code(id: u32) -> String {
    format!("{PICTURE_START}{REMOVE_BY_ID},i={id},{NO_ANSWER}{STRING_END}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLACEMENT: Placement = Placement {
        columns: 6,
        rows: 2,
        offset_x: 3,
    };

    /// Splits codes back into each code's settings & data.
    fn pieces(codes: &str) -> Vec<(&str, &str)> {
        codes
            .split(STRING_END)
            .filter(|code| !code.is_empty())
            .filter_map(|code| code.strip_prefix(PICTURE_START))
            .filter_map(|code| code.split_once(';'))
            .collect()
    }

    #[test]
    fn a_small_picture_is_sent_in_one_code() -> Result<()> {
        // One red pixel & one see through pixel.
        let rgba = [255, 0, 0, 255, 0, 0, 0, 0];
        let pixels = Pixels {
            width: 2,
            height: 1,
            rgba: &rgba,
        };

        let codes = show_codes(9, pixels, PLACEMENT)?;

        let expected = format!(
            "{PICTURE_START}a=T,f=32,s=2,v=1,i=9,c=6,r=2,X=3,C=1,q=2,m=0;{}{STRING_END}",
            BASE64.encode(rgba)
        );
        assert_eq!(codes, expected);
        Ok(())
    }

    #[test]
    fn a_big_picture_is_split_into_pieces_that_join_back_up() -> Result<()> {
        // Enough pixels for three pieces.
        let width = 2000;
        let rgba: Vec<u8> = (0..width * 4).map(|index| (index % 251) as u8).collect();
        assert!(BASE64.encode(&rgba).len() > 2 * MAX_CODE_DATA);
        let pixels = Pixels {
            width,
            height: 1,
            rgba: &rgba,
        };

        let codes = show_codes(9, pixels, PLACEMENT)?;
        let pieces = pieces(&codes);

        assert_eq!(pieces.len(), 3);
        assert!(pieces[0].0.starts_with("a=T,"));
        assert!(pieces[0].0.ends_with(MORE_FOLLOWS));
        assert_eq!(pieces[1].0, format!("{MORE_FOLLOWS},{NO_ANSWER}"));
        assert_eq!(pieces[2].0, format!("{LAST_PIECE},{NO_ANSWER}"));
        assert!(pieces.iter().all(|(_, data)| data.len() <= MAX_CODE_DATA));
        let data: String = pieces.iter().map(|(_, data)| *data).collect();
        assert_eq!(BASE64.decode(data)?, rgba);
        Ok(())
    }

    #[test]
    fn pixels_that_do_not_match_the_size_are_an_error() {
        let pixels = Pixels {
            width: 2,
            height: 2,
            rgba: &[0; 4],
        };

        assert!(show_codes(9, pixels, PLACEMENT).is_err());
    }

    #[test]
    fn removing_names_the_picture_and_asks_for_no_answer() {
        assert_eq!(
            remove_code(9),
            format!("{PICTURE_START}a=d,d=I,i=9,q=2{STRING_END}")
        );
    }
}
