//! The **Watch Intro / Watch Recap / Skip Credits** control. While the playhead sits inside one of the playing
//! leaf's server markers (`metadata::playing_markers`, fetched with `?includeMarkers=1`) or a
//! specifically named recap chapter, this button TAKES THE PLACE of the transport's Subtitles/Audio discs — same row, same right edge,
//! same height — and the discs return the moment the segment ends.
//!
//! Replacing them rather than joining them is the design call: one wide unmissable target in the
//! spot the eye already goes for transport controls. The cost is that CC/Audio are covered for the
//! length of the segment, so BACK over the pill DISMISSES the offer for that segment and puts the
//! discs back (`PlayerScreen::dismiss_offer` asks `PlayerReq::DismissOffer`, which marks the
//! segment skipped in the metadata store — the same set a Skip press uses) — the row stays at five
//! controls and the viewer never has to wait an intro out to change tracks. The button only exists
//! while the HUD is up, which is why `app.rs` RAISES the HUD once when a segment begins and parks
//! focus here. Intro and Recap automatically skip after five seconds; interaction cancels the timer
//! and changes the label to the corresponding Skip action for that marker.
#![allow(dead_code)]
use plx_data::metadata::{self, MarkerKind};
use plx_ui::theme;
use plx_ui::widgets::{Button, ControlGround};
use plx_ui::{Env, Painter, Rect, View};
use std::ffi::CString;

pub const RECAP_COUNTDOWN_MS: u32 = 5_000;
pub const INTRO_COUNTDOWN_MS: u32 = 5_000;

/// What pressing the button does. The distinction is the `final` flag on a credits marker: an
/// ordinary segment is a seek and playback continues past it, but a `final` one runs to the end of
/// the item, so "skip" means **the episode is over** — seeking to its end would race the decoder
/// against its own last frames for no benefit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SkipAction {
    /// jump to this position (ns) and keep playing
    Seek(i64),
    /// the item is finished — hand off to the end-of-playback path (next episode, or leave)
    Finish,
}

/// Automatic skip timer for one marker kind. Cancelling latches until a different marker or
/// playback session, even when seeking temporarily moves the playhead outside the marker.
#[derive(Default)]
pub struct MarkerCountdown {
    deadline: u32,
    cancelled: bool,
    marker: Option<metadata::Marker>,
}

impl MarkerCountdown {
    pub fn tick(
        &mut self,
        slot: crate::player_hud::ControlSlot,
        kind: MarkerKind,
        duration_ms: u32,
        now: u32,
    ) {
        let marker = match slot {
            crate::player_hud::ControlSlot::Skip(p) if p.kind == kind => Some(p.marker),
            _ => None,
        };
        if let Some(marker) = marker {
            if self.marker != Some(marker) {
                self.deadline = 0;
                self.cancelled = false;
                self.marker = Some(marker);
            }
            if self.deadline == 0 && !self.cancelled {
                self.deadline = now.wrapping_add(duration_ms).max(1);
            }
        } else {
            // Seeking away stops the visible timer, but keeps a declined countdown latched to
            // its marker so returning to that same segment cannot start it again.
            self.deadline = 0;
        }
    }

    pub fn armed(&self) -> bool {
        self.deadline != 0
    }

    pub fn cancelled(&self) -> bool {
        self.cancelled
    }

    pub fn cancel(&mut self) {
        self.deadline = 0;
        self.cancelled = true;
    }

    /// Cancel and remember the offered marker even if its first countdown tick has not run yet.
    pub fn cancel_offer(&mut self, slot: crate::player_hud::ControlSlot, kind: MarkerKind) -> bool {
        let crate::player_hud::ControlSlot::Skip(prompt) = slot else {
            return false;
        };
        if prompt.kind != kind {
            return false;
        }
        self.deadline = 0;
        self.cancelled = true;
        self.marker = Some(prompt.marker);
        true
    }

    pub fn reset(&mut self) {
        self.deadline = 0;
        self.cancelled = false;
        self.marker = None;
    }

