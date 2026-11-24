//! The painter: draws pictures in the bar, only when something changed.
//!
//! It is handed a list of pictures & knows nothing about what they are. Each picture says
//! what it shows with a `content` number, so the painter can tell a change from the same
//! picture handed in again.

use super::bar::{Layout, push_erase_bar};
use super::escape::{
    CSI, CURSOR_POSITION, HOLD_SCREEN, RELEASE_SCREEN, RESTORE_CURSOR, SAVE_CURSOR,
};
use super::picture::{
    Pixels, Placement, place_code, remove_code, remove_placement_code, send_codes, show_codes,
};
use crate::env::VSCODE_TERM_PROGRAM;
use anyhow::{Result, ensure};
use portable_pty::PtySize;
use std::collections::HashMap;
use std::ffi::OsStr;

/// The cell width used when the terminal does not say how big its cells are.
const DEFAULT_CELL_WIDTH: u16 = 8;

/// The cell height used when the terminal does not say how big its cells are.
const DEFAULT_CELL_HEIGHT: u16 = 16;

/// The most pictures the bar shows at once.
pub const MAX_PICTURES: usize = 16;

/// How many picture numbers each of the two sets used for drawing in the cells holds. At
/// least `MAX_PICTURES`.
const PICTURE_ID_SET_SIZE: u32 = 16;

/// The first picture number the painter uses. Numbers below it are left for other uses,
/// such as the picture check.
const FIRST_PICTURE_ID: u32 = 7710;

/// How the terminal keeps pictures, which decides how they are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawingWay {
    /// Pictures float above the text, as in kitty. Each picture is sent once, while the
    /// cursor is still where the shell left it, & the terminal keeps it. After that it is
    /// only placed in the bar, which is a short code, & the old one is taken off.
    /// The bar's cells are never erased & the screen is never held, so the cursor only sits
    /// in the bar for the short placing code, & no cursor box flashes there.
    Layered,
    /// Pictures are burned into the cells, as in VS Code, so removing one leaves it on
    /// screen. The screen is held while the bar is erased & drawn again.
    InCells,
}

impl DrawingWay {
    /// The way for the terminal named in `TERM_PROGRAM`.
    pub fn for_terminal(term_program: Option<&OsStr>) -> Self {
        if term_program == Some(OsStr::new(VSCODE_TERM_PROGRAM)) {
            Self::InCells
        } else {
            Self::Layered
        }
    }
}

/// Where the bar is on screen & how big one cell is in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarArea {
    /// The bar's top row, counted from 1.
    pub first_row: u16,
    /// The bar's bottom row, counted from 1.
    pub last_row: u16,
    /// The window's width in cells.
    pub columns: u16,
    /// One cell's width in pixels.
    pub cell_width: u16,
    /// One cell's height in pixels.
    pub cell_height: u16,
}

impl BarArea {
    /// The bar for `layout` in a window of `window` size, or `None` while the bar is off.
    pub fn new(layout: Layout, window: PtySize) -> Option<Self> {
        let first_row = layout.first_bar_row()?;
        let cell_width = cell_size(window.pixel_width, window.cols, DEFAULT_CELL_WIDTH);
        let cell_height = cell_size(window.pixel_height, window.rows, DEFAULT_CELL_HEIGHT);
        Some(Self {
            first_row,
            last_row: layout.rows,
            columns: window.cols,
            cell_width,
            cell_height,
        })
    }

    /// The bar's width, in the pixels of a picture `picture_height` pixels tall. Pictures
    /// are scaled to the bar's height, so this is how many of their pixels fit across it.
    pub fn width_in_picture_pixels(self, picture_height: u32) -> u32 {
        let bar_width = u64::from(self.columns) * u64::from(self.cell_width);
        let bar_height = u64::from(self.rows()) * u64::from(self.cell_height);
        let width = (bar_width * u64::from(picture_height))
            .checked_div(bar_height)
            .unwrap_or(0);
        // A window is far less than 4 billion pixels wide, so this always fits.
        u32::try_from(width).unwrap_or(u32::MAX)
    }

    fn rows(self) -> u16 {
        self.last_row - self.first_row + 1
    }

