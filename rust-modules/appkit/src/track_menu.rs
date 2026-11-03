Warning: truncated output (original token count: 98444)
Total output lines: 7527

//! In-player modal track menu: audio + subtitle pickers over the video, rendered on the reusable
//! animated `TableView` (Apple-TV "settings" look — a sliding pill selection, section header with
//! a codec accessory, per-row badges, a leading checkmark on the active track). app.rs routes
//! D-pad/OK/BACK here while the menu is open; LEFT/RIGHT switch between the Audio and Subtitles
//! panels on a root (on the Subtitles tab LEFT pops a sub-page and RIGHT on a Nav row enters it;
//! BACK pops a sub-page before it dismisses). The selection commit (native audio switch / server transcode / burn) is unchanged
//! from the previous procedural version — only the presentation moved onto the table.
//!
//! **The Subtitles panel is grouped, not one flat list** (plan `subtitle-menu-capsule` §3,
//! `/tmp/dsplayer/player.html:1104-1162`): Off, Forced only and every single-track "yours"
//! language sit under one "Subtitles" header; a "yours" language with several tracks gets its own
//! section (header = language, `accessory("N tracks")`), ranked full < SDH < forced < commentary;
//! everything else is
//! ONE "Other languages" row (a drill-in to [`TrackPage::OtherLanguages`], one row per language
//! A-Z; a multi-track language there drills in again to [`TrackPage::Language`], its tracks
//! ranked); and a headerless section holds Timing and Style. That
//! grouping is a pure DATA model, `metadata::sub_layout` (host-tested without a
//! `PlaybackSession`/`MetadataView` fixture); this module only turns it into `TableView` sections
//! ([`table_form`]) and answers focus/OK by the focused row's [`TrackRow`] identity. "Yours" is
//! the pref language (if the play resolved under one), the playing audio's language, and the
//! current subtitle's own language, in that order (`route::cur_sub_pref_lang`, carried in by
//! `screens::player::overlay`).
//!
//! **Both tabs are keyed declarative forms** (`ui::form`, `docs/player-submenus.md`): each row is
//! declared once with its semantic [`TrackRow`] identity, a stable [`RowKey`] (a family base plus
//! the track's index in the playing item's list, which is fixed for the item's lifetime, never the
//! list position), its kind and its [`Row`]. The focus layer's element is the row's key, so a row
//! added above another (a subtitle offered mid-play, the enhancement pair returning) moves no key,
//! and a rebuild restores the viewer's row by id.
//!
//! **Timing** is a single value row that reads out the current offset (no chevron: it is a
//! hand-off, not a page); OK on it does not step anything here — it returns
//! [`TrackOk::OpenTiming`], which `screens::player::overlay`'s `activate` turns into a hand-off: it
//! dismisses this panel and presents the Timing capsule overlay (`OverlayKind::Timing`,
//! `appkit::timing_capsule`) in its place. The row is dim and inert while no subtitle is drawn
//! (Off, or Forced only with no forced track to show; OK there neither opens the capsule nor
//! closes the panel), and Timing together with Style is omitted
//! during an ordinary transcode, which burns captions into the picture where no client-side offset
//! or style can reach. That omission now holds only while no subtitle is on: once the server draws
//! the selected subtitle ([`plx_media::route::subtitle_presenter`] says `ServerBurn`: a remux that
//! cannot carry an embedded track, this app's own audio-enhancement burn, a quality-limited
//! re-encode), both rows stay visible — dim, inert, with a one-line reason (`Row::note`) saying
//! why the server is drawing it — rather than being found simply gone. An external sidecar over a
//! remux is drawn by the app (`ClientOverRemux`), so there both rows are live with no reason.
//!
//! **Forced only is a MODE row, not a track** (FO, `docs/playback-settings.md`). It sits right after Off
//! wherever Off is, whether or not the item carries a forced track, and holds exactly one checkmark
//! with the tracks and Off: while the mode holds, the forced track it draws is NOT also checked on
//! its own row, and Off is not checked. Its value is the language of what it draws (else the playing
//! audio's), bounded (`sub_layout::forced_only_readout`) so a long name never clips the row's own
//! words. OK commits [`TrackCommit::SubtitleForcedOnly`]: the forced track for the audio that plays
//! ([`plx_media::route::forced_only_pick`], with the container's track names) or nothing; for audio
//! that names no language it matches in the language the session fell back to
//! ([`plx_media::route::forced_only_lang`]), as the resolve does. OK on the row that is already the
//! mode recommits what is on screen and writes nothing.
//!
//! **A pick is the show's memory, and the menu says when the route cannot honour it.** An explicit
//! pick in either tab commits its descriptor with the track ([`TrackCommit`]'s `memo`), which the
//! app's `commit_track` hands to `route::commit_audio_pick`, and for the two subtitle arms to
//! `route::commit_subtitle_and_sidecar` / `commit_forced_only_and_sidecar` (the pick commit plus its
//! sidecar select or deselect); the `*_pick` commits write it to
//! the show's track memory when the route accepted the pick, and the next episode's plan reads it
//! back. When the memory chose a subtitle for this play and the route can show none of it (a copy
//! remux the app cannot draw), Off is on screen, so the first group closes with one inert
//! reason ([`table_form`]'s `memory_note`, `route::cur_memory_note`), absent everywhere else.
//!
//! **Style is a drill-in, and the Subtitles tab is a page stack** (`docs/player-submenus.md`). The
//! Style row is a [`RowKind::Nav`] onto [`TrackPage::Style`], whose Size, Position and Color rows
//! each read out their current value and push a picker page ([`TrackPage::Picker`]) of
//! [`RowKind::Choice`] rows with the current rung checked. A push remembers the opener's id and the
//! scroll ([`page_stack::Saved`]); a pop ([`TrackMenuState::pop`], LEFT or BACK, or a click on the title band,
//! whose pointer-only stop is [`TITLE_KEY`]) restores both, so focus returns to the row that opened
//! the page by id. OK or RIGHT on a Nav row pushes ([`TrackMenuState::on_right`]); LEFT on the root
//! is still the tab switch and BACK on the root dismisses. Every page opens on an explicit id
//! ([`TrackMenuState::page_initial`]). A picker pick commits live and leaves the panel and page
//! open ([`TrackOk::Commit`]'s `keep_open`), so a run of picks is felt at once.
//!
//! **What Style can reach depends on the active renderer** ([`SubRenderer`]): only the client's
//! plain-text caption follows Size and Position, so under an image (PGS/VobSub) or native ASS/SSA
//! subtitle those two rows are dim, focusable and inert, with a separate note each. Color stays
//! live under every renderer: the subtitle ink tints bitmaps and ASS alike. Size and Position are
//! persisted by `route::select_subtitle_size` / `select_subtitle_position` and Color by
//! `player::set_subtitle_tone`: all three are one call into the one write door (`route::choose`,
//! `How::Optimistic`: live value first, the durable write second, never republished).
//!
//! **A sub-page never outlives the root it was built on.** The root's [`SubSig`] — subs
//! fingerprint, active index, renderer kind, transcoding, presenter and enhancement route — is
//! stored when the root is built and compared on every live poll; a mismatch refreshes the root in
//! place, and a sub-page in place too — it pops straight to the root only when its availability no
//! longer holds (the renderer, or whether Style is shown / locked). The replay canon
//! ([`TrackMenuState::canon`]) carries the tab, the page path with each return key, and the
//! selected key.
//!
//! **The Other languages flow** is data-driven by [`TrackMenuState::other`] (one `OtherLang` per
//! language, from the same `sub_layout` answer as the root), so a page lists exactly what its
//! opener promised. A language is identified by the Plex stream id of its first track in the item's
//! FULL list ([`LangId`], carried by [`TrackRow::OpenLang`] and [`TrackPage::Language`]), never a
//! list position, so tracks leaving or arriving cannot relabel an open page. A track pick on any
//! page commits and dismisses like a root pick. A page whose listing is gone (the language left the
//! offered list) pops to the root ([`TrackMenuState::pages_hold`]); a language page that drops to a
//! single track stays open, and popping it lands on that track's direct row.
use plx_data::metadata;
use plx_data::metadata::sub_layout::{self, LangId, OtherLang, RowBadge, RowTarget, SubHeader, SubModel, SubRow, SubTrack};
use plx_data::metadata::track_label;
use plx_plex::plex::session::{SubtitlePosition, SubtitleSize, SubtitleTone};
use plx_ui::frame::Budget;
use plx_ui::geom::IndexElem;
use plx_machine::machine::{Canon, Cx, EntryId, FocusKey, GroupId, Host, Measure};
use plx_ui::popover::Popover;
use plx_ui::screen::{At, Dir, DrawFrame, Focusable, GroupSpec, Part, Placed, Step};
use plx_ui::form::{Activation, Form, FormId, FormSection, FormTable, RowKey, RowKind};
use plx_ui::page_stack::{
    self, popover_group_of, popover_groups, popover_neighbour, popover_place,
    popover_register_stops, popover_seat, PageStack, PopoverPanel,
};
use plx_ui::panel_motion::PanelMotion;
use plx_ui::table::{Badge, Row, Section, TableView};
use plx_ui::table_screen::BAND_BASE;
use plx_ui::theme;
use plx_ui::{Painter, Rect};
use std::os::raw::c_int;


