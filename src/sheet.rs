//! The sheet: files, tracks, indices, and the text that names them.

use crate::error::{CueError, Result};
use crate::time::parse_time;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// One `FILE` entry and every track it holds.
///
/// A sheet for a multi-disc book has one `FILE` per disc, and each file's track times are
/// *relative to that file*. A caller that wants book-relative times must add the durations
/// of the files before this one — which needs the audio files themselves, so it is the
/// caller's arithmetic and not this crate's guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueFile {
    /// The audio file's name as the sheet wrote it, quotes removed.
    ///
    /// Kept as written rather than resolved: the sheet may live in a different directory
    /// from the audio, and joining paths is the caller's decision.
    pub path: String,
    /// The declared file type.
    pub file_type: FileType,
    /// The tracks, in sheet order.
    pub tracks: Vec<Track>,
}

impl CueFile {
    /// The chapter list this file describes: `INDEX 01` starts, with titles.
    ///
    /// `INDEX 00` is the *pregap* — where the silence before a track begins — and is
    /// deliberately not a chapter boundary. A reader that made gaps into chapters puts a
    /// chapter of silence between every pair of real ones, which is the defect that makes
    /// a ripped audiobook unlistenable: skip-forward lands you in silence, twice.
    ///
    /// Tracks with no `INDEX 01` are skipped rather than contributed at zero: a track
    /// whose start is unknown cannot be placed, and a chapter at time zero for every such
    /// track is a list that looks plausible and is wrong.
    #[must_use]
    pub fn chapter_starts(&self) -> Vec<(u64, Option<String>)> {
        self.tracks
            .iter()
            .filter_map(|t| t.index_one().map(|ms| (ms, t.title.clone())))
            .collect()
    }
}

/// The file type a `FILE` line declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileType {
    /// Microsoft WAVE.
    Wave,
    /// MP3.
    Mp3,
    /// Apple AIFF.
    Aiff,
    /// A raw CD image, one file for the whole disc.
    Binary,
    /// Something this crate does not know. Recorded rather than refused, because the
    /// times do not depend on the container and a sheet that names an unknown type is
    /// still a list of chapters.
    Other(String),
}

impl core::fmt::Display for FileType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FileType::Wave => f.write_str("WAVE"),
            FileType::Mp3 => f.write_str("MP3"),
            FileType::Aiff => f.write_str("AIFF"),
            FileType::Binary => f.write_str("BINARY"),
            FileType::Other(s) => f.write_str(s),
        }
    }
}

/// A `TRACK` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    /// The track number, 1..=99, as the sheet wrote it.
    pub number: u8,
    /// The track mode.
    pub mode: TrackMode,
    /// CD-TEXT title.
    pub title: Option<String>,
    /// CD-TEXT performer.
    pub performer: Option<String>,
    /// CD-TEXT songwriter.
    pub songwriter: Option<String>,
    /// Every `INDEX` in sheet order.
    pub indices: Vec<Index>,
    /// Any `POSTGAP`/`PREGAP` time, in milliseconds.
    pub gaps: Vec<(GapKind, u64)>,
}

impl Track {
    /// The milliseconds this track's audio starts at, from `INDEX 01`.
    #[must_use]
    pub fn index_one(&self) -> Option<u64> {
        self.indices.iter().find(|i| i.number == 1).map(|i| i.ms)
    }

    /// The milliseconds this track's pregap begins at, from `INDEX 00`.
    ///
    /// `None` for a track with no pregap, which is most tracks on most rips.
    #[must_use]
    pub fn index_zero(&self) -> Option<u64> {
        self.indices.iter().find(|i| i.number == 0).map(|i| i.ms)
    }
}

/// A `TRACK`'s mode, as declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackMode {
    /// `AUDIO` — the only mode a book is.
    Audio,
    /// `MODE1/2048` and `MODE1/2352`.
    Mode1(u16),
    /// `MODE2/2336` and `MODE2/2352`.
    Mode2(u16),
    /// A mode this crate does not know, recorded rather than refused.
    Other(String),
}