    /// Where a picture goes: the column it starts in, counted from 0, & how it is placed.
    ///
    /// The picture is scaled to the bar's height. `x` is in the picture's own pixels, from
    /// the left end of the bar, so it scales the same way.
    fn place(self, pixels: Pixels<'_>, x: u32) -> Result<(u16, Placement)> {
        ensure!(
            pixels.height > 0,
            "a picture needs at least one row of pixels"
        );
        let bar_height = u64::from(self.rows()) * u64::from(self.cell_height);
        let picture_height = u64::from(pixels.height);
        let cell_width = u64::from(self.cell_width);

        let screen_width = u64::from(pixels.width) * bar_height / picture_height;
        // Round to the nearest column, so the picture is squeezed or stretched the least.
        let columns = ((screen_width + cell_width / 2) / cell_width).max(1);
        let screen_x = u64::from(x) * bar_height / picture_height;
        let last_column = u64::from(self.columns.saturating_sub(1));
        let column = (screen_x / cell_width).min(last_column);
        let offset_x = screen_x % cell_width;

        // Each is at most the window's columns or one cell's width, so each fits.
        let placement = Placement {
            columns: u16::try_from(columns).unwrap_or(u16::MAX),
            rows: self.rows(),
            offset_x: u16::try_from(offset_x).unwrap_or(0),
        };
        Ok((u16::try_from(column).unwrap_or(0), placement))
    }
}

/// One cell's size along one side, from the window's size in pixels & in cells.
fn cell_size(pixels: u16, cells: u16, default: u16) -> u16 {
    match pixels.checked_div(cells) {
        Some(size) if size > 0 => size,
        // The terminal did not say how big it is in pixels.
        _ => default,
    }
}

/// One picture to show in the bar.
#[derive(Clone, Copy, Debug)]
pub struct BarPicture<'a> {
    /// Says what the picture shows. The same number must always mean the same pixels.
    pub content: u64,
    /// The picture itself.
    pub pixels: Pixels<'a>,
    /// How far from the left end of the bar it starts, in the picture's own pixels.
    pub x: u32,
}

/// What the painter last drew for one picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Drawn {
    content: u64,
    x: u32,
}

/// Draws pictures in the bar & remembers what it drew.
pub struct Painter {
    way: DrawingWay,
    /// The bar the last drawing was for.
    area: Option<BarArea>,
    drawn: Vec<Drawn>,
    /// The number of the picture on screen in each place, in the order they were handed in.
    drawn_ids: Vec<u32>,
    /// Drawing in the cells only: which of two sets of numbers the next drawing uses. They
    /// take turns, so the new pictures never share a number with the old ones they replace.
    second_set: bool,
    /// Layered drawing only: the number the terminal keeps each picture under, by its
    /// `content` number, so each picture is sent once & after that only placed.
    sent: HashMap<u64, u32>,
    /// Whether the bar may have been wiped since the last drawing.
    wiped: bool,
}

impl Painter {
    /// A painter that has drawn nothing yet.
    pub fn new(way: DrawingWay) -> Self {
        Self {
            way,
            area: None,
            drawn: Vec::new(),
            drawn_ids: Vec::new(),
            second_set: false,
            sent: HashMap::new(),
            wiped: false,
        }
    }

    /// Notes that the bar may have been wiped, so the next drawing happens even if nothing
    /// else changed.
    pub fn bar_wiped(&mut self) {
        self.wiped = true;
    }