/// **What a focusable row of either tab IS** — the identity the [`FormTable`] resolves a press, a
/// focus move and a rebuild's landing by, never a position. The Audio tab's track rows, the two Plex
/// Pass DSP toggles and the Subtitles tab's rows (root, Style page and pickers) share one alphabet
/// so one [`FormTable`] serves every page. The non-selectable footnotes under the DSP pair and under
/// Style are inert slots ([`FormSection::note`]) and have no identity at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackRow {
    /// An Audio-tab track row — the index into the playing item's audio list
    /// ([`plx_data::metadata::PlayingItem::audio`]).
    Audio(usize),
    /// The Boost dialog toggle row (issue #266) — present whenever [`TrackMenuState::enhance_shown`]
    /// is `Some` (offered) OR [`TrackMenuState::enhance_disabled`] is (dim, with a reason),
    /// immediately after the last [`Self::Audio`].
    Boost,
    /// The Normalize loudness toggle row (issue #266), immediately after [`Self::Boost`].
    Loudness,
    /// The Subtitles tab's Off row.
    SubOff,
    /// The Subtitles tab's Forced only row, directly after Off: the third remembered state
    /// (Off, a track, or the forced track in the playing audio's language). It is offered wherever
    /// Off is, with or without a forced track on this item.
    SubForcedOnly,
    /// A Subtitles-tab track row — the index into the playing item's subs list.
    Sub(usize),
    /// The Timing row (hands off to the Timing capsule).
    Timing,
    /// The Style drill-in on the Subtitles root ([`TrackPage::Style`]).
    Style,
    /// The Other languages drill-in on the Subtitles root ([`TrackPage::OtherLanguages`]).
    OpenOther,
    /// Other languages page: a multi-track language's drill-in ([`TrackPage::Language`]). Carries
    /// the language's identity — the stream id of its first track in the item's FULL list
    /// ([`LangId`]) — so it stays that language when tracks leave, arrive or the offered subset
    /// changes; the id's slot only picks the row's focus key.
    OpenLang(LangId),
    /// Style page: the drill-in to one field's picker ([`TrackPage::Picker`]); reads out the
    /// field's current value.
    OpenField(StyleField),
    /// A picker page's choice: the field and the rung's index on that field's ladder.
    Choice(StyleField, usize),
    /// The Search for subtitles drill-in on the Subtitles root ([`TrackPage::Search`]). Present only
    /// when the host offers a search ([`TrackMenuState::set_search`]).
    OpenSearch,
    /// The Search page's language row: reads out the language searched, opens
    /// [`TrackPage::SearchLanguage`].
    SearchLang,
    /// The Search page's retry row, present only after a TRANSPORT failure (a refusal is not
    /// retried — a 403 retried is a 403).
    SearchRetry,
    /// A result row: the index into the hits of the generation on screen. **The one place this
    /// alphabet carries a position**: a hit has no identity of its own beyond its index — its key is
    /// ephemeral server state, re-minted per search — and the whole list is replaced at once when the
    /// generation moves, never reordered under the cursor.
    SearchHit(usize),
    /// A language on the search's language page: the index into [`search_languages`].
    LangChoice(usize),
}

impl From<RowTarget> for TrackRow {
    fn from(t: RowTarget) -> Self {
        match t {
            RowTarget::Off => TrackRow::SubOff,
            RowTarget::ForcedOnly => TrackRow::SubForcedOnly,
            RowTarget::Sub(i) => TrackRow::Sub(i),
            RowTarget::Timing => TrackRow::Timing,
            RowTarget::Style => TrackRow::Style,
            RowTarget::Other => TrackRow::OpenOther,
        }
    }
}

/// One caption style field: the Style page lists one drill-in per field, and each opens a picker
/// page of that field's ladder with the current rung checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleField {
    Size,
    Position,
    Color,
}

impl StyleField {
    pub const ALL: [StyleField; 3] = [StyleField::Size, StyleField::Position, StyleField::Color];

    fn ordinal(self) -> u32 {
        self as u32
    }

    fn label(self) -> &'static str {
        use plx_platform::i18n::msg;
        match self {
            Self::Size => msg::widgets_tracks_style_size(),
            Self::Position => msg::widgets_tracks_style_position(),
            Self::Color => msg::widgets_tracks_color(),
        }
    }

    /// How many rungs this field's ladder has.
    fn rungs(self) -> usize {
        match self {
            Self::Size => SubtitleSize::LADDER.len(),
            Self::Position => SubtitlePosition::LADDER.len(),
            Self::Color => SubtitleTone::LADDER.len(),
        }
    }

    /// The displayed name of rung `i` of this field's ladder.
    fn rung_label(self, i: usize) -> String {
        match self {
            Self::Size => subtitle_size_label(SubtitleSize::from_index(i as u8)),
            Self::Position => subtitle_position_label(SubtitlePosition::from_index(i as u8)).into(),
            Self::Color => tone_label(SubtitleTone::from_index(i as u8)).into(),
        }
    }
}

/// **A drill-in page of the Subtitles tab** — the form's `Dest`. The Subtitles root is the empty
/// page stack, not a value of this type (`docs/player-submenus.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackPage {
    /// Size, Position and Color, each a drill-in showing its current value.
    Style,
    /// One field's picker: a checked [`TrackRow::Choice`] per rung.
    Picker(StyleField),
    /// One row per language the viewer has not marked as theirs, A-Z: a single-track language is a
    /// direct pick row, a multi-track one a drill-in to its [`Self::Language`] page.
    OtherLanguages,
    /// One multi-track language's tracks (ranked), a pick row each; the payload is the language's
    /// [`LangId::stream`].
    Language(i64),
    /// Search for subtitles: the language searched, then what the server's agent found.
    Search,
    /// The language list the search can use ([`search_languages`]), the current one checked.
    SearchLanguage,
}