/// An `INDEX` line: a number and a time relative to the containing `FILE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Index {
    /// The index number, 0..=99.
    pub number: u8,
    /// Milliseconds from the start of the file.
    pub ms: u64,
}

/// Which kind of gap a `PREGAP`/`POSTGAP` line declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapKind {
    /// `PREGAP` — silence before the track.
    Pre,
    /// `POSTGAP` — silence after the track.
    Post,
}

/// A parsed sheet.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Sheet {
    /// Album-level CD-TEXT.
    pub title: Option<String>,
    /// Album-level CD-TEXT.
    pub performer: Option<String>,
    /// Album-level CD-TEXT.
    pub songwriter: Option<String>,
    /// The files, in sheet order.
    pub files: Vec<CueFile>,
    /// `REM` pairs this crate kept, for callers that want them.
    ///
    /// Most `REM` lines are noise; `REM DATE 1994` and `REM DISC_ID` are not. Kept as
    /// written rather than parsed into fields, because the set of keys in the wild is
    /// open-ended and a parser that models four of them implies the other four hundred.
    pub remarks: Vec<(String, String)>,
}

impl Sheet {
    /// Parse a sheet.
    ///
    /// # Errors
    ///
    /// [`CueError`] with the line number, for anything that cannot be read. Unknown
    /// commands are refused rather than skipped: a sheet is a list of times, and skipping
    /// an unrecognised line that carried one is how a book loses a chapter quietly.
    pub fn parse(text: &str) -> Result<Sheet> {
        let mut sheet = Sheet::default();
        // The FILE / TRACK / INDEX spine is a stack of contexts: which file, which track.
        let mut files: Vec<CueFile> = Vec::new();
        let mut track: Option<Track> = None;

        for (index, raw) in text.lines().enumerate() {
            let line = index + 1;
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            let (command, rest) = split_command(trimmed);
            match command {
                "REM" => {
                    // `REM` with no argument, and `REM` with one, are both legal; a
                    // `REM KEY value` is kept so a caller can find `DATE` if it wants it.
                    let mut parts = rest.splitn(2, char::is_whitespace);
                    if let Some(key) = parts.next() {
                        let value = parts.next().unwrap_or("").trim();
                        sheet.remarks.push((unquote(key), unquote(value)));
                    }
                }
                "PERFORMER" => apply_text(
                    |s| &mut s.performer,
                    |t| &mut t.performer,
                    &mut sheet,
                    &mut track,
                    rest,
                    line,
                ),
                "SONGWRITER" => apply_text(
                    |s| &mut s.songwriter,
                    |t| &mut t.songwriter,
                    &mut sheet,
                    &mut track,
                    rest,
                    line,
                ),
                "TITLE" => apply_text(
                    |s| &mut s.title,
                    |t| &mut t.title,
                    &mut sheet,
                    &mut track,
                    rest,
                    line,
                ),
                "FILE" => {
                    // Flush any open track into the file that came before it.
                    push_track(&mut files, &mut track);
                    let (path, file_type) = parse_file_line(rest, line)?;
                    files.push(CueFile {
                        path,
                        file_type,
                        tracks: Vec::new(),
                    });
                }
                "TRACK" => {
                    if files.is_empty() {
                        return Err(CueError::TrackBeforeFile { line });
                    }
                    push_track(&mut files, &mut track);
                    let mut parts = rest.split_whitespace();
                    let number = parts
                        .next()
                        .ok_or(CueError::TrackBeforeFile { line })
                        .and_then(|n| parse_number(n, line, 1, 99, Field::Track))?;
                    let mode = match parts.next() {
                        Some("AUDIO") => TrackMode::Audio,
                        Some(m) if m.starts_with("MODE1/") => {
                            TrackMode::Mode1(parse_suffix(m, line)?)
                        }
                        Some(m) if m.starts_with("MODE2/") => {
                            TrackMode::Mode2(parse_suffix(m, line)?)
                        }
                        Some(other) => TrackMode::Other(unquote(other)),
                        None => TrackMode::Audio,
                    };
                    track = Some(Track {
                        number: u8::try_from(number).unwrap_or(99),
                        mode,
                        title: None,
                        performer: None,
                        songwriter: None,
                        indices: Vec::new(),
                        gaps: Vec::new(),
                    });
                }
                "INDEX" => {
                    let Some(t) = track.as_mut() else {
                        return Err(CueError::IndexBeforeTrack { line });
                    };
                    let mut parts = rest.split_whitespace();
                    let number = parts
                        .next()
                        .ok_or(CueError::IndexBeforeTrack { line })
                        .and_then(|n| parse_number(n, line, 0, 99, Field::Index))?;
                    let time = parts.next().ok_or(CueError::BadTime {
                        line,
                        text: rest.to_string(),
                    })?;
                    let ms = parse_time(time, line)?;
                    t.indices.push(Index {
                        number: u8::try_from(number).unwrap_or(99),
                        ms,
                    });
                }
                "PREGAP" | "POSTGAP" => {
                    let Some(t) = track.as_mut() else {
                        return Err(CueError::IndexBeforeTrack { line });
                    };
                    let ms = parse_time(rest.trim(), line)?;
                    t.gaps.push((
                        if command == "PREGAP" {
                            GapKind::Pre
                        } else {
                            GapKind::Post
                        },
                        ms,
                    ));
                }
                "CATALOG" | "CDTEXTFILE" | "FLAGS" | "ISRC" => {
                    // Recorded in the estate's interest as *not* modelled: a caller that
                    // needs a catalog number is a different caller, and inventing fields
                    // for the four commands this parser does not need is how a parser
                    // starts implying it understands more than it does.
                }
                other => {
                    return Err(CueError::UnknownCommand {
                        line,
                        command: other.to_string(),
                    });
                }
            }
        }
        push_track(&mut files, &mut track);
        sheet.files = files;
        Ok(sheet)
    }

