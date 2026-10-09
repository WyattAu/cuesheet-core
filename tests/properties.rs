//! Property tests for the parts of the format that are arithmetic rather than grammar.
//!
//! Time is where a cue-sheet reader can be quietly wrong, because every wrong answer is
//! still a plausible number of milliseconds. These properties are what hold regardless of
//! input: conversions invert within the format's own resolution, and a parsed sheet
//! survives a write/parse cycle unchanged.

// Fixtures are allowed to assert: an `expect` here is the test stating what it assumes,
// which is the honest form, and the crate lint denies it everywhere a caller would write
// it.
#![allow(clippy::expect_used)]

use cuesheet_core::{format_time, frames_to_ms, ms_to_frames, parse_time, Sheet};

// The macros come with the `proptest!` expansion's hygiene rules: they are importable
// and must be, because a macro that cannot be named cannot be used inside another
// macro's body.
use proptest::prelude::{prop_assert, prop_assert_eq};

proptest::proptest! {
    /// Every frame count survives a conversion, back, to the frame it started in.
    #[test]
    fn a_frame_count_survives_a_round_trip(frames in 0u64..(120 * 60 * 75)) {
        let ms = frames_to_ms(frames);
        prop_assert_eq!(ms_to_frames(ms), frames);
    }

    /// Conversions never disagree by more than the format's resolution, in either
    /// direction — the resolution is the promise, and it is what makes a chapter list
    /// written from a sheet line up with the audio.
    #[test]
    fn conversions_stay_within_one_frame(ms in 0u64..(120 * 60 * 1000)) {
        let frames = ms_to_frames(ms);
        let back = frames_to_ms(frames);
        prop_assert!(back.abs_diff(ms) <= 13, "{ms} ms drifted to {back}");
    }

    /// A sheet built from parse -> write -> parse is the same sheet, whatever the
    /// titles contain. Titles are where writers differ most, so this is generated
    /// rather than fixed.
    #[test]
    fn a_written_sheet_parses_back_to_itself(
        // Line breaks are excluded because the format cannot express one inside a
        // quoted value — the writer refuses them, and a strategy that generated them
        // would be testing that refusal rather than the round trip.
        title in "[^\"\n\r]{0,60}",
        performer in "[^\"\n\r]{0,40}",
        track_title in "[^\"\n\r]{0,60}",
        minutes in 0u64..200,
    ) {
        let text = format!(
            "PERFORMER \"{performer}\"\nTITLE \"{title}\"\nFILE \"a.mp3\" MP3\n  TRACK 01 AUDIO\n    TITLE \"{track_title}\"\n    INDEX 01 {minutes}:00:00\n"
        );
        let sheet = Sheet::parse(&text).expect("parses");
        let back = Sheet::parse(&sheet.to_text().expect("writable")).expect("parses");
        prop_assert_eq!(back, sheet);
    }

    /// A time written by the formatter parses back to within one frame — the property
    /// a chapter list's accuracy rests on.
    #[test]
    fn a_formatted_time_parses_back_within_a_frame(ms in 0u64..(120 * 60 * 1000)) {
        let text = format_time(ms);
        let back = parse_time(&text, 1).expect("the formatted time parses");
        prop_assert!(back.abs_diff(ms) <= 13, "{ms} ms became {text} became {back}");
    }
}