impl TrackPage {
    /// The title band's text; a language page is titled by its language (`other` is the page
    /// model, [`TrackMenuState::other`]).
    fn title(self, other: &[OtherLang]) -> String {
        match self {
            Self::Style => plx_platform::i18n::msg::widgets_tracks_style().to_string(),
            Self::Picker(field) => field.label().to_string(),
            Self::OtherLanguages => plx_platform::i18n::msg::widgets_tracks_other_languages().to_string(),
            Self::Language(stream) => {
                other.iter().find(|o| o.id.stream == stream).map(|o| o.name.clone()).unwrap_or_default()
            }
            Self::Search => plx_platform::i18n::msg::widgets_tracks_search_open().to_string(),
            Self::SearchLanguage => plx_platform::i18n::msg::widgets_tracks_search_language().to_string(),
        }
    }

    /// A stable small number for the replay canon. Never reordered: recordings hash it.
    fn code(self) -> u32 {
        match self {
            Self::Style => 1,
            Self::Picker(field) => 0x10 + field.ordinal(),
            Self::OtherLanguages => 0x20,
            Self::Language(stream) => 0x2000_0000 | (stream as u32 & 0x00FF_FFFF),
            Self::Search => 0x30,
            Self::SearchLanguage => 0x31,
        }
    }
}

/// The hand-assigned focus key of each row: a family base per kind of row plus the track's own
/// index, all far below the band. None of these is a position, so a menu whose rows reorder (a
/// track list that sorts differently, the DSP pair appearing) moves no key. Free families for the
/// language pages: `0x0008_0000` the Other languages drill-in, `0x0009_0000 +` the slot a language
/// holds on that page. Search: `0x000A_0000` its drill-in, `0x000B_000x` the language and retry rows,
/// `0x000C_0000 +` a result, `0x000D_0000 +` a language choice.
impl FormId for TrackRow {
    fn key(&self) -> RowKey {
        // a picker's rungs: one 256-wide block per field (a ladder is a handful of rungs)
        let rung = |f: StyleField, i: usize| 0x0007_0000 + f.ordinal() * 0x100 + (i as u32).min(0xFF);
        RowKey(match *self {
            TrackRow::Audio(i) => 0x0001_0000 + i as u32,
            TrackRow::Boost => 0x0002_0000,
            TrackRow::Loudness => 0x0002_0001,
            TrackRow::SubOff => 0x0003_0000,
            TrackRow::SubForcedOnly => 0x0003_0001,
            TrackRow::Sub(i) => 0x0004_0000 + i as u32,
            TrackRow::Timing => 0x0005_0000,
            TrackRow::Style => 0x0005_0001,
            TrackRow::OpenField(f) => 0x0006_0000 + f.ordinal(),
            TrackRow::Choice(f, i) => rung(f, i),
            TrackRow::OpenOther => 0x0008_0000,
            TrackRow::OpenLang(lang) => 0x0009_0000 + (lang.slot as u32).min(0xFFFF),
            TrackRow::OpenSearch => 0x000A_0000,
            TrackRow::SearchLang => 0x000B_0000,
            TrackRow::SearchRetry => 0x000B_0001,
            TrackRow::SearchHit(i) => 0x000C_0000 + (i as u32).min(0xFFFF),
            TrackRow::LangChoice(i) => 0x000D_0000 + (i as u32).min(0xFFFF),
        })
    }
}

/// Both tabs' form: the action type is `()` (the id says what a row does); `Dest` is the page a
/// Nav row opens.
type TrackForm = Form<TrackRow, (), TrackPage>;
type TrackTable = FormTable<TrackRow, (), TrackPage>;

/// [`TrackMenuState::enh_state`]'s answer: the Audio tab's Boost/Loudness offer as displayed, the
/// route it rides, the reason it is disabled, and the live subtitle effect its note names.
type EnhState = (
    Option<plx_plex::plex::AudioEnhancements>,
    Option<plx_media::route::EnhancementRoute>,
    Option<plx_media::route::DisabledReason>,
    plx_media::route::SubtitleEffect,
);

/// **The tallest page this menu shows with app-owned rows** — what decides which SIDE of its opener
/// the panel opens on (`plx_ui::popover::anchor_panel`'s `need_h`), so the side is the same on
/// every page and never flips under a viewer who pushes one. A server-length list (audio dubs,
/// subtitle tracks) may be taller and scrolls inside the panel. Classic's room above its row is
/// 710, which this stays under, so classic always opens upward. `pub(crate)` because the HUD lint
/// places this menu with it (`hud_format::lint_menus`, M2), through [`place_panel`].
pub(crate) const NEED_H: f32 = 600.0;

/// **The one call of the shared placer for this menu**: a panel of `size` beside `anchor`, on the side
/// [`NEED_H`] decides, `PLAYER_MENU_GAP` away, inside `SAFE`. The live panel ([`table_natural`]), the
/// overscan audit ([`overscan_rects`]) and the HUD lint (`hud_format::lint_menus`) all place through
/// it, so none can drift from another.
pub(crate) fn place_panel(anchor: Rect, size: (f32, f32)) -> plx_ui::popover::Anchored {
    plx_ui::popover::anchor_panel(
        anchor,
        size,
        NEED_H,
        theme::layout::PLAYER_MENU_GAP,
        plx_ui::popover::Placement::PLAYER_MENU,
    )
}

/// **A tab root's natural panel**, placed beside `anchor` — the layout of `table` as the panel would
/// hug it. Each tab hugs its own rows (shared menu rule); the edge the opener's segment ends at is
/// fixed, so switching tabs moves only the far edge.
///
/// The panel grows away from the opener and its height is capped at the room on that side, so a
/// long list (an item with many audio dubs) SCROLLS inside the panel instead of the panel itself
/// spilling over the opener or past the safe frame.
fn table_natural(
    table: &TableView,
    anchor: Rect,
    measure: &dyn plx_machine::machine::Measure,
) -> plx_ui::popover::Anchored {
    let pw = table.menu_panel_width(measure);
    // A note row wraps, so its line count depends on the width: it is resolved HERE, against the
    // same `measure` and `pw` the panel is sized with, so no caller (`update`, hit-testing,
    // `draw`) can read the count a rebuild left stale. Idempotent and a few short strings.
    table.fit_notes(pw, measure);
    let ph = table.measured_height().max(160.0);
    place_panel(anchor, (pw, ph))
}

/// The renderer the ACTIVE subtitle is drawn by — what decides whether the caption's Size and
/// Position can reach it. Only the client's plain-text caption draw follows them
/// (`appkit::player_hud::draw_subtitle_message`); an image subtitle keeps its own bitmap geometry and
/// native ASS/SSA its authored layout. The subtitle INK tints all three, so Color is always live.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubRenderer {
    Text,
    Image,
    Styled,
}

impl SubRenderer {
    fn of_codec(codec: &str) -> Self {
        if sub_layout::is_image_sub_codec(codec) {
            Self::Image
        } else if codec.eq_ignore_ascii_case("ass") || codec.eq_ignore_ascii_case("ssa") {
            Self::Styled
        } else {
            Self::Text
        }
    }

    /// The note under the Style rows, when Size and Position cannot reach this renderer.
    fn note(self) -> Option<&'static str> {
        match self {
            Self::Text => None,
            Self::Image => Some(plx_platform::i18n::msg::widgets_tracks_style_image_note()),
            Self::Styled => Some(plx_platform::i18n::msg::widgets_tracks_style_styled_note()),
        }
    }
}

