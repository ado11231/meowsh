//! Asks the real terminal whether it can show pictures & what colors its text & background
//! are, before the bar is set up.
//!
//! Meowsh sends a test picture that is only asked about, never shown, asks for the two
//! colors, then asks what the terminal is. Every terminal answers the second question, so once that answer arrives,
//! any answer before it has arrived too. Keys typed while waiting are kept, so they
//! can be passed on to the shell.

use super::escape::{
    ASK_BACKGROUND_COLOR, ASK_DEVICE_ATTRIBUTES, ASK_TEXT_COLOR, BACKGROUND_COLOR_START, BELL,
    DEVICE_ATTRIBUTES, DEVICE_ATTRIBUTES_START, PICTURE_OK, PICTURE_START, RGB_COLOR_PREFIX,
    RGB_COLOR_SPLIT, STRING_END, TEXT_COLOR_START,
};
use super::raw::RawModeGuard;
use anyhow::{Context, Result};
use std::io::Write;
use std::time::{Duration, Instant};

/// The number the test picture is sent under, so its answer can be told apart.
pub const CHECK_PICTURE_ID: u32 = 7701;

/// The test picture's settings & data. `a=q` only asks & shows nothing, `t=d` sends the
/// pixels inside the code, `f=24` means RGB, `s` & `v` are the width & height, and `AAAA` is
/// one black pixel.
const CHECK_PICTURE: &str = "a=q,t=d,f=24,s=1,v=1;AAAA";

/// How long to wait for the terminal to answer.
const ANSWER_TIMEOUT: Duration = Duration::from_millis(500);

/// Room for one read of the terminal's answers.
const READ_BUFFER_SIZE: usize = 1024;

/// The most hex digits a terminal sends for one of red, green or blue.
const MOST_COLOR_DIGITS: usize = 4;

/// What the check found.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImageCheck {
    /// Whether the terminal said it can show the test picture.
    pub supported: bool,
    /// Keys typed during the check, in the order they came.
    pub early_input: Vec<u8>,
    /// The terminal's text color as red, green & blue, if it said.
    pub text_color: Option<[u8; 3]>,
    /// The terminal's background color as red, green & blue, if it said.
    pub background_color: Option<[u8; 3]>,
}

/// The codes that ask the terminal about pictures & its colors.
pub fn check_codes() -> String {
    format!(
        "{PICTURE_START}i={CHECK_PICTURE_ID},{CHECK_PICTURE}{STRING_END}{ASK_TEXT_COLOR}\
         {ASK_BACKGROUND_COLOR}{ASK_DEVICE_ATTRIBUTES}"
    )
}

/// The answer that says the test picture worked.
pub fn picture_ok_answer() -> String {
    format!("{PICTURE_START}{}{STRING_END}", picture_ok_body())
}

/// Asks the real terminal whether it can show pictures & its colors, & waits a short time for the answer.
///
/// Raw mode is on only while waiting, so the answers are not shown on screen.
pub fn check_images() -> Result<ImageCheck> {
    let _raw_mode = RawModeGuard::enable()?;
    let mut output = std::io::stdout();
    output
        .write_all(check_codes().as_bytes())
        .and_then(|()| output.flush())
        .context("could not ask the terminal about pictures")?;

    let deadline = Instant::now() + ANSWER_TIMEOUT;
    let mut answers = AnswerReader::default();
    let mut buffer = [0u8; READ_BUFFER_SIZE];
    while !answers.finished {
        let time_left = deadline.saturating_duration_since(Instant::now());
        if time_left.is_zero() || !wait_for_input(time_left)? {
            break;
        }
        let count = read_input(&mut buffer)?;
        if count == 0 {
            break;
        }
        answers.read(&buffer[..count]);
    }
    Ok(answers.finish())
}

/// Waits until stdin has bytes to read. Returns `false` if the time ran out first.
fn wait_for_input(timeout: Duration) -> Result<bool> {
    let mut stdin = libc::pollfd {
        fd: libc::STDIN_FILENO,
        events: libc::POLLIN,
        revents: 0,
    };
    let milliseconds = libc::c_int::try_from(timeout.as_millis()).unwrap_or(libc::c_int::MAX);
    // SAFETY: `stdin` is one valid pollfd, and the count passed is 1.
    let result = unsafe { libc::poll(&mut stdin, 1, milliseconds) };
    if result < 0 {
        let error = std::io::Error::last_os_error();
        // A signal such as a resize woke the wait early. The loop waits again.
        if error.kind() == std::io::ErrorKind::Interrupted {
            return Ok(true);
        }
        return Err(error).context("could not wait for the terminal's answer");
    }
    Ok(result > 0)
}

