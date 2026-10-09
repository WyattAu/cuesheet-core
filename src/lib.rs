//! CDRWIN cue sheet reader and writer.
//!
//! `cuesheet-core` is the estate's L0 layer for the file format a CD rip uses to say
//! where its tracks are. A rip is one long audio file plus a `.cue` — a text sidecar
//! listing `TRACK` boundaries in `MM:SS:FF` — and that list is a book's chapter list, so
//! this crate is the front door for the largest class of audiobook sources there is.
//!
//! It is `#![no_std]` + `alloc` with one runtime dependency (`thiserror`, the same
//! typed-error choice as every other L0 kit).
//!
//! # What it covers
//!
//! - **`FILE` / `TRACK` / `INDEX`**, the structural spine, including multiple `FILE`
//!   entries (one per disc is the common shape for a long book).
//! - **`MM:SS:FF`**, CDRWIN's time: minutes, seconds, and *frames* at 75 per second —
//!   CD sectors, not video frames. The minute field does not wrap at sixty.
//! - **CD-TEXT**: `TITLE`, `PERFORMER`, `SONGWRITER`, at sheet level (the album) and
//!   per track.
//! - **`REM`** comment lines, skipped.
//! - **The pregap pair**: `INDEX 00` marks where a track's gap begins, `INDEX 01` where
//!   its audio starts. A chapter list wants `INDEX 01`; [`CueFile::chapter_starts`]
//!   gives exactly that.
//!
//! # What it deliberately does not do
//!
//! - **No audio.** A sheet names files; opening them is the caller's business.
//! - **No disc-image arithmetic.** `MODE1/2352` and friends are recorded as declared, and
//!   sector offsets into a raw image are not computed — an audiobook is an `MP3` or
//!   `WAVE` file, and a raw image is not a book.
//!
//! # Verification honesty
//!
//! No independent cue-sheet implementation exists on the machine this crate was written
//! on — ffmpeg's cue demuxer is not compiled in, libcue is absent, and there is no
//! package manager to fetch one — so the *grammar* (quoting, `REM` handling, CD-TEXT
//! fields) is exercised by this crate's own tests alone. What *is* checked against an
//! external oracle is the arithmetic that matters: a sheet's `INDEX 01` times pushed into
//! an M4B by the estate's writer come back out of ffprobe as the same milliseconds. The
//! times are what a chapter list is for; the grammar is plumbing around them.
#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

extern crate alloc;

pub mod error;
pub mod sheet;
pub mod time;

pub use error::CueError;
pub use sheet::{CueFile, FileType, Index, Sheet, Track, TrackMode};
pub use time::{format_time, frames_to_ms, ms_to_frames, parse_time};