/// **What the Subtitles root's rows and locks were built from** — the rebuild signature. A live
/// poll compares it to the current answers and rebuilds when any input changed: the subs list
/// (stream ids and whether each is offered on this route), the active index, the renderer kind,
/// whether the route is transcoding, the enhancement route and subtitle effect (what the Style
/// lock and Timing's omission read), and whether the landing left a remembered subtitle unshown
/// (the note under the first group).
#[derive(Clone, Debug, PartialEq)]
struct SubSig {
    subs: Vec<(i64, bool)>,
    active: c_int,
    renderer: SubRenderer,
    transcoding: bool,
    /// who draws the subtitle: what Timing and Style are gated and explained by
    presenter: plx_media::route::SubtitlePresenter,
    enhancement: Option<plx_media::route::EnhancementRoute>,
    effect: plx_media::route::SubtitleEffect,
    /// the show's remembered subtitle was chosen for this play and the route shows none of it
    /// ([`plx_media::route::cur_memory_note`]): the root says why
    memory_note: bool,
    /// Forced only is the mode, with the language its row reads out
    /// ([`sub_layout::forced_only_readout`]); `None` off the mode. The row's check, the single
    /// checkmark and the read-out all follow it, and the language moves with the playing audio.
    forced_only: Option<String>,
}

impl SubSig {
    /// What a pushed Style page's availability was built from: the renderer (Size / Position reach
    /// only text), whether the server draws the subtitle (Style is shown but locked), and whether
    /// a transcode with no subtitle on omits it (the gate [`TrackMenuState::layout`] shows Style
    /// by). A page is popped when this changes and refreshed in place otherwise.
    fn page_availability(&self) -> (SubRenderer, bool, bool) {
        (self.renderer, self.server_burn().is_some(), self.transcoding && self.server_burn().is_none())
    }

    /// Why the server draws the subtitle, when it does.
    fn server_burn(&self) -> Option<plx_media::route::BurnReason> {
        match self.presenter {
            plx_media::route::SubtitlePresenter::ServerBurn(why) => Some(why),
            _ => None,
        }
    }
}