    /// Render the sheet back to text.
    ///
    /// The output parses back to the same sheet — asserted by tests, and by property
    /// against generated titles — which is what makes a read/modify/write cycle safe: a
    /// caller that edits one title gets a sheet whose every other line is what it read.
    ///
    /// # Errors
    ///
    /// [`CueError::NewlineInValue`] for a CD-TEXT value containing a line break, which
    /// the format cannot express: a newline would end the line mid-value, and the sheet
    /// written around it would not parse. Found by a property test generating a title
    /// with an embedded newline, which is the sort of thing only generation finds — no
    /// hand-written fixture thinks to try it.
    pub fn to_text(&self) -> Result<String> {
        // `filter_map` rather than `flatten`: a tuple is not an iterator, and the
        // two-field-per-array shape wants the Nones dropped, not iterated.
        let album_fields = [
            ("PERFORMER", self.performer.as_ref()),
            ("TITLE", self.title.as_ref()),
            ("SONGWRITER", self.songwriter.as_ref()),
        ];
        for (field, value) in album_fields
            .into_iter()
            .filter_map(|(f, v)| v.map(|v| (f, v)))
        {
            check_writable(field, value)?;
        }
        for (key, value) in &self.remarks {
            check_writable("REM key", key)?;
            check_writable("REM value", value)?;
        }
        for file in &self.files {
            check_writable("FILE path", &file.path)?;
            for track in &file.tracks {
                let track_fields = [
                    ("TITLE", track.title.as_ref()),
                    ("PERFORMER", track.performer.as_ref()),
                    ("SONGWRITER", track.songwriter.as_ref()),
                ];
                for (field, value) in track_fields
                    .into_iter()
                    .filter_map(|(f, v)| v.map(|v| (f, v)))
                {
                    check_writable(field, value)?;
                }
                if let TrackMode::Other(mode) = &track.mode {
                    check_writable("TRACK mode", mode)?;
                }
            }
        }
        Ok(self.write_to_string())
    }