    pub fn expired(&self, now: u32) -> bool {
        if self.deadline == 0 {
            return false;
        }
        let left = self.deadline.wrapping_sub(now);
        left == 0 || left >= 0x8000_0000
    }

    pub fn progress(&self, now: u32, duration_ms: u32) -> f32 {
        if self.deadline == 0 {
            return 0.0;
        }
        let left = self.deadline.wrapping_sub(now);
        let remaining = if left >= 0x8000_0000 { 0 } else { left };
        1.0 - (remaining as f32 / duration_ms as f32).clamp(0.0, 1.0)
    }
}

/// The offer the button is currently making. Carries the marker's TYPED kind, not its label:
/// asking "is the playhead in a credits segment?" by string-comparing user-facing copy made a pure
/// copy-edit — the sort of thing a design pass does — silently disable Up Next and auto-advance,
/// with no compile error and nothing to fail.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Prompt {
    /// the segment itself — carried so activating the button can retire it (`metadata::mark_skipped`)
    pub marker: metadata::Marker,
    pub kind: MarkerKind,
    pub action: SkipAction,
}

impl Prompt {
    /// The button's copy — presentation, derived from the kind at the point of drawing.
    pub fn label(&self) -> &'static str {
        match self.kind {
            MarkerKind::Intro => plx_platform::i18n::msg::widgets_skip_intro(),
            MarkerKind::Credits => plx_platform::i18n::msg::widgets_skip_credits(),
            MarkerKind::Recap => plx_platform::i18n::msg::widgets_skip_recap(),
        }
    }
}

/// PURE: the offer a given segment makes. Takes the marker rather than reading the playhead, so
/// the precedence it feeds ([`crate::player_hud::slot_for`]) is host-testable and the whole frame
/// decides from ONE playhead sample — `playpos_ns` is written by LG's media thread, and re-reading
/// it per call site let the input path and the draw path disagree within a single frame.
///
/// Markers belong to the PLAYING leaf (`metadata::playing()`), never to `metadata::current()`:
/// during a show-page episode play `current()` is the SHOW, and offering episode 1's intro timing
/// during episode 5 is the same identity bug the track store exists to prevent.
pub fn prompt_for(m: metadata::Marker) -> Prompt {
    Prompt {
        marker: m,
        kind: m.kind,
        action: match m.kind {
            // a `final` credits segment runs to the end of the item, so "skip" means the episode is
            // over — seeking to its end would race the decoder against its own last frames
            MarkerKind::Credits if m.final_seg => SkipAction::Finish,
            _ => SkipAction::Seek(m.end_ms * 1_000_000),
        },
    }
}

/// **Draw the button** at a rect the caller already has: the compiled package's pill (`Standin::skip`,
/// one per label width), placed at compile time, in the control row's place.
pub fn draw_at(
    row: &crate::player_hud::motion::HudMotion,
    p: Painter,
    pr: Prompt,
    slot: Rect,
    focused: bool,
    progress: f32,
    countdown_cancelled: bool,
) {
    let label = match (pr.kind, countdown_cancelled) {
        (MarkerKind::Intro, false) => plx_platform::i18n::msg::widgets_skip_intro_watch(),
        (MarkerKind::Intro, true) => plx_platform::i18n::msg::widgets_skip_intro(),
        (MarkerKind::Recap, true) => plx_platform::i18n::msg::widgets_skip_recap_action(),
        _ => pr.label(),
    };
    let Ok(label) = CString::new(label) else {
        return;
    };
    // No leading icon: the label alone carries it, and a chevron on a control that does not
    // navigate anywhere was reading as "more" rather than "skip".
    // Its slot's only item, so index 0 — the pop is the control ROW's (`HudMotion::scale`),
    // shared with the transport discs this pill stands in for.
    let pop = row.scale(0);
    Button::new(label.as_ptr(), theme::size::BODY, slot)
        .scale(pop)
        .focused(focused)
        .progress(progress)
        // It stands in the transport discs' own slot, over the video plane and on the HUD's ramp,
        // so it wears their ground as well as their pop — see `ControlGround`.
        .ground(ControlGround::Unkeyed)
        .draw(&Env::inert(), p);
}