/// Reads what stdin has right now.
///
/// This reads the file directly, because the standard stdin keeps its own buffer, and
/// `wait_for_input` cannot see bytes waiting in it.
fn read_input(buffer: &mut [u8]) -> Result<usize> {
    // SAFETY: the buffer is writable, and its length is passed along with it.
    let count = unsafe { libc::read(libc::STDIN_FILENO, buffer.as_mut_ptr().cast(), buffer.len()) };
    if count < 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::Interrupted {
            return Ok(0);
        }
        return Err(error).context("could not read the terminal's answer");
    }
    // Not negative, so it fits.
    Ok(usize::try_from(count).unwrap_or(0))
}

/// Sorts what stdin sends during the check into answers & typed keys.
#[derive(Default)]
struct AnswerReader {
    /// Bytes that could still be the start of an answer.
    unread: Vec<u8>,
    supported: bool,
    text_color: Option<[u8; 3]>,
    background_color: Option<[u8; 3]>,
    /// Whether the answer about the terminal arrived, so no more answers are coming.
    finished: bool,
    early_input: Vec<u8>,
}

/// Which color a color answer is about.
#[derive(Clone, Copy)]
enum ColorKind {
    Text,
    Background,
}

/// The next piece of what stdin sent.
enum Piece {
    /// An answer about a picture, this many bytes long, and whether it was the test
    /// picture working.
    PictureAnswer(usize, bool),
    /// An answer about a color, this many bytes long, which color it was about, & the
    /// color if it could be read.
    Color(usize, ColorKind, Option<[u8; 3]>),
    /// The answer about the terminal, this many bytes long.
    DeviceAttributes(usize),
    /// This many bytes the user typed.
    Typed(usize),
    /// The start of what may be an answer. More bytes are needed.
    Unfinished,
}

impl AnswerReader {
    fn read(&mut self, bytes: &[u8]) {
        self.unread.extend_from_slice(bytes);
        while !self.unread.is_empty() {
            if self.finished {
                self.early_input.append(&mut self.unread);
                return;
            }
            let length = match next_piece(&self.unread) {
                Piece::PictureAnswer(length, worked) => {
                    self.supported |= worked;
                    length
                }
                Piece::Color(length, kind, color) => {
                    match kind {
                        ColorKind::Text => self.text_color = color,
                        ColorKind::Background => self.background_color = color,
                    }
                    length
                }
                Piece::DeviceAttributes(length) => {
                    self.finished = true;
                    length
                }
                Piece::Typed(length) => {
                    self.early_input.extend_from_slice(&self.unread[..length]);
                    length
                }
                Piece::Unfinished => return,
            };
            self.unread.drain(..length);
        }
    }

    /// Ends the check. Bytes that never became a whole answer were typed.
    fn finish(mut self) -> ImageCheck {
        self.early_input.append(&mut self.unread);
        ImageCheck {
            supported: self.supported,
            early_input: self.early_input,
            text_color: self.text_color,
            background_color: self.background_color,
        }
    }
}

fn next_piece(bytes: &[u8]) -> Piece {
    let picture_start = PICTURE_START.as_bytes();
    let attributes_start = DEVICE_ATTRIBUTES_START.as_bytes();
    let color_starts = [
        (TEXT_COLOR_START.as_bytes(), ColorKind::Text),
        (BACKGROUND_COLOR_START.as_bytes(), ColorKind::Background),
    ];

    if let Some(rest) = bytes.strip_prefix(picture_start) {
        let Some(end) = find(rest, STRING_END.as_bytes()) else {
            return Piece::Unfinished;
        };
        let worked = rest[..end] == *picture_ok_body().as_bytes();
        return Piece::PictureAnswer(picture_start.len() + end + STRING_END.len(), worked);
    }
    for (color_start, kind) in color_starts {
        if let Some(rest) = bytes.strip_prefix(color_start) {
            let Some((end, end_length)) = find_osc_end(rest) else {
                return Piece::Unfinished;
            };
            let color = std::str::from_utf8(&rest[..end]).ok().and_then(read_color);
            return Piece::Color(color_start.len() + end + end_length, kind, color);
        }
    }
    if let Some(rest) = bytes.strip_prefix(attributes_start) {
        // A CSI code ends with its first byte from `@` to `~`.
        let Some(end) = rest.iter().position(|byte| (b'@'..=b'~').contains(byte)) else {
            return Piece::Unfinished;
        };
        let length = attributes_start.len() + end + 1;
        if rest[end] == DEVICE_ATTRIBUTES {
            return Piece::DeviceAttributes(length);
        }
        return Piece::Typed(length);
    }
    if picture_start.starts_with(bytes)
        || attributes_start.starts_with(bytes)
        || color_starts
            .iter()
            .any(|(color_start, _)| color_start.starts_with(bytes))
    {
        return Piece::Unfinished;
    }
    Piece::Typed(1)
}