    /// The rendering, for a sheet already known to be writable.
    fn write_to_string(&self) -> String {
        let mut out = String::new();
        if let Some(p) = &self.performer {
            out.push_str("PERFORMER ");
            out.push_str(&quote(p));
            out.push('\n');
        }
        if let Some(t) = &self.title {
            out.push_str("TITLE ");
            out.push_str(&quote(t));
            out.push('\n');
        }
        if let Some(s) = &self.songwriter {
            out.push_str("SONGWRITER ");
            out.push_str(&quote(s));
            out.push('\n');
        }
        for (key, value) in &self.remarks {
            out.push_str("REM ");
            out.push_str(&quote(key));
            out.push(' ');
            out.push_str(&quote(value));
            out.push('\n');
        }
        for file in &self.files {
            out.push_str("FILE ");
            out.push_str(&quote(&file.path));
            out.push(' ');
            out.push_str(&file.file_type.to_string());
            out.push('\n');
            for track in &file.tracks {
                out.push_str("  TRACK ");
                out.push_str(&alloc::format!("{:02}", track.number));
                out.push(' ');
                out.push_str(&match &track.mode {
                    TrackMode::Audio => "AUDIO".to_string(),
                    TrackMode::Mode1(n) => alloc::format!("MODE1/{n}"),
                    TrackMode::Mode2(n) => alloc::format!("MODE2/{n}"),
                    TrackMode::Other(s) => quote(s),
                });
                out.push('\n');
                if let Some(t) = &track.title {
                    out.push_str("    TITLE ");
                    out.push_str(&quote(t));
                    out.push('\n');
                }
                if let Some(p) = &track.performer {
                    out.push_str("    PERFORMER ");
                    out.push_str(&quote(p));
                    out.push('\n');
                }
                if let Some(s) = &track.songwriter {
                    out.push_str("    SONGWRITER ");
                    out.push_str(&quote(s));
                    out.push('\n');
                }
                for (kind, ms) in &track.gaps {
                    let name = if *kind == GapKind::Pre {
                        "PREGAP"
                    } else {
                        "POSTGAP"
                    };
                    out.push_str("    ");
                    out.push_str(name);
                    out.push(' ');
                    out.push_str(&crate::time::format_time(*ms));
                    out.push('\n');
                }
                for index in &track.indices {
                    out.push_str("    INDEX ");
                    out.push_str(&alloc::format!("{:02}", index.number));
                    out.push(' ');
                    out.push_str(&crate::time::format_time(index.ms));
                    out.push('\n');
                }
            }
        }
        out
    }

    /// Every chapter start across every file, book-relative, given each file's duration.
    ///
    /// A sheet's times are file-relative; a book's chapters are not. The durations come
    /// from the audio itself, so this is the caller's to supply — the sheet cannot know
    /// how long its files are, and a parser that guessed would be a parser that guessed.
    ///
    /// `durations` pairs with [`Sheet::files`] by position; a file with no duration
    /// contributes its tracks at file-relative times rather than being skipped, because
    /// a wrong-but-labelled time beats a missing one. Files with no tracks contribute
    /// nothing.
    #[must_use]
    pub fn chapter_starts_book_relative(
        &self,
        durations: &[Option<u64>],
    ) -> Vec<(u64, Option<String>)> {
        let mut out = Vec::new();
        let mut offset = 0u64;
        for (i, file) in self.files.iter().enumerate() {
            for (ms, title) in file.chapter_starts() {
                out.push((offset.saturating_add(ms), title));
            }
            offset = offset.saturating_add(durations.get(i).copied().flatten().unwrap_or(0));
        }
        out
    }
}

