//! Makes the left facing copy of a frame. Frames are drawn facing right.

use super::still::Still;

/// Bytes per pixel: red, green, blue & how see through it is.
const BYTES_PER_PIXEL: usize = 4;

/// A mirror copy of a frame, with each row's pixels in the other order.
pub fn flip(still: &Still) -> Still {
    let row_length = still.width as usize * BYTES_PER_PIXEL;
    let mut rgba = Vec::with_capacity(still.rgba.len());
    // `chunks` cannot take 0. A picture 0 wide has no pixels, so 1 changes nothing.
    for row in still.rgba.chunks(row_length.max(1)) {
        let (pixels, _) = row.as_chunks::<BYTES_PER_PIXEL>();
        for pixel in pixels.iter().rev() {
            rgba.extend_from_slice(pixel);
        }
    }
    Still {
        width: still.width,
        height: still.height,
        rgba,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_row_is_mirrored_and_rows_stay_in_place() {
        // 3 wide & 2 tall, every pixel different, so a pixel moved to the wrong row or
        // place would show.
        let still = Still {
            width: 3,
            height: 2,
            rgba: vec![
                1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, //
                4, 4, 4, 4, 5, 5, 5, 5, 6, 6, 6, 6, //
            ],
        };

        let flipped = flip(&still);

        assert_eq!(
            flipped,
            Still {
                width: 3,
                height: 2,
                rgba: vec![
                    3, 3, 3, 3, 2, 2, 2, 2, 1, 1, 1, 1, //
                    6, 6, 6, 6, 5, 5, 5, 5, 4, 4, 4, 4, //
                ],
            }
        );
    }
}