/// The part of `picture_ok_answer` between its start & end codes.
fn picture_ok_body() -> String {
    format!("i={CHECK_PICTURE_ID};{PICTURE_OK}")
}

/// Where an OSC answer ends, & how long its end code is. Terminals end it with either
/// `STRING_END` or `BELL`.
fn find_osc_end(bytes: &[u8]) -> Option<(usize, usize)> {
    let string_end = find(bytes, STRING_END.as_bytes()).map(|end| (end, STRING_END.len()));
    let bell = find(bytes, &[BELL as u8]).map(|end| (end, 1));
    match (string_end, bell) {
        (Some(first), Some(second)) => Some(first.min(second)),
        (first, second) => first.or(second),
    }
}

/// Reads `rgb:R/G/B` into red, green & blue. Each number is 1 to 4 hex digits, so it is
/// scaled from its own most to 255.
fn read_color(text: &str) -> Option<[u8; 3]> {
    let mut numbers = text
        .strip_prefix(RGB_COLOR_PREFIX)?
        .split(RGB_COLOR_SPLIT)
        .map(read_color_number);
    let color = [numbers.next()??, numbers.next()??, numbers.next()??];
    match numbers.next() {
        Some(_) => None,
        None => Some(color),
    }
}

/// Reads one color number of 1 to 4 hex digits & scales it to 0 to 255.
fn read_color_number(digits: &str) -> Option<u8> {
    if digits.is_empty()
        || digits.len() > MOST_COLOR_DIGITS
        || !digits.bytes().all(|digit| digit.is_ascii_hexdigit())
    {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    let most = 16u32.pow(u32::try_from(digits.len()).ok()?) - 1;
    // Rounded to the nearest whole number.
    u8::try_from((value * 255 + most / 2) / most).ok()
}

fn find(bytes: &[u8], part: &[u8]) -> Option<usize> {
    bytes.windows(part.len()).position(|window| window == part)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An answer to `ASK_DEVICE_ATTRIBUTES`, as kitty sends it.
    const ATTRIBUTES_ANSWER: &str = "\x1b[?62;c";

    /// An answer to `ASK_TEXT_COLOR`, as kitty sends it, & the color it holds.
    const TEXT_COLOR_ANSWER: &str = "\x1b]10;rgb:1e1e/8080/ffff\x1b\\";
    const TEXT_COLOR: [u8; 3] = [0x1e, 0x80, 0xff];

    /// An answer to `ASK_BACKGROUND_COLOR`, as kitty sends it, & the color it holds.
    const BACKGROUND_COLOR_ANSWER: &str = "\x1b]11;rgb:0000/1010/2020\x1b\\";
    const BACKGROUND_COLOR: [u8; 3] = [0x00, 0x10, 0x20];

    fn check(chunks: &[&[u8]]) -> ImageCheck {
        let mut answers = AnswerReader::default();
        for chunk in chunks {
            answers.read(chunk);
        }
        answers.finish()
    }

    fn joined(parts: &[&str]) -> Vec<u8> {
        parts.concat().into_bytes()
    }

    #[test]
    fn the_check_asks_about_a_picture_then_the_colors_then_the_terminal() {
        let expected = format!(
            "{PICTURE_START}i={CHECK_PICTURE_ID},a=q,t=d,f=24,s=1,v=1;AAAA{STRING_END}\
             {ASK_TEXT_COLOR}{ASK_BACKGROUND_COLOR}{ASK_DEVICE_ATTRIBUTES}"
        );

        assert_eq!(check_codes(), expected);
    }

    #[test]
    fn a_terminal_that_says_ok_can_show_pictures() {
        let answers = joined(&[
            &picture_ok_answer(),
            TEXT_COLOR_ANSWER,
            BACKGROUND_COLOR_ANSWER,
            ATTRIBUTES_ANSWER,
        ]);

        let result = check(&[&answers]);

        assert_eq!(
            result,
            ImageCheck {
                supported: true,
                early_input: Vec::new(),
                text_color: Some(TEXT_COLOR),
                background_color: Some(BACKGROUND_COLOR),
            }
        );
    }

    #[test]
    fn a_text_color_answer_ended_by_bell_is_read() {
        let answer = format!("{TEXT_COLOR_START}rgb:1e1e/8080/ffff{BELL}");
        let answers = joined(&[&answer, ATTRIBUTES_ANSWER]);

        let result = check(&[&answers]);

        assert_eq!(result.text_color, Some(TEXT_COLOR));
        assert!(result.early_input.is_empty());
    }

    #[test]
    fn a_text_color_that_cannot_be_read_is_dropped_not_typed() {
        let answer = format!("{TEXT_COLOR_START}cmyk:1/2/3/4{STRING_END}");
        let answers = joined(&[&answer, ATTRIBUTES_ANSWER]);

        let result = check(&[&answers]);

        assert_eq!(result.text_color, None);
        assert!(result.early_input.is_empty());
    }

    #[test]
    fn color_numbers_of_any_length_are_scaled_to_255() {
        assert_eq!(read_color("rgb:ffff/8080/0000"), Some([255, 128, 0]));
        assert_eq!(read_color("rgb:ff/80/00"), Some([255, 128, 0]));
        assert_eq!(read_color("rgb:f/8/0"), Some([255, 136, 0]));
    }

    #[test]
    fn a_color_that_is_not_three_hex_numbers_is_not_read() {
        for text in [
            "rgb:ff/ff",
            "rgb:ff/ff/ff/ff",
            "rgb:ff//ff",
            "rgb:fffff/ff/ff",
            "rgb:+f/ff/ff",
            "rgb:gg/ff/ff",
            "ff/ff/ff",
        ] {
            assert_eq!(read_color(text), None, "{text}");
        }
    }

    #[test]
    fn a_terminal_that_only_answers_about_itself_cannot_show_pictures() {
        assert_eq!(
            check(&[ATTRIBUTES_ANSWER.as_bytes()]),
            ImageCheck::default()
        );
    }

    #[test]
    fn an_error_or_another_pictures_answer_does_not_count() {
        let error = format!("{PICTURE_START}i={CHECK_PICTURE_ID};ENOTSUPPORTED{STRING_END}");
        let other = format!("{PICTURE_START}i=1;{PICTURE_OK}{STRING_END}");
        let answers = joined(&[&error, &other, ATTRIBUTES_ANSWER]);

        assert_eq!(check(&[&answers]), ImageCheck::default());
    }

    #[test]
    fn answers_split_across_reads_are_still_read() {
        let answers = joined(&[
            &picture_ok_answer(),
            TEXT_COLOR_ANSWER,
            BACKGROUND_COLOR_ANSWER,
            ATTRIBUTES_ANSWER,
        ]);
        let chunks: Vec<&[u8]> = answers.chunks(1).collect();

        let result = check(&chunks);

        assert!(result.supported);
        assert_eq!(result.text_color, Some(TEXT_COLOR));
        assert_eq!(result.background_color, Some(BACKGROUND_COLOR));
        assert!(result.early_input.is_empty());
    }

    #[test]
    fn keys_typed_around_the_answers_are_kept_in_order() {
        let answers = joined(&[
            "ls",
            "\x1b[A",
            &picture_ok_answer(),
            " -l",
            ATTRIBUTES_ANSWER,
            "\r",
        ]);

        let result = check(&[&answers]);

        assert!(result.supported);
        assert_eq!(result.early_input, b"ls\x1b[A -l\r");
    }

    #[test]
    fn an_answer_cut_off_by_the_timeout_counts_as_typed() {
        let result = check(&[b"\x1b_Gi=77"]);

        assert!(!result.supported);
        assert_eq!(result.early_input, b"\x1b_Gi=77");
    }

    #[test]
    fn a_lone_escape_key_is_kept() {
        let result = check(&[b"\x1b"]);

        assert_eq!(result.early_input, b"\x1b");
    }
}