/// The menu's whole state, owned by the container that mounts this panel — the modal PHASE and the
/// appear spring belong to `ui::containers::modal::ModalStack` now, not to this struct; `draw` takes
/// the appear fraction as a parameter instead of stepping its own [`Popover`].
pub struct TrackMenuState {
    tab: c_int, // 0=Audio, 1=Subtitles
    active_audio: c_int, // index into the playing item's audio list
    active_sub: c_int, // -1 = Off, else index into the playing item's subs list
    /// Forced only is the mode ([`plx_media::route::cur_sub_mode`]). `active_sub` is then the
    /// forced track it drew (or -1 when none matched) and is NOT what the root checks: the
    /// checkmark sits on the Forced only row alone ([`Self::checked_sub`]).
    forced_only: bool,
    /// The language Forced only falls back to when the audio names none
    /// ([`plx_media::route::forced_only_lang`]) as of the last open or poll. `on_ok` has no
    /// session, so a pick matches and remembers by this copy; its own Forced only pick moves it.
    forced_lang: String,
    /// Both tabs' rows, declared once each as a [`TrackForm`] and resolved by [`TrackRow`]
    /// identity — [`Self::on_ok`] reads back what the focused row IS from its id rather than from a
    /// position (and so cannot disagree with what was drawn, the way a fresh call to
    /// [`visible_subs`] could once a transcode starts). Whichever tab is built owns the table; a
    /// tab switch replaces it whole ([`Self::rebuild`]).
    form: TrackTable,
    /// The timing offset (ms) the Timing row reads out — seeded from the player on open. Kept
    /// locally (rather than re-reading the player's atomic on every draw) so the Timing capsule's
    /// eventual hand-off starts from what THIS panel showed, not from a commit the loop has not
    /// yet performed.
    offset_ms: i64,
    /// The caption tone the Color rows read out and check — seeded from the player on open, same
    /// reasoning as [`Self::offset_ms`]: what this panel last drew must not depend on the global
    /// the loop has not yet written (`TrackCommit::SubtitleTone` is dispatched to the loop, not
    /// applied inline by `on_ok`). The size and position below follow the same rule.
    tone: SubtitleTone,
    /// The caption size the Size rows read out and check.
    size: SubtitleSize,
    /// The caption position the Position rows read out and check.
    position: SubtitlePosition,
    /// The pages pushed above the Subtitles root, outermost first (empty = the root). Each entry
    /// carries the opener and scroll of the page beneath it.
    pages: PageStack<TrackPage, TrackRow>,
    /// The renderer of the ACTIVE subtitle, captured when the root is (re)built — what the Style
    /// page's Size and Position lock reads.
    renderer: SubRenderer,
    /// What the Subtitles root was last built from; a live poll rebuilds when it moves.
    sub_sig: Option<SubSig>,
    /// The Other languages page's model — one entry per language, as of the last root build or
    /// live poll. The Other languages and language pages draw from it, so what a page lists and
    /// what its opener promised cannot disagree, and a language whose entry is gone pops its page.
    other: Vec<OtherLang>,
    /// The subtitle indices the Subtitles ROOT lists as track rows, in display order — what the
    /// harness's `track:N` counts first ([`Self::sub_track_for_target`]).
    root_tracks: Vec<usize>,
    /// "Your languages" this play resolved under, in PREFERENCE order — the pref's BCP-47 code (if
    /// the play resolved under one), the playing audio's language, and the current subtitle's own,
    /// exactly as `metadata::sub_layout::sub_sections`' `yours` parameter reads them (compared by
    /// `metadata::lang_key`, never by literal string equality). Owned rather than borrowed, so
    /// a rebuild (tab switch) needs nothing from the caller beyond `ps`/`meta`.
    yours: Vec<String>,
    /// The Audio tab's Plex Pass DSP toggle rows (issue #266) — `None` when they are not offered
    /// at all (Hidden or Disabled), else what they currently read out: "desired while pending,
    /// applied otherwise" (`route::displayed_audio_enhancements`), same reasoning as
    /// [`Self::offset_ms`]/[`Self::tone`] — a run of toggle presses inside one open counts from
    /// what THIS panel last drew, and [`Self::rebuild`] is the only writer, on every (re)build of
    /// the Audio tab (`new`/`focus_tab`).
    enhance_shown: Option<plx_plex::plex::AudioEnhancements>,
    /// The route [`Self::enhance_shown`] would take, kept alongside it (`Some` iff `enhance_shown`
    /// is `Some`) — the Audio tab's consequence note (M7: a burned subtitle, a dropped Dolby Vision
    /// decl…83444 tokens truncated…{
            push_style(&mut menu, &store);
            frame(&mut menu, &ps, &store);
            assert!(menu.pop(&ps, store.view()));
            frame(&mut menu, &ps, &store);
        }
        // and a push that goes deeper while the first is still sliding
        push_style(&mut menu, &store);
        run(&mut menu, &ps, &store, 2);
        let i = menu.form.index_of(&TrackRow::OpenField(StyleField::Color)).expect("Color row");
        menu.focus_row(i as c_int);
        assert_eq!(menu.on_ok(store.view()), TrackOk::Navigated);
        assert_eq!(menu.pages.len(), 2);
        run(&mut menu, &ps, &store, 240);
        assert!(!menu.transitioning());
        assert_eq!(shown(&menu), natural(&menu));
        assert!(menu.pop(&ps, store.view()));
        assert!(menu.pop(&ps, store.view()));
        run(&mut menu, &ps, &store, 240);
        assert_eq!(shown(&menu), root);
        assert!(!menu.transitioning());
        teardown(&ps);
    }

    /// An Audio/Subtitles tab switch resizes the card with the same spring, and the content
    /// does not slide.
    #[test]
    fn a_tab_switch_animates_the_height() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open();
        let tall = natural(&menu);
        menu.focus_tab(&ps, store.view(), 0);
        let short = natural(&menu);
        assert!(short.h < tall.h, "the empty Audio tab is shorter than the Subtitles tab: {} < {}", short.h, tall.h);
        assert_eq!(short.y + short.h, tall.y + tall.h, "the bottom edge is the anchor");
        assert_eq!(short.x + short.w, tall.x + tall.w, "and so is the right edge");
        assert!(frame(&mut menu, &ps, &store), "the resize is motion");
        let first = shown(&menu);
        assert!(first.h > short.h && first.h <= tall.h, "mid-resize the card is between the two heights: {first:?}");
        assert!(!menu.motion.sliding(), "a tab switch swaps the content, it does not slide it");
        assert_eq!(menu.motion.live_dx(), 0.0);
        run(&mut menu, &ps, &store, 240);
        assert_eq!(shown(&menu), short);
        // and back
        menu.focus_tab(&ps, store.view(), 1);
        assert!(frame(&mut menu, &ps, &store));
        run(&mut menu, &ps, &store, 240);
        assert_eq!(shown(&menu), tall);
        assert!(!frame(&mut menu, &ps, &store));
        teardown(&ps);
    }

    /// The layout target is measured when the table changed and not otherwise.
    #[test]
    fn the_layout_is_cached_until_the_table_changes() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open();
        let rev = menu.form.table.layout_rev();
        let a = natural(&menu);
        run(&mut menu, &ps, &store, 3);
        assert_eq!(menu.form.table.layout_rev(), rev, "update/draw/place do not touch the table's layout");
        assert_eq!(natural(&menu), a);
        push_style(&mut menu, &store);
        assert_ne!(menu.form.table.layout_rev(), rev, "a page change moves the revision");
        teardown(&ps);
    }

    // ---- the Search for subtitles pages ----------------------------------------------------------

    use plx_data::subsearch::{DownloadPhase, SearchFailure, SearchStatus, SubHit, SubSearchCmd, SubSearchSnapshot};

    fn hit(title: &str) -> SubHit {
        SubHit { key: format!("/library/streams/{title}"), title: title.into(), provider: "OpenSubtitles".into(),
            language: "Nederlands".into(), language_code: "nld".into(), codec: "srt".into(), score: 1,
            hearing_impaired: false, forced: false }
    }

    fn snap(status: SearchStatus, failure: Option<SearchFailure>, hits: Vec<SubHit>, download: DownloadPhase) -> SubSearchSnapshot {
        SubSearchSnapshot { status, failure, hits, lang: "nl".into(), download, gen: 7 }
    }

    /// A Subtitles-tab menu with one English track, offered a search in state `s`, on the Search page.
    fn on_search_page(s: SubSearchSnapshot) -> (TrackMenuState, plx_data::stores::metadata::MetadataStore) {
        on_search_page_with(Vec::new(), s)
    }

    /// [`on_search_page`], with `extra` subtitle streams on the playing item beside the English one.
    fn on_search_page_with(
        extra: Vec<metadata::Stream>,
        s: SubSearchSnapshot,
    ) -> (TrackMenuState, plx_data::stores::metadata::MetadataStore) {
        plx_media::player::sidecar::reset();
        let ps = plx_media::route::PlaybackSession::IDLE;
        let mut subs = vec![metadata::Stream { id: 1, index: 0, lang: "English".into(),
            lang_code: "eng".into(), codec: "srt".into(), ..Default::default() }];
        subs.extend(extra);
        let store = store_with(subs);
        let mut menu = TrackMenuState::new_with(&ps, store.view(), 1, vec!["eng".into()], true);
        menu.set_search(SubSearchSnapshot::idle(7));
        menu.focus_key(TrackRow::OpenSearch.key().0);
        assert_eq!(menu.on_ok(store.view()), TrackOk::Navigated);
        menu.set_search(s);
        (menu, store)
    }

    /// The row exists only when the host offers a search, so a host without one — every test host
    /// that predates this page — draws exactly the menu it always did.
    #[test]
    fn the_search_row_appears_only_when_the_host_offers_a_search() {
        let _g = plx_base::testlock::serial();
        let ps = plx_media::route::PlaybackSession::IDLE;
        let store = store_with(vec![metadata::Stream { id: 1, index: 0, lang: "English".into(),
            lang_code: "eng".into(), codec: "srt".into(), ..Default::default() }]);
        let mut menu = TrackMenuState::new(&ps, store.view(), 1, vec!["eng".into()]);
        assert!(!menu.ids().contains(&TrackRow::OpenSearch));
        menu.set_search(SubSearchSnapshot::idle(3));
        assert!(!menu.ids().contains(&TrackRow::OpenSearch), "a late publish never adds the offer");
        let menu = TrackMenuState::new_with(&ps, store.view(), 1, vec!["eng".into()], true);
        assert_eq!(menu.ids().last(), Some(&TrackRow::OpenSearch), "the offered root ends with the row");
    }

    /// Entering the page is what makes the screen open the search — never merely opening the menu.
    #[test]
    fn entering_the_search_page_asks_for_the_search_and_lands_on_the_language_row() {
        let _g = plx_base::testlock::serial();
        let (menu, _store) = on_search_page(snap(SearchStatus::Searching, None, vec![], DownloadPhase::None));
        assert_eq!(menu.page_path(), [TrackPage::Search]);
        assert!(menu.wants_search());
        assert_eq!(menu.ids(), [TrackRow::SearchLang], "searching: nothing to list yet");
        assert_eq!(menu.form.selected_id(), Some(&TrackRow::SearchLang));
    }

    /// Results land in place, and a press quotes the generation they were published under — the
    /// keys are ephemeral, so the store refuses a press against a replaced set.
    #[test]
    fn a_result_press_asks_to_download_quoting_its_generation() {
        let _g = plx_base::testlock::serial();
        let (mut menu, store) =
            on_search_page(snap(SearchStatus::Ready, None, vec![hit("a"), hit("b")], DownloadPhase::None));
        assert_eq!(menu.ids(), [TrackRow::SearchLang, TrackRow::SearchHit(0), TrackRow::SearchHit(1)]);
        assert_eq!(menu.page_path(), [TrackPage::Search], "a landing never pops the page");
        menu.focus_key(TrackRow::SearchHit(1).key().0);
        assert_eq!(menu.on_ok(store.view()), TrackOk::Search(SubSearchCmd::Download { gen: 7, hit: 1 }));
    }

    /// Only a transport failure is retryable; a refusal says why and offers nothing, because a 403
    /// retried is a 403.
    #[test]
    fn a_transport_failure_offers_a_retry_and_a_denial_does_not() {
        let _g = plx_base::testlock::serial();
        let (mut menu, store) =
            on_search_page(snap(SearchStatus::Failed, Some(SearchFailure::Transport), vec![], DownloadPhase::None));
        assert!(menu.ids().contains(&TrackRow::SearchRetry));
        menu.focus_key(TrackRow::SearchRetry.key().0);
        assert_eq!(menu.on_ok(store.view()), TrackOk::Search(SubSearchCmd::Retry));

        let (menu, _) =
            on_search_page(snap(SearchStatus::Failed, Some(SearchFailure::Denied), vec![], DownloadPhase::None));
        assert!(!menu.ids().contains(&TrackRow::SearchRetry));
    }

    /// One download at a time: while one is on the wire every result and the language row are inert.
    #[test]
    fn a_busy_download_makes_the_rows_inert() {
        let _g = plx_base::testlock::serial();
        let (mut menu, store) = on_search_page(snap(SearchStatus::Ready, None, vec![hit("a"), hit("b")],
            DownloadPhase::Sending { hit: 0 }));
        menu.focus_key(TrackRow::SearchHit(1).key().0);
        assert_eq!(menu.on_ok(store.view()), TrackOk::Inert);
        menu.focus_key(TrackRow::SearchLang.key().0);
        assert_eq!(menu.on_ok(store.view()), TrackOk::Inert, "no language change mid-download");
    }

    /// Picking a language returns to the Search page and reads "searching" at once, so the old
    /// language's results never sit under the new language's name.
    #[test]
    fn picking_a_language_returns_to_the_search_page_searching_in_it() {
        let _g = plx_base::testlock::serial();
        let (mut menu, store) =
            on_search_page(snap(SearchStatus::Ready, None, vec![hit("a")], DownloadPhase::None));
        menu.focus_key(TrackRow::SearchLang.key().0);
        assert_eq!(menu.on_ok(store.view()), TrackOk::Navigated);
        assert_eq!(menu.page_path(), [TrackPage::Search, TrackPage::SearchLanguage]);
        let langs = search_languages();
        assert!(langs.iter().all(|l| l.code.len() == 2), "only codes the server accepts");
        let fr = langs.iter().position(|l| l.code == "fr").expect("French is offered");
        menu.focus_key(TrackRow::LangChoice(fr).key().0);
        assert_eq!(menu.on_ok(store.view()), TrackOk::Search(SubSearchCmd::SetLanguage("fr".into())));
        assert_eq!(menu.page_path(), [TrackPage::Search]);
        assert_eq!(menu.search().map(|s| s.status), Some(SearchStatus::Searching));
        assert_eq!(menu.ids(), [TrackRow::SearchLang], "the old results are gone");
    }

    /// The search rows join the one key alphabet without colliding with any other row.
    #[test]
    fn the_search_keys_are_distinct_from_every_other_row() {
        let mut ids = vec![TrackRow::SubOff, TrackRow::Timing, TrackRow::Style, TrackRow::Boost,
            TrackRow::Loudness, TrackRow::OpenOther, TrackRow::OpenSearch, TrackRow::SearchLang,
            TrackRow::SearchRetry];
        ids.extend((0..300).flat_map(|i| [TrackRow::Sub(i), TrackRow::Audio(i), TrackRow::SearchHit(i),
            TrackRow::LangChoice(i)]));
        let keys: Vec<u32> = ids.iter().map(|i| i.key().0).collect();
        for (n, a) in keys.iter().enumerate() {
            assert!(*a < BAND_BASE);
            assert!(!keys[n + 1..].contains(a), "duplicate key {a:#x}");
        }
    }

    /// The auto-select hands the installed stream over ONCE — the snapshot is republished every
    /// frame — as the same sidecar commit a pick on its row makes.
    #[test]
    fn an_installed_download_commits_once_as_a_sidecar_pick() {
        let _g = plx_base::testlock::serial();
        let installed = DownloadPhase::Installed { hit: 0, stream: plx_data::subsearch::InstalledStream {
            id: 1929519, key: "/library/streams/1929519".into(), codec: "srt".into(), language_code: "nld".into() } };
        let waiting = DownloadPhase::Waiting { hit: 0, before: vec![1], attempts: 0 };
        let (mut menu, _store) = on_search_page(snap(SearchStatus::Ready, None, vec![hit("rel")], waiting));
        assert!(menu.take_search_commit().is_none(), "nothing installed yet");
        menu.set_search(snap(SearchStatus::Ready, None, vec![hit("rel")], installed));
        let (stream, commit) = menu.take_search_commit().expect("the install is handed over");
        assert_eq!(commit, TrackCommit::Subtitle { render_ordinal: -1, stream_id: 1929519,
            sidecar_key: Some("/library/streams/1929519".into()), sidecar_codec: "srt".into(),
            // remembered by what it IS: the dutch sidecar's language, kind and title, as the hit
            // read it (the `hit("rel")` helper's own badges are not forced and not SDH)
            memo: Some(plx_plex::plex::session::prefs::SubtitleMemo::Track(
                plx_plex::plex::session::prefs::SubtitleTrackMemo {
                    lang: "nld".into(), kind: "text".into(), external: true, title: "rel".into(),
                    ..Default::default()
                })) });
        assert!(stream.external && stream.sidecar_renderable(), "listed as a drawable sidecar");
        assert_eq!((stream.lang.as_str(), stream.title.as_str()), ("Nederlands", "rel"));
        assert!(menu.take_search_commit().is_none(), "never twice for one stream");
    }

    /// **Regression (device, 2026-10-04):** after a download, the Subtitles and Audio menus could
    /// no longer be opened. The store keeps its Installed state for the playing item, and every
    /// NEW panel saw it, re-committed and dismissed itself on its first tick. Only a panel that
    /// watched the download in flight may auto-select it.
    #[test]
    fn a_panel_opened_after_the_install_never_auto_selects_it_again() {
        let _g = plx_base::testlock::serial();
        let installed = DownloadPhase::Installed { hit: 0, stream: plx_data::subsearch::InstalledStream {
            id: 1929519, key: "/library/streams/1929519".into(), codec: "srt".into(), language_code: "nld".into() } };
        let ps = plx_media::route::PlaybackSession::IDLE;
        let store = store_with(vec![metadata::Stream { id: 1, index: 0, lang: "English".into(),
            lang_code: "eng".into(), codec: "srt".into(), ..Default::default() }]);
        for tab in [0, 1] {
            let mut menu = TrackMenuState::new_with(&ps, store.view(), tab, vec!["eng".into()], true);
            menu.set_search(snap(SearchStatus::Ready, None, vec![hit("rel")], installed.clone()));
            assert!(menu.take_search_commit().is_none(), "tab {tab}: reopening must not re-commit");
        }
    }

    /// **Regression (ultrareview, 2026-10-05):** a result badged SDH/Forced on the Search page lost
    /// the badge once it landed in the Subtitles list — the listed stream was built with both flags
    /// defaulted. It carries the hit's own flags, so the row reads the same in both places.
    #[test]
    fn an_installed_download_keeps_its_sdh_and_forced_flags() {
        let _g = plx_base::testlock::serial();
        let flagged = SubHit { hearing_impaired: true, forced: true, ..hit("rel") };
        let waiting = DownloadPhase::Waiting { hit: 0, before: vec![1], attempts: 0 };
        let installed = DownloadPhase::Installed { hit: 0, stream: plx_data::subsearch::InstalledStream {
            id: 1929519, key: "/library/streams/1929519".into(), codec: "srt".into(), language_code: "nld".into() } };
        let (mut menu, _store) = on_search_page(snap(SearchStatus::Ready, None, vec![flagged.clone()], waiting));
        menu.set_search(snap(SearchStatus::Ready, None, vec![flagged], installed));
        let (stream, _) = menu.take_search_commit().expect("the install is handed over");
        assert!(stream.sdh && stream.forced, "sdh={} forced={}", stream.sdh, stream.forced);
    }

    /// A sidecar the server already carries, as the playing item lists it: the release name PMS
    /// copied from the candidate it was downloaded from, under a NEW id.
    fn sidecar(id: i64, title: &str, lang_code: &str) -> metadata::Stream {
        metadata::Stream { id, lang_code: lang_code.into(), title: title.into(), external: true,
            key: format!("/library/streams/{id}"), codec: "srt".into(), ..Default::default() }
    }

    /// **Regression (owner report):** a hit the item ALREADY carries as a sidecar (downloaded
    /// earlier, or on another client) could be pressed and added a second time, and the track menu
    /// then listed it twice until the film was reloaded. It reads "Added" like the row installed
    /// this session and OK does nothing on it; its neighbours stay live.
    #[test]
    fn a_hit_the_item_already_carries_reads_added_and_cannot_be_added_again() {
        let _g = plx_base::testlock::serial();
        let (mut menu, store) = on_search_page_with(
            vec![sidecar(5, "a", "nld")],
            snap(SearchStatus::Ready, None, vec![hit("a"), hit("b")], DownloadPhase::None),
        );
        let (row_a, row_b) = (TrackRow::SearchHit(0), TrackRow::SearchHit(1));
        let value_of = |menu: &TrackMenuState, id: TrackRow| {
            let i = menu.form.index_of(&id).expect("on the page");
            let row = menu.form.table.sections.iter().flat_map(|s| &s.rows).nth(i).unwrap();
            row.value.clone().unwrap_or_default()
        };
        assert_eq!(value_of(&menu, row_a), plx_platform::i18n::msg::widgets_tracks_search_added());
        assert_eq!(value_of(&menu, row_b), "", "a hit the item lacks is not marked");
        menu.focus_key(row_a.key().0);
        assert_eq!(menu.on_ok(store.view()), TrackOk::Inert);
        menu.focus_key(row_b.key().0);
        assert_eq!(menu.on_ok(store.view()), TrackOk::Search(SubSearchCmd::Download { gen: 7, hit: 1 }));
    }

    /// The identity is the release name AND the language: the same release in another language, or
    /// an embedded track that merely shares the title, is not "already added".
    #[test]
    fn only_a_sidecar_of_the_same_release_and_language_marks_a_hit_added() {
        let _g = plx_base::testlock::serial();
        let embedded = metadata::Stream { external: false, key: String::new(), ..sidecar(6, "a", "nld") };
        let (mut menu, store) = on_search_page_with(
            vec![sidecar(5, "a", "fra"), embedded, sidecar(7, "", "nld")],
            snap(SearchStatus::Ready, None, vec![hit("a"), SubHit { title: String::new(), ..hit("a") }],
                DownloadPhase::None),
        );
        for i in 0..2 {
            menu.focus_key(TrackRow::SearchHit(i).key().0);
            assert_eq!(menu.on_ok(store.view()), TrackOk::Search(SubSearchCmd::Download { gen: 7, hit: i }),
                "hit {i}");
        }
    }

    /// The installed subtitle joins the PLAYING item's list, once, and only that item's.
    #[test]
    fn an_installed_subtitle_is_appended_to_the_playing_item_once_and_only_there() {
        let _g = plx_base::testlock::serial();
        let mut store = store_with(vec![metadata::Stream { id: 1, ..Default::default() }]);
        let (sid, rk) = { let p = store.view().playing().expect("playing"); (p.sid, p.rk.clone()) };
        let stream = || metadata::Stream { id: 9, external: true, key: "/library/streams/9".into(),
            codec: "srt".into(), ..Default::default() };
        let append = |rk: String| plx_data::stores::metadata::MetadataCmd::AppendPlayingSub { sid, rk, stream: stream() };
        assert!(store.run(append(rk.clone())));
        assert!(!store.run(append(rk.clone())), "a repeat is a no-op");
        assert!(!store.run(append(format!("{rk}-other"))), "another item's landing is dropped");
        let ids: Vec<i64> = store.view().playing().unwrap().subs.iter().map(|s| s.id).collect();
        assert_eq!(ids, [1, 9]);
    }

    /// **Regression (owner report):** PMS gives a re-downloaded subtitle a NEW id, so the stale
    /// entry for the same release and the new one both sat in the list (two rows, one subtitle)
    /// until a reload. The new id takes the old entry's place.
    #[test]
    fn a_reinstalled_subtitle_replaces_its_stale_entry_instead_of_duplicating_it() {
        let _g = plx_base::testlock::serial();
        let mut store = store_with(vec![metadata::Stream { id: 1, ..Default::default() }, sidecar(5, "rel", "nld")]);
        let (sid, rk) = { let p = store.view().playing().expect("playing"); (p.sid, p.rk.clone()) };
        let append = |stream| plx_data::stores::metadata::MetadataCmd::AppendPlayingSub { sid, rk: rk.clone(), stream };
        assert!(store.run(append(sidecar(9, "rel", "nld"))), "the list changed");
        let ids = |store: &plx_data::stores::metadata::MetadataStore| -> Vec<i64> {
            store.view().playing().unwrap().subs.iter().map(|s| s.id).collect()
        };
        assert_eq!(ids(&store), [1, 9], "one entry for the subtitle, under the new id, in place");
        assert!(!store.run(append(sidecar(9, "rel", "nld"))), "the same id again is a no-op");
        assert_eq!(ids(&store), [1, 9]);
    }

    /// A different release, a different language, or a nameless stream is a different subtitle:
    /// it still appends beside what is there.
    #[test]
    fn a_different_subtitle_still_appends() {
        let _g = plx_base::testlock::serial();
        let mut store = store_with(vec![sidecar(5, "rel", "nld"), sidecar(6, "", "nld")]);
        let (sid, rk) = { let p = store.view().playing().expect("playing"); (p.sid, p.rk.clone()) };
        let append = |stream| plx_data::stores::metadata::MetadataCmd::AppendPlayingSub { sid, rk: rk.clone(), stream };
        assert!(store.run(append(sidecar(9, "other", "nld"))), "another release");
        assert!(store.run(append(sidecar(10, "rel", "fra"))), "the same release in another language");
        assert!(store.run(append(sidecar(11, "", "nld"))), "an empty title names nothing, so matches nothing");
        let ids: Vec<i64> = store.view().playing().unwrap().subs.iter().map(|s| s.id).collect();
        assert_eq!(ids, [5, 6, 9, 10, 11]);
    }
}