/// Apply a CD-TEXT line at whichever level is open.
fn apply_text(
    sheet_field: fn(&mut Sheet) -> &mut Option<String>,
    track_field: fn(&mut Track) -> &mut Option<String>,
    sheet: &mut Sheet,
    track: &mut Option<Track>,
    rest: &str,
    _line: usize,
) {
    let value = unquote(rest.trim());
    if let Some(t) = track {
        *track_field(t) = Some(value);
    } else {
        *sheet_field(sheet) = Some(value);
    }
}

/// Move an open track into the file that came before it.
fn push_track(files: &mut [CueFile], track: &mut Option<Track>) {
    if let Some(t) = track.take() {
        if let Some(f) = files.last_mut() {
            f.tracks.push(t);
        }
    }
}

/// Split a line into its command and the rest.
fn split_command(line: &str) -> (&str, &str) {
    match line.find(char::is_whitespace) {
        Some(at) => (&line[..at], line[at..].trim_start()),
        None => (line, ""),
    }
}

/// Parse a number in a range, with the line it came from.
fn parse_number(text: &str, line: usize, low: u32, high: u32, field: Field) -> Result<u32> {
    let n: u32 = text.parse().map_err(|_| CueError::BadNumber {
        line,
        text: text.to_string(),
    })?;
    if n < low || n > high {
        return Err(field.out_of_range(line, n));
    }
    Ok(n)
}

/// Which numbered field a range check is about, so the error names the field rather
/// than guessing from the value's text — a heuristic that reads the value for a clue
/// is how `TRACK 100` ends up reporting itself as an index number.
#[derive(Debug, Clone, Copy)]
enum Field {
    /// A `TRACK` number.
    Track,
    /// An `INDEX` number.
    Index,
}

impl Field {
    fn out_of_range(self, line: usize, number: u32) -> CueError {
        match self {
            Field::Track => CueError::TrackNumberOutOfRange { line, number },
            Field::Index => CueError::IndexNumberOutOfRange { line, number },
        }
    }
}

/// The byte count after `MODE1/`.
fn parse_suffix(mode: &str, line: usize) -> Result<u16> {
    let suffix = mode.split('/').nth(1).unwrap_or("");
    suffix.parse().map_err(|_| CueError::BadNumber {
        line,
        text: mode.to_string(),
    })
}

/// Parse `FILE "path" TYPE`.
fn parse_file_line(rest: &str, line: usize) -> Result<(String, FileType)> {
    let rest = rest.trim_start();
    if !rest.starts_with('"') {
        // Unquoted paths exist in the wild, and the type is still the last word.
        let mut parts = rest.rsplit_once(char::is_whitespace);
        if let Some((path, kind)) = parts.take() {
            return Ok((path.trim().to_string(), parse_file_type(kind)));
        }
        return Ok((rest.to_string(), FileType::Wave));
    }
    let _ = line;
    let closed = rest[1..]
        .find('"')
        .map(|at| at + 1)
        .ok_or(CueError::UnterminatedQuote { line })?;
    // `closed` is the index of the closing quote itself, so the path is the slice up to
    // it and the type is whatever follows it.
    let path = rest[1..closed].to_string();
    let kind = rest[closed + 1..].trim();
    Ok((path, parse_file_type(kind)))
}

fn parse_file_type(kind: &str) -> FileType {
    match kind {
        "WAVE" => FileType::Wave,
        "MP3" => FileType::Mp3,
        "AIFF" => FileType::Aiff,
        "BINARY" => FileType::Binary,
        other => FileType::Other(other.to_string()),
    }
}

/// Remove one layer of double quotes, resolving the `\"` escape.
fn unquote(text: &str) -> String {
    let text = text.trim();
    if let Some(stripped) = text.strip_prefix('"') {
        if let Some(stripped) = stripped.strip_suffix('"') {
            return stripped.replace("\\\"", "\"");
        }
    }
    text.replace("\\\"", "\"")
}

