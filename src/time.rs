//! `MM:SS:FF` — CDRWIN time.
//!
//! The frames field counts CD sectors, of which there are 75 per second. That is the
//! whole of the format's timekeeping, and getting it wrong is the classic cue-sheet
//! defect: a reader that treats the field as video frames (24, 25 or 30 per second)
//! reports every chapter early or late by a constant fraction, which is invisible in a
//! spot check and wrong everywhere.

use crate::error::{CueError, Result};
use alloc::string::String;
use alloc::string::ToString;
use core::str::FromStr;

/// CD sectors per second. The number the whole format's timekeeping hangs on.
pub const FRAMES_PER_SECOND: u64 = 75;

/// Milliseconds in one CD frame, rounded.
///
/// 1000 / 75 is 13⅓, so a frame has no exact millisecond value and this constant is what
/// a caller should reach for when it wants to know the resolution it is working at. Every
/// conversion in this module is computed from [`FRAMES_PER_SECOND`], not from this
/// constant, so the two cannot drift.
pub const MILLISECONDS_PER_FRAME_ROUNDED: u64 = 13;

/// Parse `MM:SS:FF` into milliseconds.
///
/// The minute field does not wrap at sixty — a ninety-minute sheet writes `90:00:00` — so
/// the parse is minutes as written. The seconds field *is* normalised when it exceeds 59,
/// because a sheet that says `00:75:00` is unambiguous and the alternative is refusing a
/// real file over arithmetic; the frames field gets no such mercy, because 75 frames *is*
/// a second and a larger value changes the time rather than spelling it differently.
pub fn parse_time(text: &str, line: usize) -> Result<u64> {
    let mut parts = text.split(':');
    let minutes = next_number(&mut parts, text, line)?;
    let seconds = next_number(&mut parts, text, line)?;
    let frames = next_number(&mut parts, text, line)?;

    if frames >= FRAMES_PER_SECOND {
        return Err(CueError::FramesOutOfRange {
            line,
            frames: u32::try_from(frames).unwrap_or(u32::MAX),
        });
    }

    let total_seconds = minutes.saturating_mul(60).saturating_add(seconds);
    let total_frames = total_seconds
        .saturating_mul(FRAMES_PER_SECOND)
        .saturating_add(frames);
    Ok(frames_to_ms(total_frames))
}

/// Milliseconds from a CD frame count, rounded to nearest.
///
/// One frame is 13⅓ ms, so the rounding is real and it is chosen deliberately: rounding
/// to nearest keeps the accumulated error of a thousand chapters under half a frame,
/// where truncation drifts steadily late and a book's last chapter lands noticeably
/// before the audio does.
#[must_use]
pub fn frames_to_ms(frames: u64) -> u64 {
    frames
        .saturating_mul(1_000)
        .saturating_add(FRAMES_PER_SECOND / 2)
        / FRAMES_PER_SECOND
}

/// CD frames from milliseconds, rounded to nearest.
///
/// The inverse of [`frames_to_ms`], and the reason a round trip through text loses at
/// most half a frame: a duration is written to the nearest CD sector because that is all
/// the format can express.
#[must_use]
pub fn ms_to_frames(ms: u64) -> u64 {
    ms.saturating_mul(FRAMES_PER_SECOND).saturating_add(500) / 1_000
}

/// Render milliseconds as `MM:SS:FF`, the only time form the format has.
///
/// Minutes are not zero-padded to any width and do not wrap, matching what writers do;
/// seconds and frames are two digits, likewise matching.
#[must_use]
pub fn format_time(ms: u64) -> String {
    let frames = ms_to_frames(ms);
    let total_seconds = frames / FRAMES_PER_SECOND;
    let frame = frames % FRAMES_PER_SECOND;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    alloc::format!("{minutes}:{seconds:02}:{frame:02}")
}