/// **Classic's settled Tracks panel does not move** when its placement comes from an anchor: the
/// right edge is the control row's (`CTRL_RIGHT`), the bottom edge 764, the width the hug, and the
/// height the measured one up to the room above the row.
#[cfg(test)]
mod classic_placement_tests {
    use super::tests::stream;
    use super::*;
    use plx_base::fontcov::advances::ShippedMeasure;

    fn item(audio: usize) -> plx_data::stores::metadata::MetadataStore {
        let mut store = plx_data::stores::metadata::MetadataStore::default();
        let mut item = metadata::PlayingItem::with_subs(vec![stream(10, 0, "English", "eng", ""), stream(11, 1, "French", "fra", "SDH")]);
        item.audio = (0..audio).map(|i| stream(100 + i as i64, i as i64, &format!("Dub {i}"), "eng", "")).collect();
        assert!(store.run(plx_data::stores::metadata::MetadataCmd::InstallPlaying(Some(item))));
        store
    }

    /// (name, right edge, bottom edge, width, height) of each settled page.
    fn settled(audio: usize) -> Vec<(String, [f32; 4])> {
        let _g = plx_base::testlock::serial();
        let ps = plx_media::route::PlaybackSession::IDLE;
        let store = item(audio);
        let mut out = Vec::new();
        for (tab, name) in [(0, "audio root"), (1, "subtitles root")] {
            let mut menu = TrackMenuState::new(&ps, store.view(), tab, vec!["eng".into()]);
            let r = menu.panel_rect(&ShippedMeasure);
            out.push((format!("{name} ({audio} dubs)"), [r.x + r.w, r.y + r.h, r.w, r.h]));
            if let Some(i) = menu.form.index_of(&TrackRow::Style) {
                menu.focus_row(i as c_int);
                assert_eq!(menu.on_ok(store.view()), TrackOk::Navigated);
                let r = menu.panel_rect(&ShippedMeasure);
                out.push((format!("style page ({audio} dubs)"), [r.x + r.w, r.y + r.h, r.w, r.h]));
            }
        }
        plx_plex::plex::reset_servers_for_test();
        out
    }

