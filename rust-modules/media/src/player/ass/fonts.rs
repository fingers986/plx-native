//! The faces every subtitle renderer starts with, and the order it adds them (plan D6).
//!
//! libass here runs with no font provider: it finds a face by FAMILY NAME among the faces handed
//! to `plx_ass_add_font`, and anything those faces lack falls to the default path (the CJK file,
//! with no coverage check). So Thai, Devanagari, Arabic and the other scripts of the account
//! language picker are reachable only because a face named for them is in the library:
//!
//!   * `appfont.ttf`, `appfont-bold.ttf` - Inter.
//!   * `subtitle-open-sans-*.ttf` - Open Sans Regular/Bold; italic is synthesized by libass.
//!   * `subtitle-sf-pro-text-*.otf` - SF Pro Text Regular/Bold/Italic/Bold Italic.
//!   * `fonts/subs/NotoSans<Script>-Bold.ttf` - one per script, cut by `tools/cut-noto-subs.py`,
//!     listed in `fonts/subs/manifest.json`. Each answers to its own name and the selected subtitle
//!     families (Inter and Open Sans), and carries no Latin, digit or punctuation codepoint Inter
//!     Bold has. SF Pro Text falls back through Inter's family when a glyph is missing.
//!
//! **The order is load-bearing.** libass breaks a tie between equally good members of a family by
//! the order they were added, and takes the first member for code 0 without a coverage check, so
//! the complete Inter, Open Sans and SF Pro Text families go in before every Noto face. The Noto
//! faces follow in manifest order.
//!
//! **Read at create.** The files are read from disk each time a renderer is created, handed to
//! libass (`ass_add_font` copies the bytes) and dropped. Nothing here is a process-lifetime copy:
//! the resident cost is libass's one copy per live renderer.

use super::Fault;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Inter Regular, then Inter Bold.
const INTER: [&str; 2] = ["appfont.ttf", "appfont-bold.ttf"];
/// Open Sans follows Inter's two-face scheme; libass synthesizes italic when a cue requests it.
const OPEN_SANS: [&str; 2] = [
    "subtitle-open-sans-regular.ttf",
    "subtitle-open-sans-bold.ttf",
];
/// SF Pro Text's four static faces let libass match bold and italic tags to real designs.
const SF_PRO_TEXT: [&str; 4] = [
    "subtitle-sf-pro-text-regular.otf",
    "subtitle-sf-pro-text-bold.otf",
    "subtitle-sf-pro-text-regular-italic.otf",
    "subtitle-sf-pro-text-bold-italic.otf",
];
/// Where the Noto faces and their manifest sit, under the app directory.
const SUBS_DIR: &str = "fonts/subs";
const MANIFEST: &str = "manifest.json";

#[derive(Deserialize)]
struct Manifest {
    faces: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    file: String,
}

/// Every bundled face, in the order the renderer adds it. A missing or unreadable manifest is
/// `FontsUnavailable`, like a missing Inter: a package without them is broken, and subtitles that
/// silently lose a script are worse than a subtitle error that says why.
pub(super) fn bundled(asset_dir: &Path) -> Result<Vec<PathBuf>, Fault> {
    let dir = asset_dir.join(SUBS_DIR);
    let manifest: Manifest = std::fs::read(dir.join(MANIFEST))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or(Fault::FontsUnavailable)?;
    if manifest.faces.is_empty() {
        return Err(Fault::FontsUnavailable);
    }
    let mut files: Vec<PathBuf> = INTER.iter().chain(OPEN_SANS.iter()).chain(SF_PRO_TEXT.iter())
        .map(|name| asset_dir.join(name)).collect();
    for entry in manifest.faces {
        // A bare file name: the manifest is payload, but a path out of the directory is never right.
        let plain = !entry.file.is_empty()
            && entry.file.ends_with(".ttf")
            && !entry.file.contains(['/', '\\', '\0'])
            && !entry.file.starts_with('.');
        if !plain {
            return Err(Fault::FontsUnavailable);
        }
        files.push(dir.join(entry.file));
    }
    Ok(files)
}

#[cfg(test)]
mod tests;
