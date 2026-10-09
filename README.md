# cuesheet-core

CDRWIN cue sheet reader and writer — the estate's L0 layer for the format a CD rip uses
to say where its tracks are. `#![no_std]` + `alloc`, one runtime dependency
(`thiserror`), the same shape as every other L0 kit (`mp4-core`, `id3-core`).

```rust
use cuesheet_core::Sheet;

let sheet = Sheet::parse(r#"
    FILE "book.mp3" MP3
      TRACK 01 AUDIO
        TITLE "Chapter One"
        INDEX 00 00:00:00
        INDEX 01 00:02:00
      TRACK 02 AUDIO
        TITLE "Chapter Two"
        INDEX 01 37:01:30
"#)?;

// INDEX 01 only: the pregap is silence, and a chapter of silence between every pair of
// real chapters is how skip-forward lands you in nothing.
let chapters = sheet.files[0].chapter_starts();
assert_eq!(chapters[0].0, 120_000);
assert_eq!(chapters[1].0, 37 * 60_000 + 1_000 + 400);
# Ok::<(), cuesheet_core::CueError>(())
```

## Layers

This crate is **L0**: no dependencies above it, one `no_std`-capable dependency, and
`deny(unsafe_code)` / `deny(missing_docs)` throughout.