    /// Captured from the constants-placed panels (`CTRL_RIGHT - pw`, bottom 764) before the
    /// placement came from an anchor: (right, bottom, width, height).
    #[test]
    fn classic_tracks_panels_settle_where_they_always_did() {
        let got = settled(3);
        assert_eq!(
            got.iter().map(|(n, r)| (n.as_str(), *r)).collect::<Vec<_>>(),
            [
                ("audio root (3 dubs)", [1824.0, 764.0, 468.0, 356.0]),
                ("subtitles root (3 dubs)", [1824.0, 764.0, 447.0, 488.0]),
                ("style page (3 dubs)", [1824.0, 764.0, 450.0, 296.0]),
            ],
        );
    }

    /// A list taller than the room scrolls inside a panel that stops at the cap. The cap was 704
    /// (`bottom - 60`) and is now the room above the row, 710: the one settled frame that differs,
    /// and only for a list taller than the old cap.
    #[test]
    fn a_long_dub_list_stops_at_the_room_above_the_row() {
        let got = settled(40);
        let (name, [right, bottom, w, h]) = &got[0];
        assert!(name.starts_with("audio root"));
        assert_eq!((*right, *bottom, *w, *h), (1824.0, 764.0, 468.0, 710.0));
        assert_eq!(*bottom - *h, plx_ui::consts::SAFE.y, "the room is what is left under SAFE's top");
    }