    /// Adds the codes that draw `pictures` in `area` to `output`. Adds nothing if the same
    /// pictures are already there. While `area` is `None` the bar is off, so the old
    /// pictures are removed & nothing is drawn.
    ///
    /// Saving the cursor here replaces a save the shell made itself. Programs rarely save
    /// it for long, and the painter waits for a pause in their output before drawing.
    pub fn paint(
        &mut self,
        pictures: &[BarPicture<'_>],
        area: Option<BarArea>,
        output: &mut String,
    ) -> Result<()> {
        ensure!(
            pictures.len() <= MAX_PICTURES,
            "the bar shows at most {MAX_PICTURES} pictures, not {}",
            pictures.len()
        );
        let wanted: Vec<Drawn> = pictures
            .iter()
            .map(|picture| Drawn {
                content: picture.content,
                x: picture.x,
            })
            .collect();
        if !self.wiped && area == self.area && wanted == self.drawn {
            return Ok(());
        }

        match area {
            // The rows belong to the shell now, so only the pictures are removed.
            None => self.push_remove_old(output),
            Some(bar) => match self.way {
                DrawingWay::Layered => self.paint_layered(pictures, bar, output)?,
                DrawingWay::InCells => self.paint_in_cells(pictures, bar, output)?,
            },
        }
        self.area = area;
        self.drawn = if area.is_some() { wanted } else { Vec::new() };
        self.wiped = false;
        Ok(())
    }

    /// Sends the pictures the terminal does not have yet while the cursor is still where
    /// the shell left it, then places them all in the bar.
    fn paint_layered(
        &mut self,
        pictures: &[BarPicture<'_>],
        bar: BarArea,
        output: &mut String,
    ) -> Result<()> {
        if self.wiped {
            // A wipe can be a reset or a switch to the other screen, which can lose the
            // pictures the terminal kept. They are freed & sent again when next shown.
            for (_, id) in self.sent.drain() {
                output.push_str(&remove_code(id));
            }
            self.drawn_ids.clear();
        }

        // Built in full first, so an error leaves the painter as it was.
        let mut newly_sent: HashMap<u64, u32> = HashMap::new();
        let mut sending = String::new();
        let mut placing = String::new();
        let mut new_ids = Vec::with_capacity(pictures.len());
        for (index, picture) in pictures.iter().enumerate() {
            let known = self.sent.get(&picture.content);
            let id = match known.or_else(|| newly_sent.get(&picture.content)) {
                Some(&id) => id,
                None => {
                    let id = FIRST_PICTURE_ID + u32::try_from(self.sent.len() + newly_sent.len())?;
                    sending.push_str(&send_codes(id, picture.pixels)?);
                    newly_sent.insert(picture.content, id);
                    id
                }
            };
            let (column, placement) = bar.place(picture.pixels, picture.x)?;
            push_move_to(&mut placing, bar, column);
            placing.push_str(&place_code(id, placement_id(index), placement));
            new_ids.push(id);
        }

        output.push_str(&sending);
        output.push_str(SAVE_CURSOR);
        output.push_str(&placing);
        // A place that shows the same picture was moved by placing it again, so only places
        // that now show another picture, or none, are removed.
        for (index, &old_id) in self.drawn_ids.iter().enumerate() {
            if new_ids.get(index) != Some(&old_id) {
                output.push_str(&remove_placement_code(old_id, placement_id(index)));
            }
        }
        output.push_str(RESTORE_CURSOR);

        self.sent.extend(newly_sent);
        self.drawn_ids = new_ids;
        Ok(())
    }

    /// Holds the screen, erases the bar & sends every picture again, because pictures in
    /// the cells cannot be moved.
    fn paint_in_cells(
        &mut self,
        pictures: &[BarPicture<'_>],
        bar: BarArea,
        output: &mut String,
    ) -> Result<()> {
        let set_start = FIRST_PICTURE_ID
            + if self.second_set {
                PICTURE_ID_SET_SIZE
            } else {
                0
            };
        // Built in full first, so an error leaves the painter as it was.
        let mut codes = String::new();
        codes.push_str(HOLD_SCREEN);
        codes.push_str(SAVE_CURSOR);
        push_erase_bar(&mut codes, bar.first_row, bar.last_row);
        let mut new_ids = Vec::with_capacity(pictures.len());
        for (index, picture) in (0u32..).zip(pictures) {
            let id = set_start + index;
            let (column, placement) = bar.place(picture.pixels, picture.x)?;
            push_move_to(&mut codes, bar, column);
            codes.push_str(&show_codes(id, picture.pixels, placement)?);
            new_ids.push(id);
        }
        output.push_str(&codes);
        self.push_remove_old(output);
        output.push_str(RESTORE_CURSOR);
        output.push_str(RELEASE_SCREEN);

        self.drawn_ids = new_ids;
        self.second_set = !self.second_set;
        Ok(())
    }

    /// Takes the pictures drawn last off the screen. Layered pictures stay kept by the
    /// terminal, so they can be placed again without sending them.
    fn push_remove_old(&mut self, output: &mut String) {
        for (index, id) in self.drawn_ids.drain(..).enumerate() {
            let code = match self.way {
                DrawingWay::Layered => remove_placement_code(id, placement_id(index)),
                DrawingWay::InCells => remove_code(id),
            };
            output.push_str(&code);
        }
    }
}

/// The placement number for the picture at `index` in the list. Placement numbers start
/// at 1, because 0 means none.
fn placement_id(index: usize) -> u32 {
    // There are at most `MAX_PICTURES` pictures, so this always fits.
    u32::try_from(index + 1).unwrap_or(u32::MAX)
}

/// Adds the code that moves the cursor to `column` in the bar's top row.
fn push_move_to(codes: &mut String, bar: BarArea, column: u16) {
    codes.push_str(&format!(
        "{CSI}{};{}{CURSOR_POSITION}",
        bar.first_row,
        column + 1
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::bar::BAR_ROWS;

    /// A 2 by 1 picture: one white pixel & one see through pixel.
    const TWO_PIXELS: [u8; 8] = [255, 255, 255, 255, 0, 0, 0, 0];

    /// A bar in rows 23 & 24 of an 80 column window, with 10 by 20 pixel cells.
    const AREA: BarArea = BarArea {
        first_row: 23,
        last_row: 24,
        columns: 80,
        cell_width: 10,
        cell_height: 20,
    };

    fn picture(content: u64, x: u32) -> BarPicture<'static> {
        BarPicture {
            content,
            pixels: Pixels {
                width: 2,
                height: 1,
                rgba: &TWO_PIXELS,
            },
            x,
        }
    }

    fn paint(
        painter: &mut Painter,
        pictures: &[BarPicture<'_>],
        area: Option<BarArea>,
    ) -> Result<String> {
        let mut output = String::new();
        painter.paint(pictures, area, &mut output)?;
        Ok(output)
    }

    /// The codes that move to the first picture's place & place picture number `id` there,
    /// after it was sent.
    fn placed(id: u32, picture: BarPicture<'_>) -> Result<String> {
        let (column, placement) = AREA.place(picture.pixels, picture.x)?;
        Ok(format!("{CSI}23;{}{CURSOR_POSITION}", column + 1) + &place_code(id, 1, placement))
    }

    /// The codes that show `picture` as number `id` in `AREA`.
    fn shown(id: u32, picture: BarPicture<'_>) -> Result<String> {
        let (column, placement) = AREA.place(picture.pixels, picture.x)?;
        Ok(format!("{CSI}23;{}{CURSOR_POSITION}", column + 1)
            + &show_codes(id, picture.pixels, placement)?)
    }

    #[test]
    fn each_set_of_numbers_has_room_for_every_picture() -> Result<()> {
        assert!(MAX_PICTURES <= usize::try_from(PICTURE_ID_SET_SIZE)?);
        Ok(())
    }

    #[test]
    fn vs_code_draws_in_the_cells_and_other_terminals_in_layers() {
        let vscode = OsStr::new(VSCODE_TERM_PROGRAM);

        assert_eq!(DrawingWay::for_terminal(Some(vscode)), DrawingWay::InCells);
        assert_eq!(
            DrawingWay::for_terminal(Some(OsStr::new("kitty"))),
            DrawingWay::Layered
        );
        assert_eq!(DrawingWay::for_terminal(None), DrawingWay::Layered);
    }

    #[test]
    fn the_area_uses_the_cell_size_the_terminal_gives() {
        let window = PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 800,
            pixel_height: 480,
        };

        assert_eq!(BarArea::new(Layout::new(24), window), Some(AREA));
    }

    #[test]
    fn the_area_guesses_the_cell_size_when_the_terminal_does_not_say() -> Result<()> {
        let window = PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        };

        let area = BarArea::new(Layout::new(24), window)
            .ok_or_else(|| anyhow::anyhow!("a 24 row window has a bar"))?;

        assert_eq!(
            (area.cell_width, area.cell_height),
            (DEFAULT_CELL_WIDTH, DEFAULT_CELL_HEIGHT)
        );
        Ok(())
    }

    #[test]
    fn there_is_no_area_while_the_bar_is_off() {
        assert_eq!(BarArea::new(Layout::new(5), PtySize::default()), None);
    }

    #[test]
    fn a_picture_is_scaled_to_the_bar_and_placed_inside_a_cell() -> Result<()> {
        // The canvas size from ANIMATION.md.
        let rgba = vec![0; 309 * 215 * 4];
        let pixels = Pixels {
            width: 309,
            height: 215,
            rgba: &rgba,
        };

        // The bar is 40 pixels tall, so the picture is 57.5 pixels wide on screen, which
        // rounds to 6 columns. 100 picture pixels in is 18.6 screen pixels: column 1, 8
        // pixels into the cell.
        let (column, placement) = AREA.place(pixels, 100)?;

        assert_eq!(column, 1);
        assert_eq!(
            placement,
            Placement {
                columns: 6,
                rows: BAR_ROWS,
                offset_x: 8
            }
        );
        Ok(())
    }

    #[test]
    fn the_bar_width_is_given_in_the_pictures_own_pixels() {
        // The bar is 800 by 40 screen pixels. A 215 pixel tall picture is scaled down to 40,
        // so 800 screen pixels across are 4300 of its pixels.
        assert_eq!(AREA.width_in_picture_pixels(215), 4300);
    }

    #[test]
    fn a_picture_past_the_right_end_starts_in_the_last_column() -> Result<()> {
        let (column, _) = AREA.place(picture(1, 0).pixels, u32::MAX)?;

        assert_eq!(column, AREA.columns - 1);
        Ok(())
    }

    #[test]
    fn layered_drawing_sends_each_picture_once_and_then_only_places_it() -> Result<()> {
        let mut painter = Painter::new(DrawingWay::Layered);
        let first_id = FIRST_PICTURE_ID;
        let second_id = FIRST_PICTURE_ID + 1;
        let pixels = picture(1, 0).pixels;

        let first = paint(&mut painter, &[picture(1, 0)], Some(AREA))?;
        let moved = paint(&mut painter, &[picture(1, 40)], Some(AREA))?;
        let changed = paint(&mut painter, &[picture(2, 40)], Some(AREA))?;
        let back = paint(&mut painter, &[picture(1, 40)], Some(AREA))?;

        assert_eq!(
            first,
            format!(
                "{}{SAVE_CURSOR}{}{RESTORE_CURSOR}",
                send_codes(first_id, pixels)?,
                placed(first_id, picture(1, 0))?
            )
        );
        // The same picture in a new place is only placed again, which moves it.
        assert_eq!(
            moved,
            format!(
                "{SAVE_CURSOR}{}{RESTORE_CURSOR}",
                placed(first_id, picture(1, 40))?
            )
        );
        // A new picture is sent once, placed, & the old one is taken off.
        assert_eq!(
            changed,
            format!(
                "{}{SAVE_CURSOR}{}{}{RESTORE_CURSOR}",
                send_codes(second_id, pixels)?,
                placed(second_id, picture(2, 40))?,
                remove_placement_code(first_id, 1)
            )
        );
        // A picture the terminal already has is not sent again.
        assert_eq!(
            back,
            format!(
                "{SAVE_CURSOR}{}{}{RESTORE_CURSOR}",
                placed(first_id, picture(1, 40))?,
                remove_placement_code(second_id, 1)
            )
        );
        Ok(())
    }

    #[test]
    fn a_wipe_frees_the_kept_pictures_and_sends_them_again() -> Result<()> {
        let mut painter = Painter::new(DrawingWay::Layered);
        paint(&mut painter, &[picture(1, 0)], Some(AREA))?;
        paint(&mut painter, &[picture(2, 0)], Some(AREA))?;

        painter.bar_wiped();
        let output = paint(&mut painter, &[picture(2, 0)], Some(AREA))?;

        let freed = remove_code(FIRST_PICTURE_ID).len() + remove_code(FIRST_PICTURE_ID + 1).len();
        assert!(output.contains(&remove_code(FIRST_PICTURE_ID)));
        assert!(output.contains(&remove_code(FIRST_PICTURE_ID + 1)));
        assert_eq!(
            output[freed..],
            format!(
                "{}{SAVE_CURSOR}{}{RESTORE_CURSOR}",
                send_codes(FIRST_PICTURE_ID, picture(2, 0).pixels)?,
                placed(FIRST_PICTURE_ID, picture(2, 0))?
            )
        );
        Ok(())
    }

    #[test]
    fn turning_the_bar_off_keeps_layered_pictures_for_later() -> Result<()> {
        let mut painter = Painter::new(DrawingWay::Layered);
        paint(&mut painter, &[picture(1, 0)], Some(AREA))?;

        let off = paint(&mut painter, &[picture(1, 0)], None)?;
        let on = paint(&mut painter, &[picture(1, 0)], Some(AREA))?;

        assert_eq!(off, remove_placement_code(FIRST_PICTURE_ID, 1));
        assert_eq!(
            on,
            format!(
                "{SAVE_CURSOR}{}{RESTORE_CURSOR}",
                placed(FIRST_PICTURE_ID, picture(1, 0))?
            )
        );
        Ok(())
    }

    #[test]
    fn drawing_in_the_cells_holds_the_screen_and_erases_the_bar_first() -> Result<()> {
        let mut painter = Painter::new(DrawingWay::InCells);
        paint(&mut painter, &[picture(1, 0)], Some(AREA))?;

        let output = paint(&mut painter, &[picture(2, 0)], Some(AREA))?;

        let mut erase = String::new();
        push_erase_bar(&mut erase, AREA.first_row, AREA.last_row);
        let expected = format!(
            "{HOLD_SCREEN}{SAVE_CURSOR}{erase}{}{}{RESTORE_CURSOR}{RELEASE_SCREEN}",
            shown(FIRST_PICTURE_ID + PICTURE_ID_SET_SIZE, picture(2, 0))?,
            remove_code(FIRST_PICTURE_ID)
        );
        assert_eq!(output, expected);
        Ok(())
    }

    #[test]
    fn nothing_is_drawn_when_nothing_changed() -> Result<()> {
        let mut painter = Painter::new(DrawingWay::Layered);
        paint(&mut painter, &[picture(1, 0)], Some(AREA))?;

        assert_eq!(paint(&mut painter, &[picture(1, 0)], Some(AREA))?, "");
        Ok(())
    }

    #[test]
    fn a_wiped_bar_or_a_new_area_is_drawn_again() -> Result<()> {
        let mut painter = Painter::new(DrawingWay::Layered);
        paint(&mut painter, &[picture(1, 0)], Some(AREA))?;

        painter.bar_wiped();
        assert!(!paint(&mut painter, &[picture(1, 0)], Some(AREA))?.is_empty());
        assert_eq!(paint(&mut painter, &[picture(1, 0)], Some(AREA))?, "");

        let taller = BarArea {
            first_row: 39,
            last_row: 40,
            ..AREA
        };
        assert!(!paint(&mut painter, &[picture(1, 0)], Some(taller))?.is_empty());
        Ok(())
    }

    #[test]
    fn turning_the_bar_off_only_removes_the_pictures_in_the_cells() -> Result<()> {
        let mut painter = Painter::new(DrawingWay::InCells);
        paint(&mut painter, &[picture(1, 0)], Some(AREA))?;

        let off = paint(&mut painter, &[picture(1, 0)], None)?;

        assert_eq!(off, remove_code(FIRST_PICTURE_ID));
        assert_eq!(paint(&mut painter, &[picture(1, 0)], None)?, "");
        Ok(())
    }

    #[test]
    fn too_many_pictures_is_an_error_that_changes_nothing() -> Result<()> {
        let mut painter = Painter::new(DrawingWay::Layered);
        paint(&mut painter, &[picture(1, 0)], Some(AREA))?;
        let too_many = vec![picture(1, 0); MAX_PICTURES + 1];

        assert!(paint(&mut painter, &too_many, Some(AREA)).is_err());
        assert_eq!(paint(&mut painter, &[picture(1, 0)], Some(AREA))?, "");
        Ok(())
    }
}
