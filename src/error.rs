//! Errors a cue sheet can produce, with the line they were found on.
//!
//! A line number is not decoration: a sheet is written by a human with an editor, and
//! "bad line 14" is the difference between a fifteen-second fix and an hour of staring at
//! a file that "looks fine".

use alloc::string::String;

/// The result of parsing or writing a cue sheet.
pub type Result<T, E = CueError> = core::result::Result<T, E>;

/// What went wrong, and where.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CueError {
    /// A line could not be understood.
    #[error("line {line}: unknown command {command:?}")]
    UnknownCommand {
        /// The one-based line number.
        line: usize,
        /// The command as it appeared.
        command: String,
    },
    /// A `TRACK` appeared before any `FILE`, so there is nothing for it to belong to.
    #[error("line {line}: TRACK before any FILE")]
    TrackBeforeFile {
        /// The one-based line number.
        line: usize,
    },
    /// A time field was not `MM:SS:FF`.
    #[error("line {line}: {text:?} is not MM:SS:FF")]
    BadTime {
        /// The one-based line number.
        line: usize,
        /// The text as it appeared.
        text: String,
    },
    /// The frames field was 75 or more.
    ///
    /// The frames field counts CD sectors at 75 per second, so 75 frames is one second
    /// and a sheet that says otherwise is not saying something this crate can guess at.
    /// Unlike an over-wide seconds field — which is unambiguous and normalised — an
    /// out-of-range frames field changes the value, so it is refused.
    #[error("line {line}: {frames} frames is out of range (0..75)")]
    FramesOutOfRange {
        /// The one-based line number.
        line: usize,
        /// The frames value as written.
        frames: u32,
    },
    /// A numeric field was not a number.
    #[error("line {line}: {text:?} is not a number")]
    BadNumber {
        /// The one-based line number.
        line: usize,
        /// The text as it appeared.
        text: String,
    },
    /// A quoted string was still open at end of line.
    #[error("line {line}: a quoted string was not closed")]
    UnterminatedQuote {
        /// The one-based line number.
        line: usize,
    },
    /// A `TRACK` number was outside 1..=99.
    #[error("line {line}: track number {number} is out of range (1..=99)")]
    TrackNumberOutOfRange {
        /// The one-based line number.
        line: usize,
        /// The number as written.
        number: u32,
    },
    /// An `INDEX` number was outside 0..=99.
    #[error("line {line}: index number {number} is out of range (0..=99)")]
    IndexNumberOutOfRange {
        /// The one-based line number.
        line: usize,
        /// The number as written.
        number: u32,
    },
    /// An `INDEX` appeared before any `TRACK`, so there is nothing for it to belong to.
    #[error("line {line}: INDEX before any TRACK")]
    IndexBeforeTrack {
        /// The one-based line number.
        line: usize,
    },
    /// Writing produced text that does not parse back.
    #[error("internal: a written sheet did not parse back: {0}")]
    RoundTrip(String),
    /// A CD-TEXT value contained a line break.
    ///
    /// The format has no escape for one — a quoted string still ends at its closing
    /// quote, and the newline ends the *line* — so a value carrying one cannot be written
    /// as text. Mangling it to a space would be silent, and a mangled title is a title
    /// the user cannot find; refused, with the value, so the caller decides.
    #[error("{field} contains a line break and cannot be written: {value:?}")]
    NewlineInValue {
        /// Which field, for the error message.
        field: &'static str,
        /// The value as it is.
        value: String,
    },
}