    /// Every opener of classic gets the same anchor from the player screen, and it is the control
    /// row's, ending at the segment's edge: that is why the Audio, Subtitles and `…` discs all open
    /// a panel at `CTRL_RIGHT - pw`, bottom 764.
    #[test]
    fn the_classic_anchor_is_the_row_ending_at_the_segment_edge_above_the_gap() {
        let a = crate::player_hud::classic::ctrl_row_hit_rect();
        assert_eq!(a.x + a.w, crate::player_hud::classic::CTRL_RIGHT, "the segment's outer edge");
        assert_eq!(a.y - theme::layout::PLAYER_MENU_GAP, 764.0, "PLAYER_MENU_GAP above the row is classic's bottom edge");
        assert_eq!((a.y, a.h), (crate::player_hud::classic::CTRL_Y, crate::player_hud::classic::CTRL_H), "the discs' own band");
    }

    fn menu_at(anchor: Rect, tab: c_int) -> (TrackMenuState, plx_data::stores::metadata::MetadataStore) {
        let ps = plx_media::route::PlaybackSession::IDLE;
        let store = item(3);
        (TrackMenuState::new(&ps, store.view(), tab, vec!["eng".into()]).with_anchor(anchor), store)
    }

    /// The panel opens beside the anchor it was handed, whichever side that is.
    #[test]
    fn the_panel_opens_beside_the_anchor_it_was_handed() {
        let _g = plx_base::testlock::serial();
        use plx_ui::popover::Side;
        let gap = theme::layout::PLAYER_MENU_GAP;
        // an opener high on the screen: the panel hangs BELOW it, from its right edge
        let high = Rect::new(1500.0, 120.0, 324.0, 64.0);
        let (menu, _store) = menu_at(high, 1);
        let r = menu.panel_rect(&ShippedMeasure);
        assert_eq!(menu.side.get(), Side::Below);
        assert_eq!((r.y, r.x + r.w), (high.y + high.h + gap, high.x + high.w));
        // an opener in the middle of the right edge with no room above or below for NEED_H: to its left
        let mid = Rect::new(1700.0, 560.0, 64.0, 64.0);
        let (menu, _store) = menu_at(mid, 1);
        let r = menu.panel_rect(&ShippedMeasure);
        assert_eq!(menu.side.get(), Side::Start);
        assert_eq!(r.x + r.w, mid.x - gap);
        assert!(plx_ui::consts::SAFE.contains(r.x, r.y) && r.y + r.h <= plx_ui::consts::SAFE.y + plx_ui::consts::SAFE.h);
        plx_plex::plex::reset_servers_for_test();
    }

    /// **The anchor is latched**: the panel's side and its edge by the opener stay put as the page
    /// changes under it (a push, the other tab: both change the width and height), because nothing
    /// re-reads where the opener is.
    #[test]
    fn the_panel_does_not_move_off_its_anchor_while_it_is_open() {
        let _g = plx_base::testlock::serial();
        let (mut menu, store) = menu_at(Rect::new(1500.0, 300.0, 324.0, 64.0), 1);
        let first = menu.panel_rect(&ShippedMeasure);
        let side = menu.side.get();
        assert_eq!(side, plx_ui::popover::Side::Below, "the opener is high: the panel hangs under it");
        let i = menu.form.index_of(&TrackRow::Style).expect("Style row");
        menu.focus_row(i as c_int);
        assert_eq!(menu.on_ok(store.view()), TrackOk::Navigated);
        let pushed = menu.panel_rect(&ShippedMeasure);
        assert_ne!((pushed.w, pushed.h), (first.w, first.h), "the page changed the panel's size");
        assert_eq!(menu.side.get(), side);
        assert_eq!(pushed.x + pushed.w, first.x + first.w, "the edge by the opener");
        assert_eq!(pushed.y, first.y, "and the panel's start (below the opener: its top)");
        plx_plex::plex::reset_servers_for_test();
    }

    /// `NEED_H` is what the side is decided on, so it has to cover the tallest page the menu shows
    /// with app-owned rows, and has to fit classic's room above the row (710) or classic would flip.
    #[test]
    fn need_h_covers_the_app_owned_pages_and_fits_classics_room() {
        let _g = plx_base::testlock::serial();
        let ps = plx_media::route::PlaybackSession::IDLE;
        let store = item(0);
        let mut tallest = 0.0f32;
        for tab in [0, 1] {
            let mut menu = TrackMenuState::new_with(&ps, store.view(), tab, vec!["eng".into()], true);
            tallest = tallest.max(menu.form.table.measured_height());
            if let Some(i) = menu.form.index_of(&TrackRow::Style) {
                menu.focus_row(i as c_int);
                assert_eq!(menu.on_ok(store.view()), TrackOk::Navigated);
                tallest = tallest.max(menu.form.table.measured_height());
            }
        }
        assert!(tallest > 200.0, "something was measured: {tallest}");
        assert!(tallest <= NEED_H, "a page of {tallest} px is taller than NEED_H {NEED_H}");
        assert!(NEED_H <= 764.0 - plx_ui::consts::SAFE.y, "classic's room above the row holds NEED_H");
        plx_plex::plex::reset_servers_for_test();
    }

    /// The overscan audit's rect is the placer's, and sits inside SAFE.
    #[test]
    fn the_overscan_rect_comes_from_the_placer() {
        let mut out = Vec::new();
        overscan_rects(&mut out);
        let r = out[0].1;
        assert_eq!((r.x + r.w, r.y + r.h, r.w), (1824.0, 764.0, plx_ui::table::MENU_MAX_W));
        assert_eq!(r.y, plx_ui::consts::SAFE.y, "the whole room");
    }
}