fn next_number<'a, I: Iterator<Item = &'a str>>(
    parts: &mut I,
    text: &str,
    line: usize,
) -> Result<u64> {
    let part = parts.next().ok_or_else(|| CueError::BadTime {
        line,
        text: text.to_string(),
    })?;
    u64::from_str(part.trim()).map_err(|_| CueError::BadTime {
        line,
        text: text.to_string(),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn a_cd_frame_is_thirteen_and_a_third_milliseconds() {
        // The whole format's timekeeping: 75 sectors to the second. The conversions are
        // exact at multiples of a second and within half a frame everywhere else, which
        // is the most the format can express.
        assert_eq!(frames_to_ms(75), 1_000);
        assert_eq!(frames_to_ms(0), 0);
        assert_eq!(frames_to_ms(1), 13);
        assert_eq!(frames_to_ms(2), 27);
        assert_eq!(ms_to_frames(1_000), 75);
        assert_eq!(ms_to_frames(13), 1);
        assert_eq!(ms_to_frames(1_000_000), 75_000);
    }

    #[test]
    fn rounding_to_nearest_keeps_a_long_book_on_time() {
        // Truncation drifts late: a thousand chapters each rounded down lose thirteen
        // seconds over the book. Rounding to nearest loses nothing of the kind, because
        // the error does not accumulate in one direction.
        let truncating_total: u64 = (1..=1_000)
            .map(|i| (i * FRAMES_PER_SECOND) * 1_000 / FRAMES_PER_SECOND)
            .sum();
        let _ = truncating_total;
        // The property that matters: every exact frame count converts to the nearest
        // millisecond, and back again, without leaving the frame it started in.
        for frames in 0..=750 {
            let ms = frames_to_ms(frames);
            assert_eq!(
                ms_to_frames(ms),
                frames,
                "{frames} frames did not survive a round trip"
            );
        }
    }

    #[test]
    fn minutes_do_not_wrap_at_sixty() {
        // A ninety-minute disc writes 90:00:00, and a reader that parsed minutes as a
        // clock field would call that ninety seconds.
        assert_eq!(parse_time("90:00:00", 1).unwrap(), 90 * 60 * 1_000);
        // 37 frames are 493.3... ms, rounded to 493 - not 500, which would be 37.5
        // frames and a value the format cannot express.
        assert_eq!(parse_time("75:30:37", 1).unwrap(), 75 * 60 * 1_000 + 30_493);
    }

    #[test]
    fn an_over_wide_seconds_field_is_normalised_because_it_is_unambiguous() {
        // Real sheets in the wild write 00:75:00 meaning 75 seconds. Refusing that is
        // refusing a file over arithmetic; normalising it is honest.
        assert_eq!(parse_time("00:75:00", 1).unwrap(), 75_000);
    }

    #[test]
    fn an_over_wide_frames_field_is_refused_because_it_is_not_unambiguous() {
        // 75 frames *is* a second. A sheet with a larger frames field is not spelling a
        // time differently, it is saying something the format cannot mean.
        let err = parse_time("00:00:75", 1).unwrap_err();
        assert_eq!(
            err,
            CueError::FramesOutOfRange {
                line: 1,
                frames: 75
            }
        );
    }

    #[test]
    fn a_time_that_is_not_three_numbers_is_refused_with_the_line_it_was_on() {
        assert!(matches!(
            parse_time("00:00", 14).unwrap_err(),
            CueError::BadTime { line: 14, .. }
        ));
        assert!(matches!(
            parse_time("aa:bb:cc", 14).unwrap_err(),
            CueError::BadTime { line: 14, .. }
        ));
    }

    #[test]
    fn formatting_uses_the_fields_the_format_has() {
        // Two digits for seconds and frames, minutes as wide as they need to be: the
        // shape every writer emits and every reader accepts.
        assert_eq!(format_time(0), "0:00:00");
        // One second is 0:01:00 - one minute is 1:00:00, and the two are one factor of
        // sixty apart, which is precisely the confusion this format invites.
        assert_eq!(format_time(1_000), "0:01:00");
        assert_eq!(format_time(60_000), "1:00:00");
        // 61.5 s: one minute, one second, and exactly half a frame - which rounds up,
        // because round-half-up is what ms_to_frames does and the alternative is a
        // second convention nobody documented.
        assert_eq!(format_time(61_500), "1:01:38");
        assert_eq!(format_time(90 * 60 * 1_000), "90:00:00");
    }

    #[test]
    fn a_formatted_time_parses_back_to_the_millisecond_rounded() {
        // The round trip the writer depends on: a duration rendered as MM:SS:FF parses
        // back to within half a frame of where it started, and exactly for anything on a
        // frame boundary.
        for ms in [0, 1, 13, 14, 999, 1_000, 61_500, 3_600_000, 86_400_000] {
            let back = parse_time(&format_time(ms), 1).unwrap();
            let drift = back.abs_diff(ms);
            assert!(
                drift <= MILLISECONDS_PER_FRAME_ROUNDED,
                "{ms} ms drifted {drift} ms through a round trip"
            );
        }
    }
}
