# Changelog

All notable changes to this project are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-10-09

### Added

- **`Sheet::parse` / `Sheet::to_text`**; the CDRWIN cue sheet: `FILE` / `TRACK` /
  `INDEX`, CD-TEXT (`TITLE`, `PERFORMER`, `SONGWRITER`) at album and track level, `REM`
  pairs kept for callers that want them, `PREGAP`/`POSTGAP`, and multiple `FILE` entries
  (one per disc is the common shape for a long book).
- **`MM:SS:FF`**; the format's whole timekeeping, at 75 frames to the second. Minutes do
  not wrap at sixty; an over-wide seconds field is normalised because it is unambiguous;
  an over-wide frames field is refused because 75 frames *is* a second and a larger value
  changes the time rather than spelling it differently. Conversions round to nearest, so
  a book's later chapters do not drift late the way truncation drifts.
- **`CueFile::chapter_starts`**; `INDEX 01` only. `INDEX 00` is the pregap, and a
  reader that made gaps into chapters puts a chapter of silence between every pair of
  real ones. A track with no `INDEX 01` is skipped rather than placed at time zero.
- **`Sheet::chapter_starts_book_relative`**; file-relative times made book-relative
  from caller-supplied durations, because the sheet cannot know how long its audio is.

### Refused, deliberately

- **Unknown commands** are errors with their line number, not skipped lines: a sheet is a
  list of times, and skipping an unrecognised line that carried one is how a book loses a
  chapter quietly.
- **A CD-TEXT value containing a line break** cannot be written; the format has no
  escape for one; and is refused with the value rather than mangled to a space. Found by
  a property test generating a title with an embedded newline, which is the sort of thing
  only generation finds.

### Verification honesty

- No independent cue-sheet implementation exists on the machine this crate was written
  on: ffmpeg's cue demuxer is not compiled in, libcue is absent, and there is no package
  manager to fetch one. The *grammar* is therefore exercised by this crate's tests and
  property tests alone. The *arithmetic that matters* is checked end to end by the
  estate: a sheet's `INDEX 01` times pushed into an M4B come back out of ffprobe as the
  same milliseconds, and that gate lives in `audiobook-shelf`.
- Four of this crate's own tests were written wrong before they were written right -
  three misread `MM:SS:FF` (37:01:30 is 37 min + 1 s + 30 frames, not +1.5 s; 00:10:00 is
  ten seconds, not ten minutes) and one formatted a second as a minute. A format whose
  time fields invite that confusion is a format whose tests need the arithmetic pinned in
  comments, which they now are.