/// Add the quoting a written line needs.
fn quote(text: &str) -> String {
    alloc::format!("\"{}\"", text.replace('"', "\\\""))
}

/// A value the format cannot express as text is refused, not mangled.
fn check_writable(field: &'static str, value: &str) -> Result<()> {
    if value.contains(['\n', '\r']) {
        return Err(CueError::NewlineInValue {
            field,
            value: value.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )]

    use super::*;

    /// A sheet in the shape EAC writes for a spoken-word rip: pregap pairs, CD-TEXT,
    /// and an MP3 file.
    const EAC_SHAPE: &str = r#"REM COMMENT "ExactAudioCopy v1.6"
REM DATE 1994
PERFORMER "Terry Pratchett"
TITLE "Guards! Guards!"
FILE "book.mp3" MP3
  TRACK 01 AUDIO
    TITLE "A Night Watchman"
    PERFORMER "Terry Pratchett"
    INDEX 00 00:00:00
    INDEX 01 02:00:00
  TRACK 02 AUDIO
    TITLE "The Librarian's Opposable Thumbs"
    INDEX 00 37:00:00
    INDEX 01 37:01:30
  TRACK 03 AUDIO
    TITLE "Fire"
    INDEX 01 74:30:00
"#;

    #[test]
    fn an_eac_sheet_parses_to_its_chapter_starts() {
        let sheet = Sheet::parse(EAC_SHAPE).expect("parses");
        assert_eq!(sheet.title.as_deref(), Some("Guards! Guards!"));
        assert_eq!(sheet.performer.as_deref(), Some("Terry Pratchett"));
        assert_eq!(sheet.files.len(), 1);
        assert_eq!(sheet.files[0].path, "book.mp3");
        assert_eq!(sheet.files[0].file_type, FileType::Mp3);

        let chapters = sheet.files[0].chapter_starts();
        assert_eq!(chapters.len(), 3, "three tracks, three chapters");
        // 2:00, 37:01.5, 74:30 — in milliseconds, each from its INDEX 01 and not its
        // pregap. A reader that took INDEX 00 puts a chapter of silence before every
        // real one.
        assert_eq!(chapters[0], (120_000, Some("A Night Watchman".to_string())));
        // 37:01:30 is thirty-seven minutes, one second, thirty frames - and thirty
        // frames are 400 ms, not half a second. The frames field is CD sectors at 75 to
        // the second, and misreading it as hundredths is the classic cue-sheet defect.
        assert_eq!(chapters[1].0, 37 * 60_000 + 1_000 + 400);
        assert_eq!(chapters[2].0, 74 * 60_000 + 30_000);
    }

    #[test]
    fn a_pregap_is_not_a_chapter() {
        // Track 02's INDEX 00 is at 37:00:00 and its INDEX 01 thirty seconds later. The
        // chapter is where the *audio* starts; the gap belongs to the end of the track
        // before it, and a chapter boundary in the middle of silence is how skip-forward
        // lands you in nothing.
        let sheet = Sheet::parse(EAC_SHAPE).expect("parses");
        let t2 = &sheet.files[0].tracks[1];
        assert_eq!(t2.index_zero(), Some(37 * 60_000));
        assert_eq!(t2.index_one(), Some(37 * 60_000 + 1_000 + 400));
        let chapters = sheet.files[0].chapter_starts();
        assert_eq!(chapters[1].0, 37 * 60_000 + 1_000 + 400);
    }

    #[test]
    fn remarks_are_kept_but_not_invented_into_fields() {
        // `REM DATE 1994` is real information; a parser that models four REM keys implies
        // the other four hundred. Kept as pairs for a caller that wants them.
        let sheet = Sheet::parse(EAC_SHAPE).expect("parses");
        assert_eq!(sheet.remarks.len(), 2);
        assert_eq!(sheet.remarks[0].0, "COMMENT");
        assert_eq!(sheet.remarks[0].1, "ExactAudioCopy v1.6");
        assert_eq!(sheet.remarks[1], ("DATE".to_string(), "1994".to_string()));
    }

    #[test]
    fn a_multi_file_sheet_keeps_its_times_file_relative() {
        // One file per disc is the common shape for a long book, and each file's times
        // start over. A parser that treated the second file's 00:00:00 as the book's
        // 00:00:00 would stack every disc on top of the first.
        let sheet = Sheet::parse(
            r#"FILE "disc1.mp3" MP3
  TRACK 01 AUDIO
    TITLE "One"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Two"
    INDEX 01 30:00:00
FILE "disc2.mp3" MP3
  TRACK 03 AUDIO
    TITLE "Three"
    INDEX 01 10:00:00
"#,
        )
        .expect("parses");
        assert_eq!(sheet.files.len(), 2);
        assert_eq!(sheet.files[0].tracks.len(), 2);
        assert_eq!(sheet.files[1].tracks.len(), 1);
        // Book-relative requires the durations, supplied by the caller.
        let book = sheet.chapter_starts_book_relative(&[Some(30 * 60_000), Some(35 * 60_000)]);
        assert_eq!(book.len(), 3);
        assert_eq!(
            book[2],
            (30 * 60_000 + 10 * 60_000, Some("Three".to_string()))
        );
        // And without durations the times are file-relative, still labelled.
        let raw = sheet.chapter_starts_book_relative(&[None, None]);
        assert_eq!(raw[2].0, 10 * 60_000);
    }

    #[test]
    fn a_track_before_any_file_is_refused() {
        let err = Sheet::parse("  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n").unwrap_err();
        assert!(
            matches!(err, CueError::TrackBeforeFile { line: 1 }),
            "{err:?}"
        );
    }

    #[test]
    fn an_index_before_any_track_is_refused() {
        let err = Sheet::parse("FILE \"a.mp3\" MP3\n  INDEX 01 00:00:00\n").unwrap_err();
        assert!(
            matches!(err, CueError::IndexBeforeTrack { line: 2 }),
            "{err:?}"
        );
    }

    #[test]
    fn an_unknown_command_is_refused_rather_than_skipped() {
        // A sheet is a list of times. Skipping an unrecognised line that carried one is
        // how a book loses a chapter quietly, which is the failure this whole estate
        // exists to make loud.
        let err = Sheet::parse("FILE \"a.mp3\" MP3\n  TOTALLY_NOT_A_COMMAND x\n").unwrap_err();
        assert_eq!(
            err,
            CueError::UnknownCommand {
                line: 2,
                command: "TOTALLY_NOT_A_COMMAND".to_string()
            }
        );
    }

    #[test]
    fn a_track_number_out_of_range_is_refused() {
        // TRACK 00 is a lead-in convention, not a track, and 100 does not exist on a disc.
        let err = Sheet::parse("FILE \"a.mp3\" MP3\n  TRACK 100 AUDIO\n").unwrap_err();
        assert!(matches!(
            err,
            CueError::TrackNumberOutOfRange {
                line: 2,
                number: 100
            }
        ));
    }

    #[test]
    fn quoting_survives_a_round_trip() {
        // A title containing a double quote is the case the escape exists for, and a
        // reader that mishandles it produces a title that ends early.
        let sheet = Sheet::parse(
            "FILE \"a.mp3\" MP3\n  TRACK 01 AUDIO\n    TITLE \"The \\\"Best\\\" Chapter\"\n    INDEX 01 00:00:00\n",
        )
        .expect("parses");
        assert_eq!(
            sheet.files[0].tracks[0].title.as_deref(),
            Some("The \"Best\" Chapter")
        );
        let back = Sheet::parse(&sheet.to_text().expect("writable")).expect("parses");
        assert_eq!(back, sheet);
    }

    #[test]
    fn a_written_sheet_parses_back_to_itself() {
        let sheet = Sheet::parse(EAC_SHAPE).expect("parses");
        let text = sheet.to_text().expect("writable");
        let back = Sheet::parse(&text).expect("the written sheet parses");
        assert_eq!(back, sheet, "a read/modify/write cycle must be lossless");
        // And the written text is sheet-shaped: commands, indentation, quoted strings.
        assert!(text.contains("FILE \"book.mp3\" MP3"));
        assert!(text.contains("  TRACK 01 AUDIO"));
        assert!(text.contains("    INDEX 01 2:00:00"), "{text}");
    }

    #[test]
    fn a_track_with_no_index_one_is_skipped_not_placed_at_zero() {
        // A track whose start is unknown cannot be placed. Contributing it at time zero
        // would build a chapter list that looks plausible and is wrong, which is the
        // worst shape a list can take.
        let sheet = Sheet::parse(
            "FILE \"a.mp3\" MP3\n  TRACK 01 AUDIO\n    TITLE \"No start\"\n  TRACK 02 AUDIO\n    INDEX 01 05:00:00\n",
        )
        .expect("parses");
        let chapters = sheet.files[0].chapter_starts();
        assert_eq!(chapters.len(), 1, "the placeless track is skipped");
        assert_eq!(chapters[0].0, 5 * 60_000);
    }

    #[test]
    fn modes_are_recorded_not_interpreted() {
        let sheet = Sheet::parse(
            "FILE \"a.bin\" BINARY\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  TRACK 02 MODE1/2352\n    INDEX 01 00:01:00\n  TRACK 03 MODE2/2336\n    INDEX 01 00:02:00\n",
        )
        .expect("parses");
        let tracks = &sheet.files[0].tracks;
        assert_eq!(tracks[0].mode, TrackMode::Audio);
        assert_eq!(tracks[1].mode, TrackMode::Mode1(2352));
        assert_eq!(tracks[2].mode, TrackMode::Mode2(2336));
    }

    #[test]
    fn gaps_are_recorded_in_milliseconds() {
        // PREGAP/POSTGAP declare silence with the same MM:SS:FF clock.
        let sheet = Sheet::parse(
            "FILE \"a.mp3\" MP3\n  TRACK 01 AUDIO\n    PREGAP 00:01:00\n    INDEX 01 00:02:00\n    POSTGAP 00:00:37\n",
        )
        .expect("parses");
        let t = &sheet.files[0].tracks[0];
        assert_eq!(t.gaps[0], (GapKind::Pre, 1_000));
        // 37 frames is 493.3... ms, rounded to 493: the rounding is part of what the
        // format is, and asserting the exact number pins it.
        assert_eq!(t.gaps[1], (GapKind::Post, 493));
    }

    #[test]
    fn an_unterminated_quote_is_refused_with_its_line() {
        let err = Sheet::parse("FILE \"a.mp3 MP3\n").unwrap_err();
        assert!(
            matches!(err, CueError::UnterminatedQuote { line: 1 }),
            "{err:?}"
        );
    }

    #[test]
    fn an_unquoted_path_is_accepted_because_the_wild_writes_them() {
        let sheet = Sheet::parse("FILE a.mp3 MP3\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n")
            .expect("parses");
        assert_eq!(sheet.files[0].path, "a.mp3");
    }

    #[test]
    fn a_value_with_a_line_break_is_refused_not_mangled() {
        // The format has no escape for a newline: writing one would end the line
        // mid-value and produce a sheet that does not parse. Replacing it with a space
        // would "work" and would silently change a title the user is looking for.
        let mut sheet =
            Sheet::parse("FILE \"a.mp3\" MP3\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n")
                .expect("parses");
        sheet.title = Some("Two\nLines".to_string());
        let err = sheet.to_text().unwrap_err();
        assert!(
            matches!(err, CueError::NewlineInValue { field: "TITLE", .. }),
            "{err:?}"
        );
    }
}
