Warning: truncated output (original token count: 98203)
Total output lines: 7503

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

    /// The localized name of rung `i` of this field's ladder.
    fn rung_label(self, i: usize) -> &'static str {
        match self {
            Self::Size => subtitle_size_label(SubtitleSize::from_index(i as u8)),
            Self::Position => subtitle_position_label(SubtitlePosition::from_index(i as u8)),
            Self::Color => tone_label(SubtitleTone::from_index(i as u8)),
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
    /// declaration) reads the flavour, not just whether the toggle is on.
    enhance_route: Option<plx_media::route::EnhancementRoute>,
    /// Why the toggle is drawn dim with a reason instead of offered — `Some` only when the owner's
    /// "hidden stays ONLY for no Plex Pass" direction (2026-09-29) applies a plain-language reason
    /// instead of the older silent absence (I1/I2 now covers the Plex-Pass gate alone).
    /// `None` together with [`Self::enhance_shown`]'s `None` means the rows are HIDDEN entirely.
    enhance_disabled: Option<plx_media::route::DisabledReason>,
    /// What is on screen right now (M7) — read alongside [`Self::enhance_route`] purely for the
    /// Audio tab's consequence NOTE, which needs to tell "no subtitle" apart from "an unaffected
    /// sidecar" even though [`enhancement_availability`](plx_media::route::EnhancementAvailability)
    /// folds both into the same [`plx_media::route::EnhancementRoute::Remux`].
    enhance_subtitle_effect: plx_media::route::SubtitleEffect,
    /// The Audio tab's focused row identity, banked across a live rows-VANISH: the enhancement
    /// offer can drop for a poll or two on a route change this menu never asked for (a subtitle
    /// switched on mid-play, a momentary refusal) and return before the viewer presses anything.
    /// [`Self::rebuild_audio`] stashes the focused [`TrackRow::Boost`]/[`TrackRow::Loudness`]
    /// here the moment those rows are about to disappear, and restores it — in preference to
    /// reading `table.sel` back — the moment they reappear. Reading `table.sel` at that point
    /// instead would be wrong: while the rows are gone `table.sel` sits on whatever the fallback
    /// (or the ENGINE's own raw index clamp while the row count was smaller — see
    /// [`TrackMenuPart::reconcile`]) landed on, which names an unrelated track once the enhancement
    /// rows are back. `None` once consumed, or when nothing needs remembering.
    sticky_audio_target: Option<TrackRow>,
    /// The card's resize and the page slide ([`plx_ui::panel_motion`]): the layout target is
    /// cached there, the top/left edges spring to it, and a push or pop slides the two pages.
    motion: PanelMotion,
    /// The owner token of the background queue this menu parked ([`plx_gfx::text::park_prewarm_as_background`]).
    /// [`Drop`] clears the queue only while it is still this menu's: a dismissed menu lives on
    /// through its fade-out (`ModalStack`'s `Closing`), and a Tracks menu reopened inside that
    /// fade has parked its own queue by the time the old one drops.
    background_owner: Option<u64>,
    /// The subtitle search for the playing item, as the host last published it
    /// ([`Self::set_search`]); `None` when the host offers no search, which also hides the root's
    /// Search row. The panel never performs the search: it reads this and answers
    /// [`TrackOk::Search`] for the screen to send.
    search: Option<plx_data::subsearch::SubSearchSnapshot>,
    /// THIS panel watched a download in flight (Sending/Waiting), so an Installed landing is its
    /// own to auto-select. Cleared when handed over ([`Self::take_search_commit`]), so a landing
    /// republished every frame commits once. The store keeps Installed after the panel closes;
    /// a panel opened onto it never saw the flight and must not commit it again — doing so
    /// dismissed every later Subtitles/Audio menu on its first tick (device, 2026-10-04).
    search_watching: bool,
    /// The playing item's external subtitles as of the last open or [`SubSig`] change — what the
    /// Search page checks a hit against ([`Self::hit_on_item`]), because that page is built
    /// without the item in hand. A copy, refreshed wherever the subs list is re-read, so a
    /// subtitle that lands (or is replaced) while the page is up marks its hit at once.
    item_sidecars: Vec<metadata::Stream>,
    /// The opener's rect, handed in when the menu opened ([`Self::with_anchor`]) and never read
    /// from anywhere again: a stand-in taking the HUD row, or a language change, cannot move an
    /// open panel.
    anchor: Rect,
    /// The side of [`Self::anchor`] the panel took, as of the last placement: where its rise
    /// comes from.
    side: std::cell::Cell<plx_ui::popover::Side>,
}

/// **What the track menu DECIDED**, for the loop to perform (spec §2.2).
///
/// The panel owns its rows and its cursor; it does not own the playback, so it may not call
/// `route::commit_audio_pick` / `commit_subtitle_pick` itself — those take the session's
/// `&mut`, and a screen is only ever shown the frame's publication. `None` means the pick changed
/// nothing (audio only: a subtitle OK always commits, so the route republishes, though the panel
/// knows from `active_sub` whether the pick CHANGED the shown subtitle, and only a changed pick
/// carries a `memo`).
///
/// **A track pick also says what to REMEMBER.** `memo` is the descriptor of the row the viewer
/// picked, built here from the exact `metadata::Stream` the row was drawn from
/// (`route::memo_of_audio` / `memo_of_subtitle`, the matchers' own predicates), so the loop can
/// write the show's track memory without ever seeing the item's streams. `None` is a commit that
/// is not the viewer's choice (a harness setup, or a subtitle OK on the row already checked) and
/// remembers nothing; the pick commits write only when the route accepted it.
#[derive(Clone, Debug, PartialEq)]
pub enum TrackCommit {
    /// The frozen `CarriedAudio` snapshot for the picked row (issue #266), built via
    /// `CarriedAudio::from_stream` from the exact `metadata::Stream` the row was drawn from.
    Audio { carried: plx_media::route::CarriedAudio, memo: Option<plx_plex::plex::session::prefs::AudioMemo> },
    /// The Audio tab's Boost dialog / Normalize loudness toggle rows (issue #266): the full
    /// preference after the flip, so the loop's `player::request_audio_enhancement` has both
    /// bits regardless of which row was pressed. Built from [`TrackMenuState::enhance_shown`],
    /// never re-derived from `ps` here — the panel owns its own rows, not the playback (see this
    /// enum's own doc).
    AudioEnhancement(plx_plex::plex::AudioEnhancements),
    /// `sidecar_key` is `Some` when the pick is an EXTERNAL text subtitle the client can draw
    /// on direct play (`metadata::Stream::sidecar_renderable`): it has no demuxer ordinal
    /// (`render_ordinal` is -1), so `route::commit_subtitle_and_sidecar` hands it to `player::sidecar`
    /// beside the unchanged route commit. `sidecar_codec` preserves ASS/SSA on download; a key need not have an
    /// extension. `None` — Off, or an embedded track — deselects any sidecar. `memo` is
    /// `SubtitleMemo::Off` for Off and `Track(..)` for every track, an embedded one, a sidecar or
    /// a fresh download alike; `None` for an OK on the row that is already checked (a row that is
    /// checked because the mode is Forced only is NOT: leaving Forced only is a change, so the
    /// track it drew is a memo of its own when picked on its row).
    Subtitle {
        render_ordinal: c_int,
        stream_id: i64,
        sidecar_key: Option<String>,
        sidecar_codec: String,
        memo: Option<plx_plex::plex::session::prefs::SubtitleMemo>,
    },
    /// **Forced only** (FO): the panel matched the forced track in the
    /// playing audio's language (`render_ordinal`/`stream_id`, `-1`/`0` when the item has none),
    /// and the loop commits it, holds the mode and remembers `SubtitleMemo::ForcedOnly { lang }`
    /// (the drawn track's language, else the audio's, else the language the session fell back to).
    /// `changed` is false for OK on the row that is already the mode: the menu closes, the commit
    /// carries what is on screen unchanged (that stream, or `-1`/`0` for nothing) and nothing is
    /// written. Always an embedded track, never a sidecar: a forced track the client cannot demux is
    /// not matched.
    SubtitleForcedOnly { render_ordinal: c_int, stream_id: i64, lang: String, changed: bool },
    /// The caption's tone. Not a track at all, but it is picked in this panel and it is the
    /// loop that performs it (`player::set_subtitle_tone` saves through the one write door, `route::choose`),
    /// like the two above.
    SubtitleTone(SubtitleTone),
    /// The caption's size, picked on the Style > Size page: the loop publishes it live and
    /// persists it (`route::select_subtitle_size`).
    SubtitleSize(SubtitleSize),
    /// The caption's vertical position, picked on the Style > Position page
    /// (`route::select_subtitle_position`).
    SubtitlePosition(SubtitlePosition),
    /// The caption's timing offset in ms (`player::set_subtitle_offset`) — produced by the Timing
    /// capsule overlay (plan §4), not by this panel: the Timing ROW here only opens that capsule
    /// ([`TrackOk::OpenTiming`]). The variant stays here because `TrackCommit` is the one
    /// player-state-commit type every subtitle control produces, capsule included.
    SubtitleOffset(i64),
}

/// **The whole outcome of OK on the focused row**, one level up from [`TrackCommit`] — what to
/// perform AND whether the panel stays, so the caller reads one value rather than asking twice
/// (once before `on_ok` rebuilt the rows under the cursor). The Timing row hands off to a
/// different overlay (`screens::player::overlay`'s Tracks→Timing transition, plan §4) — a
/// decision this panel can state but not perform, since it does not own the overlay stack.
#[derive(Clone, Debug, PartialEq)]
pub enum TrackOk {
    /// Perform `commit`. `keep_open` is true for a Style pick (the viewer is watching the caption
    /// change, so a run of picks needs no reopen-and-rewalk between them) and for the Audio
    /// toggles; every track pick closes the panel.
    Commit { commit: TrackCommit, keep_open: bool },
    /// A Nav row opened a page ([`TrackMenuState::push`]): the panel stays, its rows replaced.
    Navigated,
    /// Nothing changed (the already-playing audio track): close the panel.
    Dismiss,
    /// Open the Timing capsule overlay: `screens::player::overlay`'s `activate` dismisses the
    /// Tracks panel and asks for `OverlayKind::Timing` in its place.
    OpenTiming,
    /// Open the Custom subtitle-distance capsule after publishing this anchor choice.
    OpenCustomDistance(SubtitlePosition),
    /// OK does nothing and must not close the panel either: the dim Timing row while subtitles are
    /// Off, a locked Timing / Style row, a disabled Size / Position row, and a re-pick of the
    /// rung that is already checked.
    Inert,
    /// A subtitle-search command for the screen to send to the store. The panel stays open: a
    /// search or a download is watched, not fired and forgotten.
    Search(plx_data::subsearch::SubSearchCmd),
}

impl Drop for TrackMenuState {
    /// A closed menu cannot switch tabs, so its other tab's warm ([`Self::warm_other_tab`]) is
    /// work for nobody.
    fn drop(&mut self) {
        if let Some(owner) = self.background_owner {
            plx_gfx::text::clear_background_prewarm_owned(owner);
        }
    }
}

impl TrackMenuState {
    /// Build the menu focused on `tab` (0=Audio, 1=Subtitles) — the on-screen audio/subs icons
    /// pick a specific tab this way; the plain open path passes 0. `yours` is "your languages" in
    /// preference order (pref, playing audio, current subtitle) — see [`Self::yours`].
    pub fn new(
        ps: &plx_media::route::PlaybackSession,
        meta: metadata::MetadataView<'_>,
        tab: c_int,
        yours: Vec<String>,
    ) -> Self {
        Self::new_with(ps, meta, tab, yours, false)
    }

    /// [`Self::new`], with the Subtitles root offering *Search subtitles* from its FIRST build when
    /// `offer_search` — the host can run the search. Offering it later, from a tick, would rebuild
    /// the root under a live panel; a closing panel's refresh is what kept a fading Tracks surface
    /// alive past its hand-off (`tracks_to_timing_hands_off_without_stacking_a_second_surface`).
    pub fn new_with(
        ps: &plx_media::route::PlaybackSession,
        meta: metadata::MetadataView<'_>,
        tab: c_int,
        yours: Vec<String>,
        offer_search: bool,
    ) -> Self {
        let mut s = TrackMenuState {
            tab,
            active_audio: 0,
            active_sub: -1,
            forced_only: false,
            forced_lang: String::new(),
            form: TrackTable::new(BAND_BASE),
            offset_ms: plx_media::player::subtitle_offset_ms(),
            tone: plx_media::player::subtitle_tone(),
            size: plx_media::route::subtitle_size(),
            position: plx_media::route::subtitle_position(),
            pages: PageStack::new(),
            renderer: SubRenderer::Text,
            sub_sig: None,
            other: Vec::new(),
            root_tracks: Vec::new(),
            yours,
            enhance_shown: None,
            enhance_route: None,
            enhance_disabled: None,
            enhance_subtitle_effect: plx_media::route::SubtitleEffect::None,
            sticky_audio_target: None,
            motion: PanelMotion::new(),
            background_owner: None,
            search: offer_search.then(|| plx_data::subsearch::SubSearchSnapshot::idle(0)),
            search_watching: false,
            item_sidecars: Vec::new(),
            anchor: crate::player_hud::classic::ctrl_row_hit_rect(),
            side: std::cell::Cell::new(plx_ui::popover::Side::Above),
        };
        s.form.table.min_panel_w = theme::layout::PLAYER_MENU_MIN_W;
        s.sync_item(ps, meta);
        s.rebuild(ps, meta, tab, false);
        s
    }

    /// Open beside `anchor`, the rect of the control (widened to the end of its segment) that
    /// opened the menu, as the player screen hands it over. Without it the menu opens where classic
    /// does: the control row's rect, whose right edge is `CTRL_RIGHT`. Called once, on the
    /// freshly built menu and before its first frame: the placement is cached against the table's
    /// layout, and the anchor is latched for the menu's whole life.
    pub fn with_anchor(mut self, anchor: Rect) -> Self {
        self.anchor = anchor;
        self
    }

    /// The anchor the menu was opened with ([`Self::with_anchor`]).
    pub fn anchor(&self) -> Rect {
        self.anchor
    }

    /// The highlighted row, for the focus probe (`crate::focusprobe`) — a READ of the cursor the
    /// key ladder moves, and the reason it exists: `app.rs`'s UP/DOWN arm for this panel changes
    /// nothing else, so without this the fingerprint records the panel opening and closing and
    /// nothing between.
    pub fn sel(&self) -> i32 {
        self.form.table.sel
    }

    /// **The replay canon** of this panel: the tab, the page path (each pushed page and the row
    /// that opened it) and the selected row's [`RowKey`] — not its index, which moves when a row
    /// is added above it. `screens::player::overlay` writes this in place of the bare row index, so
    /// a replay tells Subtitles from Audio, the root from a Style page, and two return stacks
    /// apart.
    pub fn canon(&self, c: &mut Canon) {
        c.u32(self.tab as u32);
        self.pages.canon(c, TrackPage::code, |r| r.key().0);
        c.u32(self.form.key_at(self.form.table.sel.max(0) as usize).map_or(u32::MAX, |k| k.0));
    }

    /// What the `submenuosc` trigger needs to choose its next key: the tab (0 Audio, 1 Subtitles),
    /// how many pages are pushed above the root, and whether the root offers Other languages.
    pub fn osc_probe(&self) -> (c_int, usize, bool) {
        (self.tab, self.pages.len(), self.form.index_of(&TrackRow::OpenOther).is_some())
    }

    /// The pages pushed above the Subtitles root, outermost first, for tests and probes.
    #[cfg(any(test, feature = "test-support"))]
    pub fn page_path(&self) -> Vec<TrackPage> {
        self.pages.iter().map(|s| s.page).collect()
    }

    /// Was the Subtitles root last built with the server drawing the subtitle (what locks Timing and Style)?
    #[cfg(any(test, feature = "test-support"))]
    pub fn server_burn_built(&self) -> bool {
        self.sub_sig.as_ref().is_some_and(|sig| sig.server_burn().is_some())
    }

    /// The highlighted row's id (`None` on a note or an empty list).
    #[cfg(any(test, feature = "test-support"))]
    pub fn selected_id(&self) -> Option<TrackRow> {
        self.form.selected_id().copied()
    }

    /// The focusable rows at the current tab, in drawn order, by identity — for a test that needs
    /// to name a row without duplicating this layout by hand (`screens::player::overlay_tests`'s
    /// Timing hand-off test).
    #[cfg(any(test, feature = "test-support"))]
    pub fn ids(&self) -> Vec<TrackRow> {
        (0..self.form.table.n_rows() as usize).filter_map(|i| self.form.id_at(i).copied()).collect()
    }

    /// Every focusable row's focus element, in drawn order.
    #[cfg(any(test, feature = "test-support"))]
    pub fn keys(&self) -> Vec<u32> {
        self.ids().iter().map(|id| id.key().0).collect()
    }

    /// The focus element (a row's [`RowKey`] number) of the row `id` names, if this tab has it.
    #[cfg(any(test, feature = "test-support"))]
    pub fn key_of(&self, id: TrackRow) -> Option<u32> {
        self.form.index_of(&id).map(|_| id.key().0)
    }

    /// Move focus onto the row `id` names (a test's, or the `submenuosc` trigger's, way of
    /// pressing DOWN to it); `false` when this tab has no such row.
    pub fn focus_id(&mut self, id: TrackRow) -> bool {
        self.focus_key(id.key().0);
        self.form.selected_id() == Some(&id)
    }

    /// **Write back the engine's own focus cursor** (restructure phase 12): the Column group
    /// [`TrackMenuPart`] answers is the source of geometry, but the ENGINE owns the current
    /// element (§7.3 step 5) — the owner's `step` is the only place that mutates in response to a
    /// `FocusMoved`, and this is `screens::player::overlay::PlayerOverlayScreen::step`'s write.
    /// The element is a row's [`RowKey`] number; a key this tab does not know moves nothing.
    pub fn focus_key(&mut self, elem: u32) {
        if let Some(i) = self.form.index_of_key(RowKey(elem)) {
            self.form.table.sel = i as i32;
        }
    }

    /// index into the playing item's audio list of the chosen audio track
    pub fn active_audio(&self) -> c_int {
        self.active_audio
    }
    /// -1 = subtitles off, else index into the playing item's subs list. Under Forced only, the
    /// forced track it drew (-1 when none matched); [`Self::forced_only`] says which mode it is.
    pub fn active_sub(&self) -> c_int {
        self.active_sub
    }
    /// Is Forced only the subtitle mode (the checkmark is on its row)?
    pub fn forced_only(&self) -> bool {
        self.forced_only
    }
    /// The track the track rows check: the active one, or none while Forced only holds the
    /// checkmark. Every row that checks a track reads this, never `active_sub` directly.
    fn checked_sub(&self) -> c_int {
        if self.forced_only { -1 } else { self.active_sub }
    }
    /// Plex stream id of the chosen audio track (for &audioStreamID), or 0
    pub fn audio_stream_id(&self, meta: metadata::MetadataView<'_>) -> i64 {
        let i = self.active_audio();
        tracks(meta)
            .and_then(|t| t.audio.get(i.max(0) as usize))
            .map(|s| s.id)
            .unwrap_or(0)
    }
    /// Plex stream id of the chosen subtitle track (for &subtitleStreamID), or 0 if Off
    pub fn sub_stream_id(&self, meta: metadata::MetadataView<'_>) -> i64 {
        let i = self.active_sub();
        if i < 0 {
            return 0;
        }
        tracks(meta)
            .and_then(|t| t.subs.get(i as usize))
            .map(|s| s.id)
            .unwrap_or(0)
    }

    /// Derive the checked tracks from the PLAYBACK state — the route owns the truth
    /// (CUR_AUDIO_SID/CUR_SUB_SID, set by the start-of-play pick and every commit): the auto-picked
    /// default/smart-DP track is checked on first open, a replayed item resets with the playback,
    /// and a prior pick round-trips by id. When no id is recorded (codec-default play), the file's
    /// flagged default is checked. Free function (no `&self`) so both [`Self::sync_item`] (on
    /// every open) and the live poll below (PR #309 field report) derive the SAME pair the same
    /// way — the desync that report caught was exactly two readers of this answer drifting apart.
    fn derive_active(ps: &plx_media::route::PlaybackSession, meta: metadata::MetadataView<'_>) -> (c_int, c_int) {
        (Self::derive_active_audio(ps, meta), Self::derive_active_sub(ps, meta))
    }

    /// The Audio tab's half of [`Self::derive_active`].
    fn derive_active_audio(ps: &plx_media::route::PlaybackSession, meta: metadata::MetadataView<'_>) -> c_int {
        let Some(t) = tracks(meta) else { return 0 };
        let asid = plx_media::route::cur_audio_sid(ps);
        (asid > 0)
            .then(|| t.audio.iter().position(|s| s.id == asid))
            .flatten()
            .or_else(|| t.audio.iter().position(|s| s.default))
            .unwrap_or(0) as c_int
    }

    /// The Subtitles tab's half of [`Self::derive_active`] — what the live poll scans alone, the
    /// audio half being discarded there.
    fn derive_active_sub(ps: &plx_media::route::PlaybackSession, meta: metadata::MetadataView<'_>) -> c_int {
        let Some(t) = tracks(meta) else { return -1 };
        let ssid = plx_media::route::cur_sub_sid(ps);
        (ssid > 0)
            .then(|| t.subs.iter().position(|s| s.id == ssid))
            .flatten()
            .map(|i| i as c_int)
            .unwrap_or(-1)
    }

    /// [`Self::derive_active`] on every open — the menu can never show a stale or desynced
    /// checkmark at the moment it appears. Deliberately does NOT touch `tab`:
    /// [`TrackMenuState::new`] sets it directly.
    fn sync_item(&mut self, ps: &plx_media::route::PlaybackSession, meta: metadata::MetadataView<'_>) {
        let (audio, sub) = Self::derive_active(ps, meta);
        self.active_audio = audio;
        self.active_sub = sub;
        self.forced_only = Self::derive_forced_only(ps);
        self.forced_lang = plx_media::route::forced_only_lang(ps).to_string();
        self.read_item_sidecars(meta);
    }

    /// Is the playback in Forced only? The route owns that fact (`cur_sub_mode`): the drawn forced
    /// track's stream id alone cannot tell it from a pick of that same track.
    fn derive_forced_only(ps: &plx_media::route::PlaybackSession) -> bool {
        plx_media::route::cur_sub_mode(ps) == plx_media::route::SubMode::ForcedOnly
    }

    /// The language the Forced only row reads out while it is the mode: the drawn track's, else the
    /// playing audio's ([`sub_layout::forced_only_readout`]); for audio that names no language, the
    /// one the mode fell back to, named by a track of the item that carries it. `None` off the mode.
    fn forced_readout(
        ps: &plx_media::route::PlaybackSession,
        meta: metadata::MetadataView<'_>,
        active: c_int,
        forced: bool,
    ) -> Option<String> {
        forced.then(|| {
            let item = tracks(meta);
            let drawn = item.and_then(|t| t.subs.get(usize::try_from(active).ok()?));
            let audio = item.and_then(|t| t.audio.get(usize::try_from(Self::derive_active_audio(ps, meta)).ok()?));
            let audio_lang = audio.map_or("", |a| a.lang_code.as_str());
            let language = plx_media::route::forced_only_language(audio_lang, plx_media::route::forced_only_lang(ps));
            // the fallback stood in for audio that names none: no stream of that language need be
            // drawn, so it is named by any track of the item that carries it
            let stand_in = (drawn.is_none() && language != audio_lang)
                .then(|| item.and_then(|t| t.subs.iter().chain(&t.audio).find(|s| metadata::lang_matches(language, &s.lang_code))))
                .flatten();
            sub_layout::forced_only_readout(drawn, stand_in.or(audio))
        })
    }

    /// Re-read the playing item's external subtitles for the Search page ([`Self::item_sidecars`]).
    fn read_item_sidecars(&mut self, meta: metadata::MetadataView<'_>) {
        self.item_sidecars =
            tracks(meta).map(|t| t.subs.iter().filter(|s| s.external).cloned().collect()).unwrap_or_default();
    }

    /// The Subtitles tab's half of the live poll `Self::update` runs every tick, mirroring the
    /// Audio tab's own `enh_state` poll just above it. Issue #309's field report: a subtitle pick
    /// that reroutes the play to (or away from) the enhancement's own Burn lands `active_sub`
    /// at once (`Self::on_ok`'s own optimistic write), but the server-burn lock (`server_burn_built`) can only become true
    /// once the Burn's `/decision` round trip actually answers (`route::decision::request_retranscode`
    /// on a `route::flight` worker, a real network call) — seconds later. A panel that stays open across that window (the
    /// diagnostic `screens::player::overlay::pick_track_row` trigger deliberately does, "so a
    /// capture can show the picked track") was built and never touched again, so its drawn
    /// checkmark and its Style/Timing dim state both kept whatever `Self::layout` baked at open,
    /// disagreeing with the route by the time a capture actually looked at it. `rebuild`'s own
    /// subtitle arm always re-homes the cursor onto the checked row — correct for an open or a tab
    /// switch, wrong for a background poll that must not steal focus from wherever the viewer's
    /// cursor actually is (the exact focus-desync class `refresh_audio`'s doc names)
    /// — so this refreshes the form, which restores the current row by [`TrackRow`] in the
    /// freshly built list instead of snapping to the active track, the same fix `refresh_audio` applies for the Audio
    /// tab's own poll.
    ///
    /// **It rebuilds on a [`SubSig`] change**, not on the two values it once compared: the subs
    /// list, the active index, the renderer kind, transcoding, and the enhancement route and
    /// subtitle effect. On the root that is a refresh in place. ON A SUB-PAGE the page is refreshed in
    /// place too (focus kept by id) and popped to the root only when its availability no longer
    /// holds ([`SubSig::page_availability`]: the renderer, or whether Style is shown / locked), so a
    /// Style page never outlives the renderer or the lock its Size and Position rows were built for
    /// while a track list or enhancement change it does not read leaves it alone.
    fn poll_subtitle_state(&mut self, ps: &plx_media::route::PlaybackSession, meta: metadata::MetadataView<'_>) {
        let live_sub = Self::derive_active_sub(ps, meta);
        let live_forced = Self::derive_forced_only(ps);
        let live_lang = plx_media::route::forced_only_lang(ps);
        if self.forced_lang != live_lang {
            self.forced_lang = live_lang.to_string();
        }
        let sig = self.sub_sig_for(ps, meta, live_sub, Self::forced_readout(ps, meta, live_sub, live_forced));
        if self.sub_sig.as_ref() == Some(&sig) {
            return;
        }
        self.active_sub = live_sub;
        self.forced_only = live_forced;
        self.read_item_sidecars(meta);
        let Some(first) = self.pages.first().copied() else {
            let form = self.layout(ps, meta);
            // not the viewer's row any more (its track left the offered list): the checked row
            self.form.refresh_with(form, None, Some(&self.active_sub_id()));
            return;
        };
        // the pages' own data is read BEFORE judging them: a language whose entry is gone is as
        // unavailable as a changed renderer
        self.other = self.sub_model(ps, meta, true).other;
        let availability_moved =
            self.sub_sig.as_ref().is_some_and(|built| built.page_availability() != sig.page_availability());
        if availability_moved || !self.pages_hold() {
            // The page was opened on a root that no longer holds (the renderer changed under
            // Size/Position, a burn landed or left, the language it lists left the offered list):
            // pop to the root, restoring the opener and scroll the stack saved at its first push.
            self.pages.clear();
            let form = self.layout(ps, meta);
            self.form.restore(form, Some(&first.return_id), first.scroll);
            self.form.table.set_title(None);
            self.motion.cancel_slide();
        } else {
            // The page still holds: remember what the root is now built from (the pop back
            // rebuilds it) and refresh this page in place, focus kept by id.
            self.sub_sig = Some(sig);
            if let Some(page) = self.pages.top() {
                let form = self.page_form(page);
                // a language can change shape under the viewer (one track <-> several): the row
                // they were on follows it to its new one
                let prefer = (page == TrackPage::OtherLanguages)
                    .then(|| self.form.selected_id().map(|id| self.other_row_for(*id)))
                    .flatten();
                self.form.refresh_with(form, prefer.as_ref(), None);
            }
        }
    }

    /// Does every pushed page still have what it lists? An Other languages page needs a language
    /// to list; a language page needs ITS language (the same [`LangId`], whatever its tracks'
    /// positions are now) still offered, with however many tracks it has left.
    fn pages_hold(&self) -> bool {
        self.pages.iter().all(|saved| match saved.page {
            TrackPage::OtherLanguages => !self.other.is_empty(),
            TrackPage::Language(stream) => self.other_lang(stream).is_some(),
            TrackPage::Style | TrackPage::Picker(_) => true,
            // its listing is the search's, not the item's: the download's own stream landing must
            // not pop the page the viewer is watching it on
            TrackPage::Search | TrackPage::SearchLanguage => self.search.is_some(),
        })
    }

    /// The Subtitles root's rebuild signature for `active`, read fresh off the route and the
    /// item. [`Self::layout`] stores the same value it builds from.
    fn sub_sig_for(
        &self,
        ps: &plx_media::route::PlaybackSession,
        meta: metadata::MetadataView<'_>,
        active: c_int,
        forced_only: Option<String>,
    ) -> SubSig {
        let item = tracks(meta);
        let offered = visible_subs(ps, meta);
        let subs = item
            .map(|t| t.subs.iter().enumerate().map(|(i, s)| (s.id, offered.contains(&i))).collect())
            .unwrap_or_default();
        let renderer = item
            .and_then(|t| t.subs.get(usize::try_from(active).ok()?))
            .map_or(SubRenderer::Text, |s| SubRenderer::of_codec(&s.codec));
        let (_, enhancement, _, effect) = Self::enh_state(ps);
        SubSig {
            subs,
            active,
            renderer,
            transcoding: plx_media::route::is_transcoding(ps),
            presenter: plx_media::route::subtitle_presenter(ps),
            enhancement,
            effect,
            memory_note: plx_media::route::cur_memory_note(ps),
            forced_only,
        }
    }

    /// The language entry of the Other languages model that [`LangId::stream`] names.
    fn other_lang(&self, stream: i64) -> Option<&OtherLang> {
        self.other.iter().find(|o| o.id.stream == stream)
    }

    /// The row that stands for a language on the Other languages page NOW: its direct pick row
    /// while it offers one track, its drill-in while it offers several. A language changes shape
    /// under a viewer (a sidecar becomes offered or leaves), and the row they were on must follow
    /// it, not be looked up in a shape that no longer exists. Any other id is returned unchanged.
    fn other_row_for(&self, id: TrackRow) -> TrackRow {
        let lang = match id {
            TrackRow::OpenLang(l) => self.other_lang(l.stream),
            TrackRow::Sub(i) => self.other.iter().find(|o| o.tracks.iter().any(|t| t.i == i)),
            _ => None,
        };
        lang.map_or(id, Self::other_lang_row)
    }

    /// A language's row on the Other languages page: the pick row of its only track, or its
    /// drill-in.
    fn other_lang_row(o: &OtherLang) -> TrackRow {
        match o.tracks.as_slice() {
            [t] => TrackRow::Sub(t.i),
            _ => TrackRow::OpenLang(o.id),
        }
    }

    /// The Subtitles ROOT's row carrying the checkmark: Forced only while it is the mode, else the
    /// active track, or Off — or, when the active track is not listed on the root at all (its
    /// language is not "yours": a codeless or unrecognised-language track, or one a live change
    /// moved), the Other languages row that stands for it.
    fn active_sub_id(&self) -> TrackRow {
        if self.forced_only {
            return TrackRow::SubForcedOnly;
        }
        match self.active_sub {
            i if i >= 0 && self.other.iter().any(|o| o.tracks.iter().any(|t| t.i == i as usize)) => TrackRow::OpenOther,
            i if i >= 0 => TrackRow::Sub(i as usize),
            _ => TrackRow::SubOff,
        }
    }

    /// Focus an ABSOLUTE table row — the /tmp/plxnative-menupick trigger's contract ("row N"), the
    /// one dev-only entry that speaks positions (the named targets below do not). The interactive
    /// path always moves relatively; this exists because the initial focus is the ACTIVE row
    /// (derived from playback state), so a relative walk from it would land elsewhere.
    pub fn focus_row(&mut self, row: c_int) {
        for _ in 0..64 {
            if self.form.table.sel == row {
                break;
            }
            let before = self.form.table.sel;
            self.form.table.move_sel(if self.form.table.sel < row { 1 } else { -1 });
            if self.form.table.sel == before {
                break; // clamped at an end — row out of range
            }
        }
    }

    /// Resolve a NAMED Audio-tab target (`"boost"`/`"loudness"`) to its absolute table row, for
    /// the `/tmp/plxnative-menupick` trigger's named form — an alternative to a row number hand-
    /// derived from the item's track count, which is exactly the issue #266 PR4 bug: this harness
    /// once hardcoded the Normalize Loudness row from a WRONG assumed track count. Reading it back
    /// through the form, the same identities [`Self::on_ok`] dispatches on, means the name
    /// is correct however many tracks the item actually has. `None` when `name` is unrecognized,
    /// or recognized but not currently built (the DSP toggle rows are not offered right now).
    pub fn row_for_audio_target(&self, name: &str) -> Option<c_int> {
        let target = match name {
            "boost" => TrackRow::Boost,
            "loudness" => TrackRow::Loudness,
            _ => return None,
        };
        self.form.index_of(&target).map(|i| i as c_int)
    }

    /// Resolve a NAMED Subtitles-tab target to a TRACK, for the `/tmp/plxnative-menupick` trigger:
    /// `"track:N"` is the N-th (0-based) track in PAGE ORDER — the root's track rows first, then the
    /// Other languages page A-Z with a multi-track language expanded into its own ranked page —
    /// never Off, Timing or Style. It answers the track's index in the item's list, not a table
    /// row: a track behind Other languages has no row on the page that is showing, and
    /// [`Self::commit_sub_track`] commits by that index. A hand-written row number drifts every
    /// time the panel gains or loses a row (the `subtitle_text_srt` case picked row 3, which became
    /// the Style row); a position in this order cannot. `None` for an unrecognized name or an N past
    /// the last track.
    pub fn sub_track_for_target(&self, name: &str) -> Option<usize> {
        let n: usize = name.strip_prefix("track:")?.trim().parse().ok()?;
        self.root_tracks.iter().copied().chain(self.other.iter().flat_map(|o| o.tracks.iter().map(|t| t.i))).nth(n)
    }

    /// Show `tab` (0=Audio, 1=Subtitles) on a menu that is ALREADY open — the second disc pressed
    /// while the first one's tab is showing. Same body as the LEFT/RIGHT arm below, which is why
    /// that arm calls this rather than repeating it.
    pub fn focus_tab(&mut self, ps: &plx_media::route::PlaybackSession, meta: metadata::MetadataView<'_>, tab: c_int) {
        if tab != self.tab {
            self.tab = tab;
            self.rebuild(ps, meta, tab, false); // swap the whole list → snap the pill, no long glide
        }
    }

    /// commit the focused row as the active track for its tab — dismissing the panel afterward is
    /// the container's job now, not this method's; the answer says whether it should. The row is
    /// read back by its [`TrackRow`] (never by position), and a row the form declared DISABLED (the
    /// dim Timing/Style under a live burn, a Size/Position row the renderer cannot reach) is inert
    /// at the form layer.
    pub fn on_ok(&mut self, meta: metadata::MetadataView<'_>) -> TrackOk {
        let tab = self.tab;
        let sel = self.form.table.sel;
        let focused = self.form.selected_id().copied();
        if tab == 0 {
            // The two Plex Pass DSP rows (issue #266), after the audio tracks — see
            // `Self::audio_form`, which declares them beside the tracks, so a track pick below is
            // never mistaken for one of these by position arithmetic.
            return match focused {
                // A `Disabled` pair is drawn dim, reading Off, and OK on it is a no-op — same
                // "focusable but inert" shape `TrackRow::Timing` already uses while subtitles are
                // Off. (The footnote under the pair is an inert slot: no key, never focused.)
                Some(TrackRow::Boost | TrackRow::Loudness) if self.enhance_disabled.is_some() => {
                    TrackOk::Inert
                }
                target @ (Some(TrackRow::Boost) | Some(TrackRow::Loudness)) => {
                    let mut a = self.enhance_shown.unwrap_or(plx_plex::plex::AudioEnhancements::NONE);
                    if target == Some(TrackRow::Boost) {
                        a.boost_dialog = !a.boost_dialog;
                    } else {
                        a.normalize_loudness = !a.normalize_loudness;
                    }
                    self.enhance_shown = Some(a);
                    if let Some(row) = self.form.table.row_mut(sel) {
                        row.toggle = Some(if target == Some(TrackRow::Boost) {
                            a.boost_dialog
                        } else {
                            a.normalize_loudness
                        });
                    }
                    plx_telemetry::diag::event(plx_telemetry::diag::schema::DiagEvent::FeatureUsed {
                        feature: plx_telemetry::diag::schema::Feature::AudioEnhancement,
                    });
                    TrackOk::Commit { commit: TrackCommit::AudioEnhancement(a), keep_open: true }
                }
                Some(TrackRow::Audio(i)) => {
                    let pick = i as c_int;
                    let changed = self.active_audio != pick;
                    self.active_audio = pick;
                    if changed {
                        // the menu only reports the pick — native-switch vs re-transcode is
                        // route's policy. The demuxer-facing index is the CONTAINER ordinal
                        // (audio_ordinal), not the row.
                        if let Some(s) = tracks(meta).and_then(|t| t.audio.get(i)) {
                            let ord = tracks(meta)
                                .map(|t| metadata::audio_ordinal(&t.audio, i))
                                .unwrap_or(pick);
                            plx_telemetry::diag::event(plx_telemetry::diag::schema::DiagEvent::FeatureUsed {
                                feature: plx_telemetry::diag::schema::Feature::AudioTrack,
                            });
                            return TrackOk::Commit {
                                commit: TrackCommit::Audio {
                                    carried: plx_media::route::CarriedAudio::from_stream(s, ord),
                                    memo: tracks(meta).and_then(|t| plx_media::route::memo_of_audio(&t.audio, i)),
                                },
                                keep_open: false,
                            };
                        }
                    }
                    TrackOk::Dismiss
                }
                _ => TrackOk::Dismiss,
            };
        }

        let Some(id) = focused else { return TrackOk::Inert };
        match self.form.activate(sel.max(0) as usize) {
            // M7 follow-up: while the live route is actually burning a subtitle into the picture,
            // Timing and Style are declared disabled (`Self::layout`) — the text is already in
            // the pixels — and so are Size/Position under an image or styled subtitle: OK on any
            // of them is a no-op.
            None => return TrackOk::Inert,
            Some(Activation::Push(dest)) => {
                self.push(dest);
                return TrackOk::Navigated;
            }
            Some(Activation::Action(())) => {}
        }
        match id {
            TrackRow::Choice(field, rung) => self.pick_style(field, rung),
            // Read LIVE, not from the form: a pick in this same open (`active_sub` written below,
            // no rebuild) must make the next OK on Timing open the capsule.
            TrackRow::Timing if self.active_sub >= 0 => TrackOk::OpenTiming,
            TrackRow::Timing => TrackOk::Inert, // dim and inert while subtitles are Off
            TrackRow::SubOff => self.commit_sub(-1, meta),
            TrackRow::SubForcedOnly => self.commit_forced_only(meta),
            TrackRow::Sub(i) => self.commit_sub(i as c_int, meta),
            TrackRow::SearchHit(i) => self.search_download(i),
            TrackRow::SearchRetry => TrackOk::Search(plx_data::subsearch::SubSearchCmd::Retry),
            TrackRow::LangChoice(i) => self.pick_search_language(i),
            _ => TrackOk::Inert,
        }
    }

    /// Commit the subtitle track at `i` in the item's list as the active one, exactly as OK on its
    /// row does ([`Self::on_ok`]): by the track's own index, wherever its row sits (the root, the
    /// Other languages page or a language page), so a caller that names a track never needs the row
    /// to be on the current page.
    pub fn commit_sub_track(&mut self, i: usize, meta: metadata::MetadataView<'_>) -> TrackOk {
        self.commit_sub(i as c_int, meta)
    }

    /// The subtitle commit behind OK on Off (`-1`) or on a track row: records the pick, and answers
    /// the commit. The commit is made whether or not the pick changed anything (the route
    /// republishes), but it carries a memo only when it DID: the panel opens on the checked row, so
    /// an OK there is closing the menu, not choosing, and an automatic pick (the ladder's Off, or
    /// its track) must not become the show's memory.
    fn commit_sub(&mut self, new_sub: c_int, meta: metadata::MetadataView<'_>) -> TrackOk {
        // Leaving Forced only is a change even onto the stream it drew (the forced track's own row)
        // or onto nothing (Off while no forced track matched): the index is the same, the state is
        // not.
        let changed = plx_media::route::subtitle_pick_changed(self.active_sub, new_sub, self.forced_only);
        self.active_sub = new_sub;
        self.forced_only = false;
        // the client renderer takes the EMBEDDED-subtitle ordinal (what the demuxer
        // enumerates); an external pick has no demux ordinal — it is drawn by the sidecar
        // renderer on direct play, or burned
        let ridx = tracks(meta)
            .filter(|_| new_sub >= 0)
            .map(|t| metadata::sub_render_ordinal(&t.subs, new_sub as usize))
            .unwrap_or(-1);
        if changed {
            plx_telemetry::diag::event(plx_telemetry::diag::schema::DiagEvent::FeatureUsed {
                feature: plx_telemetry::diag::schema::Feature::SubtitleTrack,
            });
        }
        let sidecar = tracks(meta)
            .filter(|_| new_sub >= 0)
            .and_then(|t| t.subs.get(new_sub as usize))
            .filter(|s| s.sidecar_renderable());
        TrackOk::Commit {
            commit: TrackCommit::Subtitle {
                render_ordinal: ridx,
                stream_id: self.sub_stream_id(meta),
                sidecar_key: sidecar.map(|s| s.key.clone()),
                sidecar_codec: sidecar.map(|s| s.codec.clone()).unwrap_or_default(),
                // Off is a choice of its own, remembered as Off; a track is remembered by what it
                // IS. A row with no stream behind it remembers nothing, and neither does a pick
                // that changed nothing.
                memo: if !changed {
                    None
                } else if new_sub < 0 {
                    Some(plx_plex::plex::session::prefs::SubtitleMemo::Off)
                } else {
                    tracks(meta)
                        .and_then(|t| t.subs.get(new_sub as usize))
                        .map(plx_media::route::memo_of_subtitle)
                },
            },
            keep_open: false,
        }
    }

    /// **OK on Forced only**: match the forced track for the audio that plays, commit it (or
    /// nothing, when the item has none) and hold the mode. The match is the route's own
    /// ([`plx_media::route::forced_only_pick`]) over the container's names, in the language the
    /// resolve would use: the audio's, else the one the session fell back to ([`Self::forced_lang`]).
    /// So the row and the next episode choose by one rule. The language remembered is the drawn
    /// track's, else that same language.
    ///
    /// **A pick of the row that is already the mode is not a choice**: the menu opens on the checked
    /// row, so OK there is how it is dismissed. It recommits what the route shows (that stream, or
    /// nothing), unchanged and with `changed: false`, and matches nothing again: a re-match could
    /// find no track where the resolve drew one (untagged audio drawn through the fallback), and
    /// turn the subtitle off.
    fn commit_forced_only(&mut self, meta: metadata::MetadataView<'_>) -> TrackOk {
        let changed = !self.forced_only;
        let item = tracks(meta);
        let audio = item.and_then(|t| t.audio.get(usize::try_from(self.active_audio).ok()?));
        let audio_lang = audio.map_or("", |a| a.lang_code.as_str());
        let picked = if changed {
            item.and_then(|t| {
                let names = plx_media::player::SHARED.track_names_lenient();
                plx_media::route::forced_only_pick(&t.subs, &names, audio_lang, &self.forced_lang)
            })
        } else {
            usize::try_from(self.active_sub).ok()
        };
        let drawn = item.and_then(|t| {
            let i = picked?;
            Some((i, metadata::sub_render_ordinal(&t.subs, i), t.subs.get(i)?))
        });
        let lang = drawn
            .map_or_else(|| plx_media::route::forced_only_language(audio_lang, &self.forced_lang), |(.., s)| s.lang_code.as_str())
            .to_string();
        self.active_sub = drawn.map_or(-1, |(i, ..)| i as c_int);
        self.forced_only = true;
        if changed {
            self.forced_lang = lang.clone();
            plx_telemetry::diag::event(plx_telemetry::diag::schema::DiagEvent::FeatureUsed {
                feature: plx_telemetry::diag::schema::Feature::SubtitleTrack,
            });
        }
        TrackOk::Commit {
            commit: TrackCommit::SubtitleForcedOnly {
                render_ordinal: drawn.map_or(-1, |(_, ordinal, _)| ordinal),
                stream_id: drawn.map_or(0, |(.., s)| s.id),
                lang,
                changed,
            },
            keep_open: false,
        }
    }

    /// A Style picker's pick: the field's new rung, committed live, the panel and page staying so a
    /// run of picks is felt at once. The page's checkmark moves by refreshing it in place (scroll
    /// and focus kept). The already-checked rung is inert: nothing changes, nothing is written.
    fn pick_style(&mut self, field: StyleField, rung: usize) -> TrackOk {
        let commit = match field {
            StyleField::Size => {
                let size = SubtitleSize::from_index(rung as u8);
                if size == self.size {
                    return TrackOk::Inert;
                }
                self.size = size;
                TrackCommit::SubtitleSize(size)
            }
            StyleField::Position => {
                // `Custom` is an action in the player picker, not a default anchor. Its distance
                // is stored separately from the profile's default position.
                let position = match SubtitlePosition::from_index(rung as u8) {
                    SubtitlePosition::Custom(_) => SubtitlePosition::Custom(
                        plx_media::route::subtitle_custom_distance(),
                    ),
                    fixed => fixed,
                };
                if position == self.position && position.custom_gap().is_none() {
                    return TrackOk::Inert;
                }
                self.position = position;
                if position.custom_gap().is_some() {
                    return TrackOk::OpenCustomDistance(position);
                }
                TrackCommit::SubtitlePosition(position)
            }
            StyleField::Color => {
                let tone = SubtitleTone::from_index(rung as u8);
                if tone == self.tone {
                    return TrackOk::Inert;
                }
                self.tone = tone;
                TrackCommit::SubtitleTone(tone)
            }
        };
        if let Some(page) = self.pages.top() {
            let form = self.page_form(page);
            self.form.refresh(form);
        }
        TrackOk::Commit { commit, keep_open: true }
    }

    /// The rung of `field` the panel currently reads out and checks.
    fn current_rung(&self, field: StyleField) -> usize {
        let rung = match field {
            StyleField::Size => self.size.index(),
            StyleField::Position => self.position.index(),
            StyleField::Color => self.tone.index(),
        };
        rung as usize
    }

    /// The form of a pushed page, from the panel's own read-outs.
    fn page_form(&self, page: TrackPage) -> TrackForm {
        match page {
            TrackPage::Style => self.style_form(),
            TrackPage::Picker(field) => self.picker_form(field),
            TrackPage::OtherLanguages => self.other_form(),
            TrackPage::Language(stream) => self.language_form(stream),
            TrackPage::Search => self.search_form(),
            TrackPage::SearchLanguage => self.search_language_form(),
        }
    }

    /// The Other languages page: one row per language, A-Z. A single-track language is a direct
    /// pick row (checked when it is the active track); a multi-track one is a drill-in reading out
    /// its track count, or — when the active track is inside it — a check and that variant.
    fn other_form(&self) -> TrackForm {
        let active = self.checked_sub();
        let sec = self.other.iter().fold(FormSection::new(""), |sec, lang| match lang.tracks.as_slice() {
            [t] => sec.item(TrackRow::Sub(t.i), RowKind::Choice, (), flat_row(t, active)),
            tracks => {
                let current = tracks.iter().find(|t| active >= 0 && t.i == active as usize);
                let value = match current {
                    Some(t) => in_lang_label(t),
                    None => plx_platform::i18n::msg::widgets_tracks_count(tracks.len() as i64),
                };
                let row = Row::new(lang.name.clone()).value(value).checked(current.is_some());
                sec.item(TrackRow::OpenLang(lang.id), RowKind::Nav(TrackPage::Language(lang.id.stream)), (), row)
            }
        });
        Form::new().section(sec)
    }

    /// A language page: its tracks ranked full < SDH < forced < commentary, a pick row each.
    fn language_form(&self, stream: i64) -> TrackForm {
        let active = self.checked_sub();
        let tracks = self.other_lang(stream).map(|o| o.tracks.as_slice()).unwrap_or_default();
        let sec = tracks.iter().fold(FormSection::new(""), |sec, t| {
            sec.item(TrackRow::Sub(t.i), RowKind::Choice, (), in_lang_row(t, active))
        });
        Form::new().section(sec)
    }

    /// The hits of the search on screen, or none.
    fn search_hits(&self) -> &[plx_data::subsearch::SubHit] {
        self.search.as_ref().map_or(&[], |s| s.hits.as_slice())
    }

    /// The Search page: the language row (and a retry row after a transport failure), then the
    /// results. With nothing to list the results section carries ONE line saying why — searching,
    /// none found, or the failure — so the empty answer and the fault never read alike, and no
    /// failure is ever drawn as an alarm (`ui/src/CLAUDE.md`: failure read-outs are never red).
    fn search_form(&self) -> TrackForm {
        use plx_data::subsearch::{DownloadPhase, SearchFailure, SearchStatus};
        use plx_platform::i18n::msg;
        let Some(s) = self.search.as_ref() else { return Form::new() };
        let busy = s.download.busy();
        let lang = if s.lang.is_empty() { String::new() } else { plx_plex::plex::languages::label(&s.lang).to_string() };
        // a language change mid-download is refused by the store, so the row is inert meanwhile
        let mut top = FormSection::new("")
            .item(
                TrackRow::SearchLang,
                RowKind::Nav(TrackPage::SearchLanguage),
                (),
                Row::new(msg::widgets_tracks_search_language()).value(lang),
            )
            .disabled(busy);
        if s.failure == Some(SearchFailure::Transport) {
            top = top.item(TrackRow::SearchRetry, RowKind::Button, (), Row::new(msg::widgets_tracks_search_retry()));
        }
        let mut results = FormSection::new(msg::widgets_tracks_search_results());
        for (i, h) in s.hits.iter().enumerate() {
            let state = match &s.download {
                DownloadPhase::Sending { hit } | DownloadPhase::Waiting { hit, .. } if *hit == i => {
                    Some(msg::widgets_tracks_search_adding())
                }
                DownloadPhase::Installed { hit, .. } if *hit == i => Some(msg::widgets_tracks_search_added()),
                DownloadPhase::Unconfirmed { hit } if *hit == i => Some(msg::widgets_tracks_search_requested()),
                DownloadPhase::Failed { hit, .. } if *hit == i => Some(msg::widgets_tracks_search_add_failed()),
                // not this session's install, but the item already lists it: read it the same way
                _ if self.hit_on_item(h) => Some(msg::widgets_tracks_search_added()),
                _ => None,
            };
            let title = if h.title.is_empty() { h.language.clone() } else { h.title.clone() };
            let detail: Vec<&str> =
                [h.provider.as_str(), h.language.as_str()].into_iter().filter(|p| !p.is_empty()).collect();
            let mut row = Row::new(title).server_label();
            if !detail.is_empty() {
                row = row.detail(detail.join(" \u{b7} ")).server_detail();
            }
            if h.hearing_impaired {
                row = row.badge(Badge::Sdh);
            }
            if h.forced {
                row = row.badge(Badge::Forced);
            }
            if let Some(state) = state {
                row = row.value(state);
            }
            results = results.item(TrackRow::SearchHit(i), RowKind::Button, (), row).disabled(busy);
        }
        let status = match (s.status, s.failure) {
            (SearchStatus::Idle | SearchStatus::Searching, _) => Some(msg::widgets_tracks_search_searching()),
            (SearchStatus::Ready, _) if s.hits.is_empty() => Some(msg::widgets_tracks_search_none()),
            (SearchStatus::Failed, Some(SearchFailure::Denied)) => Some(msg::widgets_tracks_search_denied()),
            (SearchStatus::Failed, Some(SearchFailure::Missing)) => Some(msg::widgets_tracks_search_missing()),
            (SearchStatus::Failed, Some(SearchFailure::BadLanguage)) => {
                Some(msg::widgets_tracks_search_bad_language())
            }
            (SearchStatus::Failed, _) => Some(msg::widgets_tracks_search_failed()),
            _ => None,
        };
        if let Some(status) = status {
            results = results.note(status);
        }
        if matches!(s.download, DownloadPhase::Failed { .. }) {
            results = results.note(msg::widgets_tracks_search_add_failed_note());
        }
        Form::new().section(top).section(results)
    }

    /// The language page: every language the search can use, the one being searched checked.
    fn search_language_form(&self) -> TrackForm {
        let langs = search_languages();
        let lang = self.search.as_ref().map_or("", |s| s.lang.as_str());
        let current = langs.iter().position(|l| l.code == lang).map(TrackRow::LangChoice);
        let sec = langs.iter().enumerate().fold(FormSection::new(""), |sec, (i, l)| {
            sec.choice(TrackRow::LangChoice(i), (), Row::new(l.name), |id| Some(*id) == current)
        });
        Form::new().section(sec)
    }

    /// OK on a result: ask the store to install it, quoting the generation on screen so a press
    /// against a replaced result set (whose keys are dead) is refused there rather than sent.
    /// Inert while a download is busy, and on the row already installed or requested — or already
    /// on the item ([`Self::hit_on_item`]), which would list the subtitle twice.
    fn search_download(&self, i: usize) -> TrackOk {
        use plx_data::subsearch::DownloadPhase;
        let Some(s) = self.search.as_ref() else { return TrackOk::Inert };
        if s.download.busy() || i >= s.hits.len() || self.hit_on_item(&s.hits[i]) {
            return TrackOk::Inert;
        }
        if let DownloadPhase::Installed { hit, .. } | DownloadPhase::Unconfirmed { hit } = &s.download {
            if *hit == i {
                return TrackOk::Inert;
            }
        }
        TrackOk::Search(plx_data::subsearch::SubSearchCmd::Download { gen: s.gen, hit: i })
    }

    /// Does the playing item already carry this hit as a sidecar — the same release name and
    /// language, as [`metadata::Stream::is_sidecar_release`] reads them? Adding it again would list
    /// one subtitle twice, so such a row reads Added and is inert.
    fn hit_on_item(&self, h: &plx_data::subsearch::SubHit) -> bool {
        self.item_sidecars.iter().any(|s| s.is_sidecar_release(&h.title, &h.language_code))
    }

    /// OK on a language: back to the Search page, searching in it. The panel's copy of the search
    /// reads "searching" at once, so the old language's results never sit under the new one's name
    /// while the store is still asking.
    fn pick_search_language(&mut self, i: usize) -> TrackOk {
        let Some(lang) = search_languages().get(i).map(|l| l.code) else { return TrackOk::Inert };
        let Some(s) = self.search.as_mut() else { return TrackOk::Inert };
        if s.lang == lang {
            return TrackOk::Inert;
        }
        s.lang = lang.to_string();
        s.hits.clear();
        s.failure = None;
        s.status = plx_data::subsearch::SearchStatus::Searching;
        self.pop_to_page();
        TrackOk::Search(plx_data::subsearch::SubSearchCmd::SetLanguage(lang.to_string()))
    }

    /// Pop the top page onto the PAGE beneath it (never onto the root, which needs the session to
    /// rebuild). `false`, leaving the stack as it was, when the root is beneath.
    fn pop_to_page(&mut self) -> bool {
        let Some(saved) = self.pages.pop() else { return false };
        let Some(page) = self.pages.top() else {
            self.pages.push(saved.page, saved.return_id, saved.scroll);
            return false;
        };
        let leaving = page_stack::leave_page(&mut self.form);
        self.motion.begin_slide(leaving, -1.0);
        let form = self.page_form(page);
        self.form.restore(form, Some(&saved.return_id), saved.scroll);
        self.form.table.set_title(Some(self.page_title(page)));
        true
    }

    /// **Publish the subtitle search to the panel**, from the screen, once a frame. Whether the
    /// search is OFFERED is fixed when the panel is built ([`Self::new_with`]); a panel built
    /// without the offer ignores this, so the root is never rebuilt under a live — or closing —
    /// panel. A change refreshes the Search or language page in place, focus kept by id, so a
    /// landing never moves the row the viewer is on.
    pub fn set_search(&mut self, search: plx_data::subsearch::SubSearchSnapshot) {
        if self.search.is_none() || self.search.as_ref() == Some(&search) {
            return;
        }
        if search.download.busy() {
            self.search_watching = true;
        }
        self.search = Some(search);
        if let Some(page @ (TrackPage::Search | TrackPage::SearchLanguage)) = self.pages.top() {
            let form = self.page_form(page);
            let fallback = self.page_initial(page);
            self.form.refresh_with(form, None, fallback.as_ref());
        }
    }

    /// Is the viewer on the Search pages? The screen opens the search only then, so merely opening
    /// the menu never puts a query to the server's subtitle agent.
    pub fn wants_search(&self) -> bool {
        self.pages.iter().any(|saved| matches!(saved.page, TrackPage::Search | TrackPage::SearchLanguage))
    }

    /// **The auto-select**, once, by the panel that watched the download in flight: the subtitle as the playing item should list
    /// it, and the commit that makes it the active one — the SAME `TrackCommit::Subtitle` a pick on
    /// its row would produce, so it runs the existing chain (client-drawn sidecar on direct play, a
    /// server burn while transcoding) rather than a parallel one. The server has already selected
    /// it on its side (`docs/pms-api.md` §8); this is what makes OUR renderer draw it.
    pub fn take_search_commit(&mut self) -> Option<(metadata::Stream, TrackCommit)> {
        let s = self.search.as_ref()?;
        let plx_data::subsearch::DownloadPhase::Installed { hit, stream } = &s.download else { return None };
        if !std::mem::take(&mut self.search_watching) {
            return None;
        }
        let picked = s.hits.get(*hit);
        let sub = metadata::Stream {
            id: stream.id,
            key: stream.key.clone(),
            codec: stream.codec.clone(),
            lang: picked.map(|h| h.language.clone()).unwrap_or_default(),
            lang_code: stream.language_code.clone(),
            title: picked.map(|h| h.title.clone()).unwrap_or_default(),
            // the hit's own badges, so the row reads the same here as on the Search page
            sdh: picked.is_some_and(|h| h.hearing_impaired),
            forced: picked.is_some_and(|h| h.forced),
            external: true,
            ..Default::default()
        };
        let sidecar = sub.sidecar_renderable();
        let commit = TrackCommit::Subtitle {
            render_ordinal: -1,
            stream_id: stream.id,
            sidecar_key: sidecar.then(|| stream.key.clone()),
            sidecar_codec: if sidecar { stream.codec.clone() } else { String::new() },
            // A subtitle the viewer fetched is the viewer's pick too: remembered by what it is.
            memo: Some(plx_media::route::memo_of_subtitle(&sub)),
        };
        Some((sub, commit))
    }

    /// The search snapshot the panel holds — for the screen's auto-select step.
    pub fn search(&self) -> Option<&plx_data::subsearch::SubSearchSnapshot> {
        self.search.as_ref()
    }

    /// The Style page: one drill-in per field, each reading out its current value. Under an image
    /// or styled subtitle Size and Position are disabled (dim, focusable, inert) and one note names
    /// why; Color is live under every renderer — the subtitle ink tints bitmaps and ASS alike.
    fn custom_position_distance(&self) -> u8 {
        self.position.custom_gap()
            .unwrap_or_else(plx_media::route::subtitle_custom_distance)
    }

    fn position_readout(&self) -> String {
        subtitle_position_label(self.position).into()
    }

    fn style_form(&self) -> TrackForm {
        let field_row = |field: StyleField| {
            let value = if field == StyleField::Position {
                self.position_readout()
            } else {
                field.rung_label(self.current_rung(field)).into()
            };
            Row::new(field.label()).value(value)
        };
        let reaches = self.renderer == SubRenderer::Text;
        let mut sec = FormSection::new("")
            .item(
                TrackRow::OpenField(StyleField::Size),
                RowKind::Nav(TrackPage::Picker(StyleField::Size)),
                (),
                field_row(StyleField::Size),
            )
            .disabled(!reaches)
            .item(
                TrackRow::OpenField(StyleField::Position),
                RowKind::Nav(TrackPage::Picker(StyleField::Position)),
                (),
                field_row(StyleField::Position),
            )
            .disabled(!reaches)
            .item(
                TrackRow::OpenField(StyleField::Color),
                RowKind::Nav(TrackPage::Picker(StyleField::Color)),
                (),
                field_row(StyleField::Color),
            );
        if let Some(note) = self.renderer.note() {
            sec = sec.note(note);
        }
        Form::new().section(sec)
    }

    /// A picker page: one choice per rung of `field`'s ladder, the current one checked.
    fn picker_form(&self, field: StyleField) -> TrackForm {
        let current = TrackRow::Choice(field, self.current_rung(field));
        let sec = (0..field.rungs()).fold(FormSection::new(""), |sec, rung| {
            let position = SubtitlePosition::from_index(rung as u8);
            let row = if field == StyleField::Position
                && matches!(position, SubtitlePosition::Custom(_))
            {
                Row::new(field.rung_label(rung)).value(plx_platform::i18n::msg::settings_playback_subtitle_position_distance_px(self.custom_position_distance() as i64))
            } else {
                Row::new(field.rung_label(rung))
            };
            sec.choice(TrackRow::Choice(field, rung), (), row, |id| *id == current)
        });
        Form::new().section(sec)
    }

    /// The explicit initial focus of a pushed page: the Style page opens on Size, a picker on its
    /// checked rung, Other languages on the checked row (the active track's, or its language's
    /// drill-in) else the first, a language page on the active variant if it is inside, else its
    /// first track. `None` (a language already gone) leaves it to the table's opening row.
    fn page_initial(&self, page: TrackPage) -> Option<TrackRow> {
        let active = usize::try_from(self.checked_sub()).ok();
        let holds_active = |o: &OtherLang| active.is_some_and(|a| o.tracks.iter().any(|t| t.i == a));
        match page {
            TrackPage::Style => Some(TrackRow::OpenField(StyleField::Size)),
            TrackPage::Picker(field) => Some(TrackRow::Choice(field, self.current_rung(field))),
            TrackPage::OtherLanguages => {
                self.other.iter().find(|o| holds_active(o)).or(self.other.first()).map(Self::other_lang_row)
            }
            TrackPage::Language(stream) => {
                let tracks = &self.other_lang(stream)?.tracks;
                let t = tracks.iter().find(|t| active == Some(t.i)).or(tracks.first())?;
                Some(TrackRow::Sub(t.i))
            }
            TrackPage::Search => Some(if self.search_hits().is_empty() {
                TrackRow::SearchLang
            } else {
                TrackRow::SearchHit(0)
            }),
            TrackPage::SearchLanguage => {
                let lang = self.search.as_ref().map_or("", |s| s.lang.as_str());
                Some(TrackRow::LangChoice(search_languages().iter().position(|l| l.code == lang).unwrap_or(0)))
            }
        }
    }

    /// The title band's text of `page`.
    fn page_title(&self, page: TrackPage) -> String {
        page.title(&self.other)
    }

    /// Open `page` above the current one: remember the opener and its scroll, install the page's
    /// rows (the pill snaps, the scroll returns to the top, focus lands on
    /// [`Self::page_initial`]) and its title band.
    fn push(&mut self, page: TrackPage) {
        let Some(return_id) = self.form.selected_id().copied() else { return };
        self.pages.push(page, return_id, self.form.table.scroll_pos());
        let leaving = page_stack::leave_page(&mut self.form);
        let form = self.page_form(page);
        self.form.open(form, self.page_initial(page).as_ref());
        // after the sections: the band moves every row, so the pill is re-jumped onto the focused one
        self…48203 tokens truncated…
                Step::Edge
            ));
            let mut groups = Vec::new();
            <TrackMenuPart as Focusable<HostFixture>>::groups(&part, cx, &mut groups);
            let g = groups.into_iter().next().expect("one group");
            assert!(matches!(g.edge[2], plx_ui::screen::EdgeRule::Screen));
            assert!(matches!(g.edge[3], plx_ui::screen::EdgeRule::Screen));
        });
    }

    /// `place` reports exactly the row rect `TableView::row_frame` — and so the old `draw` —
    /// paints at.
    #[test]
    fn place_matches_the_tables_own_row_frame() {
        let e = EntryId(5);
        let st = three_row_menu();
        let r = st.panel_rect(&plx_ui::fixture::FixtureMeasure);
        let want = st.form.table.row_frame(r, 2);
        let part = TrackMenuPart { state: &st, entry: e, group: GroupId(0) };
        with_cx(e, |cx| {
            let placed = <TrackMenuPart as Focusable<HostFixture>>::place(&part, &audio_key(2), cx, At::Drawn);
            assert_eq!(
                placed.map(|p| (p.rect.x, p.rect.y, p.rect.w, p.rect.h)),
                want.map(|r| (r.x, r.y, r.w, r.h))
            );
        });
    }

    /// **Reordering the menu moves no focus key.** The rows are the same three identities in the
    /// opposite order: the focused row stays focused (by identity, through `FormTable::set`), its
    /// key is the one it always had, `seat` hands the engine that key, `place` finds it at its new
    /// position, and a DOWN step from it goes to whichever row now sits below.
    #[test]
    fn reordering_the_rows_moves_no_focus_key() {
        let e = EntryId(5);
        let mut st = three_row_menu();
        st.form.table.sel = 2; // Audio(2), the last row
        let mut sec = FormSection::new("Audio");
        for i in [2usize, 1, 0] {
            sec = sec.item(TrackRow::Audio(i), RowKind::Choice, (), Row::new(format!("row {i}")));
        }
        st.form.set(TrackForm::new().section(sec), st.form.selected_id().copied().as_ref());
        assert_eq!(st.form.selected_id(), Some(&TrackRow::Audio(2)), "focus followed the identity");
        assert_eq!(st.form.table.sel, 0, "…to its new position");

        let part = TrackMenuPart { state: &st, entry: e, group: GroupId(0) };
        with_cx(e, |cx| {
            let seat = <TrackMenuPart as Focusable<HostFixture>>::seat(
                &part,
                GroupId(0),
                Placed { rect: Rect::new(0.0, 0.0, 1.0, 1.0), rest_rect: Rect::new(0.0, 0.0, 1.0, 1.0), clip: Rect::new(0.0, 0.0, 1.0, 1.0), index: None },
                cx,
            );
            assert_eq!(seat.elem, audio_key(2), "the engine is told the key the row always had");
            let placed = <TrackMenuPart as Focusable<HostFixture>>::place(&part, &audio_key(2), cx, At::Drawn)
                .expect("the moved row still places");
            assert_eq!(placed.index, Some(0));
            let Step::Move(next) = <TrackMenuPart as Focusable<HostFixture>>::neighbour(
                &part,
                FocusKey { entry: e, elem: audio_key(2) },
                Dir::Down,
                cx,
            ) else {
                panic!("DOWN from the first row moves")
            };
            assert_eq!(next.elem, audio_key(1), "stepping follows the NEW order");
        });
    }

    /// **The follow-up gap.** A row count that shrinks and grows back (the Audio tab's enhancement
    /// rows vanishing under a live poll rebuild, then returning) can leave the ENGINE's own
    /// remembered focus index stale relative to `table.sel`: `TrackMenuState::rebuild_audio`
    /// restores `table.sel` onto the toggle row's new position (`sticky_audio_target`), but the
    /// engine has no way to learn that unless `reconcile` actually reports it. Before the fix,
    /// `reconcile` answered `settle(want)` — clamping the ENGINE's own possibly-stale index — which
    /// only differs from `want` when that raw index is now literally out of range, so a mere
    /// position change the panel already resolved (not a shrink past it) went unreported and the
    /// engine's remembered element stayed wrong. `reconcile` must instead always answer the panel's
    /// own `table.sel`, so the engine adopts it whenever it disagrees.
    #[test]
    fn reconcile_reports_the_panels_own_cursor_not_a_clamp_of_the_engines_stale_index() {
        let e = EntryId(5);
        let mut st = three_row_menu();
        // The panel's own rebuild has already moved `table.sel` to row 2 (e.g. `sticky_audio_target`
        // restoring focus onto the toggle row once the enhancement rows came back).
        st.form.table.sel = 2;
        let part = TrackMenuPart { state: &st, entry: e, group: GroupId(0) };
        with_cx(e, |cx| {
            // The engine still remembers row 0 — a perfectly in-range index for this 3-row table,
            // so the old clamp-`want` implementation would answer it back UNCHANGED.
            let want = FocusKey { entry: e, elem: audio_key(0) };
            let got = <TrackMenuPart as Focusable<HostFixture>>::reconcile(&part, want, cx);
            assert_eq!(
                got.elem, audio_key(2),
                "reconcile must follow the panel's own table.sel, not echo back an in-range `want`"
            );
        });
    }
}

#[cfg(test)]
mod localized_offset_tests {
    #[test]
    fn subtitle_timing_uses_locale_decimal_and_unit_without_changing_offset_sign() {
        use plx_platform::i18n::{LocaleContext, Preference};
        for (preference, region, negative, positive) in [
            (Preference::En, "en-US", "-0.1 s", "+1.3 s"),
            (Preference::Es, "es-ES", "-0,1 s", "+1,3 s"),
            (Preference::Be, "be-BY", "-0,1 с", "+1,3 с"),
            (Preference::Fr, "fr-FR", "-0,1 s", "+1,3 s"),
            (Preference::De, "de-DE", "-0,1 s", "+1,3 s"),
            (Preference::Hu, "hu-HU", "-0,1 mp", "+1,3 mp"),
            (Preference::Ru, "ru-RU", "-0,1 с", "+1,3 с"),
        ] {
            let locale = LocaleContext::resolve(preference, None, Some(region), None, None);
            assert_eq!(crate::timing_capsule::offset_seconds_in(-100, true, &locale), negative);
            assert_eq!(crate::timing_capsule::offset_seconds_in(1300, true, &locale), positive);
        }
    }
}

#[cfg(test)]
mod keyed_form_tests {
    use super::tests::{store_with, store_with_audio, stream};
    use super::*;
    use plx_ui::form::RowKeys;
    use plx_media::route::{enhancement_test_session, reset_player_control_for_test, EnhTestFixture};

    fn audio(id: i64, index: i64, default: bool) -> metadata::Stream {
        metadata::Stream { id, index, codec: "aac".into(), channels: 2, default, ..Default::default() }
    }

    fn key_of(menu: &TrackMenuState, id: TrackRow) -> Option<RowKey> {
        menu.form.index_of(&id).and_then(|i| menu.form.key_at(i))
    }

    #[test]
    fn keys_are_distinct_across_both_tabs_and_below_the_ceiling() {
        let mut ids = vec![TrackRow::SubOff, TrackRow::Timing, TrackRow::Style, TrackRow::Boost, TrackRow::Loudness];
        for field in StyleField::ALL {
            ids.push(TrackRow::OpenField(field));
            ids.extend((0..field.rungs()).map(|i| TrackRow::Choice(field, i)));
        }
        ids.extend((0..40).flat_map(|i| [TrackRow::Sub(i), TrackRow::Audio(i)]));
        let keys: Vec<u32> = ids.iter().map(|i| i.key().0).collect();
        for (n, a) in keys.iter().enumerate() {
            assert!(*a < BAND_BASE);
            assert!(!keys[n + 1..].contains(a), "duplicate key {a:#x}");
        }
    }

    /// **A live refresh that inserts the enhancement rows leaves focus on the same audio track**,
    /// found by id, with no pending reseat (the engine's key still names the landed row).
    #[test]
    fn a_live_refresh_inserting_the_enhancement_rows_keeps_focus_on_the_same_audio_track() {
        let _g = plx_base::testlock::serial();
        let store = store_with_audio(vec![audio(501, 0, true), audio(502, 1, false)]);
        let (ps_hidden, _s1) = enhancement_test_session(EnhTestFixture {
            pass: plx_plex::plex::serverinfo::Subscription::No,
            ..Default::default()
        });
        let mut menu = TrackMenuState::new(&ps_hidden, store.view(), 0, Vec::new());
        assert_eq!(menu.ids(), vec![TrackRow::Audio(0), TrackRow::Audio(1)]);
        menu.focus_key(TrackRow::Audio(1).key().0);
        let before = key_of(&menu, TrackRow::Audio(1));

        let (ps_offered, _s2) = enhancement_test_session(EnhTestFixture::default());
        menu.update(0.0, &plx_ui::fixture::FixtureMeasure, &ps_offered, store.view());

        assert!(menu.enhance_shown.is_some(), "premise: the pair was inserted");
        assert_eq!(menu.ids().len(), 4, "two tracks + Boost + Loudness");
        assert_eq!(menu.selected_id(), Some(TrackRow::Audio(1)));
        assert_eq!(key_of(&menu, TrackRow::Audio(1)), before, "the track's key did not move");
        assert_eq!(RowKeys::reseat(&menu.form), None, "the engine's key still names the landed row");
        reset_player_control_for_test(&ps_hidden);
        plx_plex::plex::reset_servers_for_test();
    }

    /// **Adding a subtitle track to the offered list moves no other row's key**, even though the
    /// added track sorts above them on screen (its key is its subs-list index, not its position).
    #[test]
    fn focus_keys_are_stable_when_a_subtitle_track_is_added() {
        let _g = plx_base::testlock::serial();
        plx_media::player::sidecar::reset();
        let ps = plx_media::route::PlaybackSession::IDLE;
        let two = vec![stream(1, 0, "English", "eng", ""), stream(2, 1, "French", "fra", "")];
        let mut three = two.clone();
        three.push(stream(3, 2, "Arabic", "ara", "")); // "yours" ranks Arabic first
        let yours = || vec!["ara".to_string(), "eng".to_string(), "fra".to_string()];
        let (a, b) = (store_with(two), store_with(three));
        let before = TrackMenuState::new(&ps, a.view(), 1, yours());
        let after = TrackMenuState::new(&ps, b.view(), 1, yours());
        assert!(after.form.index_of(&TrackRow::Sub(2)) < after.form.index_of(&TrackRow::Sub(0)),
            "premise: the new track is drawn ABOVE the existing ones");
        for id in [TrackRow::SubOff, TrackRow::Timing, TrackRow::Style, TrackRow::Sub(0), TrackRow::Sub(1)] {
            assert!(key_of(&before, id).is_some(), "{id:?} is built");
            assert_eq!(key_of(&before, id), key_of(&after, id), "{id:?} kept its key");
        }
    }

    /// **A language's drill-in keeps its identity when tracks are added** — a track of another
    /// language that sorts above it, a second track of its own language, a later track of a new
    /// language: `OpenLang` is the stream id of the language's first track, never a list position.
    /// Its KEY is its ordinal on the page (what the focus engine needs: unique, below the ceiling),
    /// so it follows the language when another sorts above it, and the form's `reseat` carries the
    /// engine along.
    #[test]
    fn an_open_lang_is_the_same_row_when_tracks_are_added() {
        let _g = plx_base::testlock::serial();
        plx_media::player::sidecar::reset();
        let ps = plx_media::route::PlaybackSession::IDLE;
        let base = vec![
            stream(1, 0, "German", "deu", ""),
            stream(2, 1, "French", "fra", ""),
            stream(3, 2, "French", "fra", "SDH"),
        ];
        let mut more = base.clone();
        more.push(stream(4, 3, "Arabic", "ara", "")); // sorts above French on the page
        more.push(stream(5, 4, "French", "fra", "Commentary"));
        let other_page = |subs: Vec<metadata::Stream>| {
            let store = store_with(subs);
            let mut menu = TrackMenuState::new(&ps, store.view(), 1, Vec::new());
            menu.push(TrackPage::OtherLanguages);
            menu
        };
        let (before, after) = (other_page(base), other_page(more));
        let french = TrackRow::OpenLang(LangId { stream: 2, slot: 0 });
        assert!(key_of(&before, french).is_some(), "French is a drill-in on the page");
        assert!(after.form.index_of(&french).is_some(), "the same identity is on the page after");
        assert!(after.form.index_of(&TrackRow::Sub(3)) < after.form.index_of(&french), "premise: Arabic is drawn above it");
        let keys = after.keys();
        assert_eq!(keys.len(), keys.iter().collect::<std::collections::HashSet<_>>().len(), "keys are unique on the page");
        assert_ne!(key_of(&after, french), key_of(&after, TrackRow::OpenOther));
    }

    /// **Initial focus is the active track on both tabs** — and Off when no subtitle is active.
    #[test]
    fn initial_focus_lands_on_the_active_track_on_both_tabs() {
        let _g = plx_base::testlock::serial();
        plx_media::player::sidecar::reset();
        let ps = plx_media::route::PlaybackSession::IDLE;
        // Audio: the flagged default (IDLE records no sid) is the SECOND track
        let store = store_with_audio(vec![audio(501, 0, false), audio(502, 1, true), audio(503, 2, false)]);
        let menu = TrackMenuState::new(&ps, store.view(), 0, Vec::new());
        assert_eq!(menu.selected_id(), Some(TrackRow::Audio(1)));
        assert_eq!(menu.sel(), 1);

        // Subtitles: Off when none is active, else the active track (even inside "Other languages")
        let subs = vec![stream(1, 0, "English", "eng", ""), stream(2, 1, "French", "fra", "")];
        let store = store_with(subs);
        let mut menu = TrackMenuState::new(&ps, store.view(), 1, vec!["eng".into(), "fra".into()]);
        assert_eq!(menu.selected_id(), Some(TrackRow::SubOff));
        menu.active_sub = 1;
        menu.rebuild(&ps, store.view(), 1, false);
        assert_eq!(menu.selected_id(), Some(TrackRow::Sub(1)));
        menu.focus_tab(&ps, store.view(), 0); // a tab switch re-opens on that tab's own active row
        menu.focus_tab(&ps, store.view(), 1);
        assert_eq!(menu.selected_id(), Some(TrackRow::Sub(1)));
    }
}

#[cfg(test)]
mod style_page_tests {
    use super::tests::{store_with, stream};
    use super::*;
    use plx_media::route::{enhancement_test_session, reset_player_control_for_test, EnhTestFixture, SubtitleEffect};

    fn teardown(ps: &plx_media::route::PlaybackSession) {
        reset_player_control_for_test(ps);
        plx_plex::plex::reset_servers_for_test();
    }

    /// A direct-play route with the subtitle `codec` active (or none when `effect` is `None`).
    fn open_with(
        codec: &str,
        effect: SubtitleEffect,
    ) -> (TrackMenuState, plx_media::route::PlaybackSession, plx_data::stores::metadata::MetadataStore) {
        let (ps, _sid) = enhancement_test_session(EnhTestFixture {
            remux: None,
            subtitle_effect: effect,
            ..Default::default()
        });
        let mut s = stream(999, 0, "English", "eng", "");
        s.codec = codec.into();
        let store = store_with(vec![s]);
        let menu = TrackMenuState::new(&ps, store.view(), 1, vec!["eng".into()]);
        (menu, ps, store)
    }

    fn open_text() -> (TrackMenuState, plx_media::route::PlaybackSession, plx_data::stores::metadata::MetadataStore) {
        open_with("srt", SubtitleEffect::Sidecar)
    }

    fn focus_id(menu: &mut TrackMenuState, id: TrackRow) {
        let i = menu.form.index_of(&id).unwrap_or_else(|| panic!("{id:?} is not on this page"));
        menu.focus_row(i as c_int);
        assert_eq!(menu.selected_id(), Some(id));
    }

    fn row_of(menu: &TrackMenuState, id: TrackRow) -> &Row {
        let i = menu.form.index_of(&id).unwrap_or_else(|| panic!("{id:?} is not on this page"));
        menu.form.table.sections.iter().flat_map(|s| &s.rows).nth(i).unwrap()
    }

    /// OK on Style pushes the Style page, focus on Size by id; OK on Size pushes its picker with
    /// the checked rung focused; LEFT pops each back onto the row that opened it.
    #[test]
    fn push_lands_on_an_explicit_id_and_pop_restores_the_opener() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open_text();
        focus_id(&mut menu, TrackRow::Style);
        assert_eq!(menu.on_ok(store.view()), TrackOk::Navigated);
        assert_eq!(menu.page_path(), [TrackPage::Style]);
        assert_eq!(menu.selected_id(), Some(TrackRow::OpenField(StyleField::Size)));
        assert_eq!(menu.form.table.title(), Some(TrackPage::Style.title(&[]).as_str()));

        focus_id(&mut menu, TrackRow::OpenField(StyleField::Position));
        assert_eq!(menu.on_ok(store.view()), TrackOk::Navigated);
        assert_eq!(menu.page_path(), [TrackPage::Style, TrackPage::Picker(StyleField::Position)]);
        assert_eq!(
            menu.selected_id(),
            Some(TrackRow::Choice(StyleField::Position, menu.position.index() as usize)),
            "a picker opens on its checked rung"
        );

        menu.on_left(&ps, store.view());
        assert_eq!(menu.page_path(), [TrackPage::Style]);
        assert_eq!(menu.selected_id(), Some(TrackRow::OpenField(StyleField::Position)), "the opener, by id");
        menu.on_left(&ps, store.view());
        assert!(menu.page_path().is_empty());
        assert_eq!(menu.selected_id(), Some(TrackRow::Style));
        assert_eq!(menu.form.table.title(), None, "the root has no title band");
        teardown(&ps);
    }

    /// RIGHT on a Nav row enters the page exactly as OK does; RIGHT off one is the tab switch.
    #[test]
    fn right_on_a_nav_row_pushes_and_left_at_the_root_switches_tab() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open_text();
        focus_id(&mut menu, TrackRow::Style);
        menu.on_right(&ps, store.view());
        assert_eq!(menu.page_path(), [TrackPage::Style]);
        assert!(!menu.pop(&ps, store.view()) || menu.page_path().is_empty());
        assert!(!menu.pop(&ps, store.view()), "BACK at the root pops nothing: the caller dismisses");
        menu.on_left(&ps, store.view());
        assert_eq!(menu.tab, 0, "LEFT at the root is still the tab switch");
        teardown(&ps);
    }

    /// A pop reinstates the scroll the page was left at, not the top.
    #[test]
    fn pop_restores_the_scroll_the_root_was_left_at() {
        let _g = plx_base::testlock::serial();
        let (ps, _sid) = enhancement_test_session(EnhTestFixture {
            remux: None,
            subtitle_effect: SubtitleEffect::Sidecar,
            ..Default::default()
        });
        let subs: Vec<_> = (0..40).map(|i| stream(999 + i, i, "English", "eng", &format!("Track {i}"))).collect();
        let store = store_with(subs);
        let mut menu = TrackMenuState::new(&ps, store.view(), 1, vec!["eng".to_string()]);
        focus_id(&mut menu, TrackRow::Style);
        for _ in 0..240 {
            menu.update(0.016, &plx_ui::fixture::FixtureMeasure, &ps, store.view());
        }
        let left_at = menu.form.table.scroll_pos();
        assert!(left_at > 0.0, "the premise: 40 tracks push Style below the fold ({left_at})");
        assert_eq!(menu.on_ok(store.view()), TrackOk::Navigated);
        assert!(menu.form.table.scroll_pos() < left_at, "a page starts at its top");
        assert!(menu.pop(&ps, store.view()));
        assert_eq!(menu.form.table.scroll_pos(), left_at);
        assert_eq!(menu.selected_id(), Some(TrackRow::Style));
        teardown(&ps);
    }

    /// Picking a rung commits it live, keeps the panel and page open and moves the checkmark; the
    /// already-checked rung is inert.
    #[test]
    fn a_picker_pick_commits_live_and_moves_the_checkmark() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open_text();
        focus_id(&mut menu, TrackRow::Style);
        assert_eq!(menu.on_ok(store.view()), TrackOk::Navigated);
        assert_eq!(menu.on_ok(store.view()), TrackOk::Navigated, "Size");
        let current = menu.current_rung(StyleField::Size);
        assert!(row_of(&menu, TrackRow::Choice(StyleField::Size, current)).checked);
        assert_eq!(menu.on_ok(store.view()), TrackOk::Inert, "the focused rung is the checked one");

        let other = (current + 1) % StyleField::Size.rungs();
        focus_id(&mut menu, TrackRow::Choice(StyleField::Size, other));
        assert_eq!(
            menu.on_ok(store.view()),
            TrackOk::Commit {
                commit: TrackCommit::SubtitleSize(SubtitleSize::from_index(other as u8)),
                keep_open: true
            }
        );
        assert_eq!(menu.page_path().len(), 2, "the page stays");
        assert!(row_of(&menu, TrackRow::Choice(StyleField::Size, other)).checked);
        assert!(!row_of(&menu, TrackRow::Choice(StyleField::Size, current)).checked);
        teardown(&ps);
    }

    /// The Style page reads the current value of each field on its Nav row.
    #[test]
    fn the_style_page_shows_each_fields_current_value() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open_text();
        focus_id(&mut menu, TrackRow::Style);
        menu.on_ok(store.view());
        for field in StyleField::ALL {
            let row = row_of(&menu, TrackRow::OpenField(field));
            assert_eq!(row.label, field.label());
            assert_eq!(row.value.as_deref(), Some(field.rung_label(menu.current_rung(field))));
        }
        teardown(&ps);
    }

    /// Locks per renderer kind: plain text leaves all three live; an image subtitle dims Size and
    /// Position with the image note; ASS dims them with its own; Color stays live in every case,
    /// and a dimmed row is inert for OK.
    #[test]
    fn size_and_position_lock_by_renderer_kind_and_color_never_does() {
        let _g = plx_base::testlock::serial();
        for (codec, renderer, note) in [
            ("srt", SubRenderer::Text, None),
            ("pgs", SubRenderer::Image, Some(plx_platform::i18n::msg::widgets_tracks_style_image_note())),
            ("ass", SubRenderer::Styled, Some(plx_platform::i18n::msg::widgets_tracks_style_styled_note())),
        ] {
            let (mut menu, ps, store) = open_with(codec, SubtitleEffect::Sidecar);
            assert_eq!(menu.renderer, renderer, "{codec}");
            focus_id(&mut menu, TrackRow::Style);
            assert_eq!(menu.on_ok(store.view()), TrackOk::Navigated, "{codec}: Style itself is live");
            let locked = renderer != SubRenderer::Text;
            for field in [StyleField::Size, StyleField::Position] {
                assert_eq!(row_of(&menu, TrackRow::OpenField(field)).dim, locked, "{codec} {field:?}");
            }
            assert!(!row_of(&menu, TrackRow::OpenField(StyleField::Color)).dim, "{codec}: Color stays live");
            let notes: Vec<_> = menu
                .form
                .table
                .sections
                .iter()
                .flat_map(|s| &s.rows)
                .filter(|r| r.is_note())
                .map(|r| r.label.to_string())
                .collect();
            assert_eq!(notes, note.map(|n| n.to_string()).into_iter().collect::<Vec<_>>(), "{codec}");
            if locked {
                focus_id(&mut menu, TrackRow::OpenField(StyleField::Size));
                assert_eq!(menu.on_ok(store.view()), TrackOk::Inert, "{codec}: a dim Size row opens nothing");
                menu.on_right(&ps, store.view());
                assert_eq!(menu.page_path(), [TrackPage::Style], "{codec}: RIGHT is inert on it too");
            }
            focus_id(&mut menu, TrackRow::OpenField(StyleField::Color));
            assert_eq!(menu.on_ok(store.view()), TrackOk::Navigated, "{codec}: Color opens its picker");
            teardown(&ps);
        }
    }

    /// Style follows Timing's availability: a transcode with no subtitle selected omits both, a
    /// server burn (any `ServerBurn`) keeps both drawn and dim with the reason under Style.
    #[test]
    fn style_follows_timings_availability() {
        let _g = plx_base::testlock::serial();
        let has = |menu: &TrackMenuState, id| menu.form.index_of(&id).is_some();
        // a transcode with no subtitle selected: nothing to offset or style, so neither row
        let (ps, _) = enhancement_test_session(EnhTestFixture { remux: Some(false), ..Default::default() });
        let store = store_with(vec![stream(999, 0, "English", "eng", "")]);
        let menu = TrackMenuState::new(&ps, store.view(), 1, Vec::new());
        assert!(!has(&menu, TrackRow::Timing) && !has(&menu, TrackRow::Style));
        teardown(&ps);
        // direct play: both, live
        let (menu, ps, _store) = open_text();
        assert!(has(&menu, TrackRow::Timing) && has(&menu, TrackRow::Style));
        assert!(!row_of(&menu, TrackRow::Style).dim);
        teardown(&ps);
    }

    /// **A Style page never outlives what it was built for**: a change of the
    /// page's availability (here the renderer kind) while on a sub-page pops to the root (opener and
    /// scroll restored); an unchanged signature leaves the page alone; on the root any change
    /// refreshes in place.
    #[test]
    fn a_rebuild_signature_mismatch_on_a_sub_page_pops_to_the_root() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open_with("srt", SubtitleEffect::Sidecar);
        focus_id(&mut menu, TrackRow::Style);
        menu.on_ok(store.view());
        menu.update(0.016, &plx_ui::fixture::FixtureMeasure, &ps, store.view());
        assert_eq!(menu.page_path(), [TrackPage::Style], "an unchanged signature leaves the page");

        // the same subtitle, now an image one: the renderer kind moved under the page
        let mut image = stream(999, 0, "English", "eng", "");
        image.codec = "pgs".into();
        let store2 = store_with(vec![image]);
        menu.update(0.016, &plx_ui::fixture::FixtureMeasure, &ps, store2.view());
        assert!(menu.page_path().is_empty(), "popped to the root");
        assert_eq!(menu.renderer, SubRenderer::Image);
        assert_eq!(menu.selected_id(), Some(TrackRow::Style), "on the row that opened it");
        assert_eq!(menu.form.table.title(), None);
        teardown(&ps);
    }

    /// **Only an availability change pops a sub-page.** A subs fingerprint change (a track offered
    /// mid-play) or an enhancement-route change that leaves the renderer and the Style gate alone
    /// refreshes the open Size picker in place: same page, same focused row, new signature stored.
    #[test]
    fn a_signature_change_that_keeps_the_pages_availability_refreshes_in_place() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open_text();
        focus_id(&mut menu, TrackRow::Style);
        menu.on_ok(store.view());
        focus_id(&mut menu, TrackRow::OpenField(StyleField::Size));
        menu.on_ok(store.view());
        let picker = [TrackPage::Style, TrackPage::Picker(StyleField::Size)];
        assert_eq!(menu.page_path(), picker);
        let focused = TrackRow::Choice(StyleField::Size, 0);
        focus_id(&mut menu, focused);
        let before = menu.sub_sig.clone();

        // a second subtitle offered mid-play: the fingerprint moves, the renderer does not
        let store2 = store_with(vec![stream(999, 0, "English", "eng", ""), stream(1000, 1, "French", "fra", "")]);
        menu.update(0.016, &plx_ui::fixture::FixtureMeasure, &ps, store2.view());
        assert_ne!(menu.sub_sig, before, "the new fingerprint is stored");
        assert_eq!(menu.page_path(), picker, "the picker stays");
        assert_eq!(menu.selected_id(), Some(focused), "on the row the viewer was on");
        teardown(&ps);

        // the enhancement's subtitle effect moves (an embedded track is now the one on screen), still no own burn
        let (mut menu, ps, store) = open_text();
        focus_id(&mut menu, TrackRow::Style);
        menu.on_ok(store.view());
        focus_id(&mut menu, TrackRow::OpenField(StyleField::Size));
        menu.on_ok(store.view());
        focus_id(&mut menu, focused);
        let before = menu.sub_sig.clone();
        teardown(&ps);
        let (ps2, _sid) = enhancement_test_session(EnhTestFixture {
            remux: None,
            subtitle_effect: SubtitleEffect::Embedded,
            ..Default::default()
        });
        menu.update(0.016, &plx_ui::fixture::FixtureMeasure, &ps2, store.view());
        assert_ne!(menu.sub_sig, before, "the enhancement route is part of the signature");
        assert_eq!(menu.page_path(), picker, "the picker stays");
        assert_eq!(menu.selected_id(), Some(focused));
        teardown(&ps2);
    }

    /// **Every Style surface fits the panel in every shipped language**: the Subtitles root with
    /// its Style row, the Style page under each renderer kind (Size and Position dim with their
    /// notes), and each picker page, judged by the same gates as the rest of the menu.
    #[test]
    fn every_style_page_fits_the_panel_in_every_language() {
        use plx_platform::i18n::{language_on_this_thread_for_test};
        let mut out = Vec::new();
        for language in plx_platform::i18n::SHIPPED {
            let _g = plx_base::testlock::serial();
            let _guard = language_on_this_thread_for_test(language);
            for codec in ["srt", "pgs", "ass"] {
                let (mut menu, ps, store) = open_with(codec, SubtitleEffect::Sidecar);
                let mut judge = |menu: &TrackMenuState| {
                    out.extend(menu.form.table.menu_cap_failure(&plx_base::fontcov::advances::ShippedMeasure, language.tag()));
                    out.extend(menu.form.table.app_fit_failures(plx_ui::table::MENU_MAX_W, language.tag()));
                    out.extend(menu.form.table.app_fit_failures_hugged(language.tag()));
                };
                judge(&menu);
                menu.push(TrackPage::Style);
                judge(&menu);
                for field in StyleField::ALL {
                    menu.push(TrackPage::Picker(field));
                    judge(&menu);
                    assert!(menu.pop(&ps, store.view()));
                }
                teardown(&ps);
            }
        }
        plx_ui::table::assert_no_fit_failures(&out);
    }

    /// **The locked-renderer notes wrap to two lines at the player-menu floor (three in Spanish).** A Style page is a few
    /// short rows; without [`theme::layout::PLAYER_MENU_MIN_W`] it shrank to its labels and the
    /// note wrapped to three lines in a sliver.
    #[test]
    fn the_style_note_fits_two_lines_at_the_player_menu_floor() {
        use plx_platform::i18n::{language_on_this_thread_for_test, Preference};
        let measure = plx_base::fontcov::advances::ShippedMeasure;
        for language in plx_platform::i18n::SHIPPED {
            let _g = plx_base::testlock::serial();
            let _guard = language_on_this_thread_for_test(language);
            for codec in ["pgs", "ass"] {
                let (mut menu, ps, _store) = open_with(codec, SubtitleEffect::Sidecar);
                menu.push(TrackPage::Style);
                let w = menu.form.table.menu_panel_width(&measure);
                assert!(w >= theme::layout::PLAYER_MENU_MIN_W, "{language:?}: the page is not a sliver ({w})");
                menu.form.table.fit_notes(w, &measure);
                let lines = menu.form.table.sections.iter().flat_map(|s| &s.rows)
                    .filter(|r| r.is_note()).map(|r| r.note_lines.get()).max().expect("a note row");
                // Spanish's longer wording needs ~520px for two lines; at the 440 floor it takes three
                // (measured), English and Belarusian take two
                let allowed = if language == Preference::Es { 3 } else { 2 };
                assert!(lines <= allowed, "{language:?} {codec}: the note takes {lines} lines at {w}px");
                teardown(&ps);
            }
        }
    }

    /// The title band is a pointer-only stop: its key is recognised, no row owns it.
    #[test]
    fn the_title_key_is_the_bands_and_no_rows() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open_text();
        focus_id(&mut menu, TrackRow::Style);
        menu.on_ok(store.view());
        assert!(page_stack::is_title_key(page_stack::TITLE_KEY));
        assert!(menu.keys().iter().all(|k| !page_stack::is_title_key(*k)));
        teardown(&ps);
    }

    /// The replay canon carries the tab, the page path, each opener and the selected key: two
    /// states that differ in any of them hash apart.
    #[test]
    fn the_canon_tells_pages_openers_and_selections_apart() {
        let _g = plx_base::testlock::serial();
        let hash = |menu: &TrackMenuState| {
            let mut c = Canon::new();
            menu.canon(&mut c);
            c.finish()
        };
        let (mut menu, ps, store) = open_text();
        focus_id(&mut menu, TrackRow::Style);
        let root = hash(&menu);
        menu.on_ok(store.view());
        let style = hash(&menu);
        focus_id(&mut menu, TrackRow::OpenField(StyleField::Color));
        let style_color = hash(&menu);
        menu.on_ok(store.view());
        let picker = hash(&menu);
        let all = [root, style, style_color, picker];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a, b);
            }
        }
        menu.on_left(&ps, store.view());
        assert_eq!(hash(&menu), style_color, "a pop returns to the recorded state");
        teardown(&ps);
    }
}

/// **The Other languages page and the language pages** (`docs/player-submenus.md`, PR 3).
#[cfg(test)]
mod language_page_tests {
    use super::tests::{store_with, stream};
    use super::*;
    use plx_media::route::reset_player_control_for_test;

    fn teardown(ps: &plx_media::route::PlaybackSession) {
        reset_player_control_for_test(ps);
        plx_plex::plex::reset_servers_for_test();
    }

    fn pgs(mut s: metadata::Stream) -> metadata::Stream {
        s.codec = "pgs".into();
        s
    }

    /// English is "yours" (index 0, a root row); Dutch (PGS) and German are single-track others;
    /// French has a full, an SDH and a forced PGS track.
    fn subs() -> Vec<metadata::Stream> {
        vec![
            stream(10, 0, "English", "eng", ""),
            stream(11, 1, "French", "fra", ""),
            stream(12, 2, "French", "fra", "SDH"),
            pgs(stream(13, 3, "French", "fra", "Forced")),
            stream(14, 4, "German", "deu", ""),
            pgs(stream(15, 5, "Dutch", "nld", "")),
        ]
    }

    /// [`subs`] plus Italian: a text full track and a VobSub SDH one (`dvd_subtitle` is Plex's
    /// own name for it), the long raw codec the badge must shorten.
    fn subs_with_vobsub() -> Vec<metadata::Stream> {
        let mut v = subs();
        v.push(stream(16, 6, "Italian", "ita", "Netflix"));
        let mut sdh = stream(17, 7, "Italian", "ita", "Netflix");
        sdh.codec = "dvd_subtitle".into();
        sdh.sdh = true;
        v.push(sdh);
        v
    }

    fn open(subs: Vec<metadata::Stream>) -> (TrackMenuState, plx_media::route::PlaybackSession, plx_data::stores::metadata::MetadataStore) {
        let _ = plx_media::player::sidecar::reset();
        let ps = plx_media::route::PlaybackSession::IDLE;
        let store = store_with(subs);
        let menu = TrackMenuState::new(&ps, store.view(), 1, vec!["eng".into()]);
        (menu, ps, store)
    }

    fn focus_id(menu: &mut TrackMenuState, id: TrackRow) {
        let i = menu.form.index_of(&id).unwrap_or_else(|| panic!("{id:?} is not on this page"));
        menu.focus_row(i as c_int);
        assert_eq!(menu.selected_id(), Some(id));
    }

    fn row_of(menu: &TrackMenuState, id: TrackRow) -> &Row {
        let i = menu.form.index_of(&id).unwrap_or_else(|| panic!("{id:?} is not on this page"));
        menu.form.table.sections.iter().flat_map(|s| &s.rows).nth(i).unwrap()
    }

    /// OK on the root's Other languages row opens the page A-Z (a single-track language is a pick
    /// row, French a drill-in reading its track count); OK on French opens its page, ranked; LEFT
    /// pops each back onto the row that opened it, by id, and the title follows the page.
    #[test]
    fn push_and_pop_walk_root_other_languages_and_a_language() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open(subs());
        assert_eq!(menu.ids()[..4], [TrackRow::SubOff, TrackRow::SubForcedOnly, TrackRow::Sub(0), TrackRow::OpenOther]);
        assert_eq!(row_of(&menu, TrackRow::OpenOther).value.as_deref(), Some("3"), "Dutch, French, German");

        focus_id(&mut menu, TrackRow::OpenOther);
        assert_eq!(menu.on_ok(store.view()), TrackOk::Navigated);
        assert_eq!(menu.page_path(), [TrackPage::OtherLanguages]);
        assert_eq!(menu.form.table.title(), Some("Other languages"));
        assert_eq!(
            menu.ids(),
            [TrackRow::Sub(5), TrackRow::OpenLang(LangId { stream: 11, slot: 1 }), TrackRow::Sub(4)],
            "Dutch, French, German"
        );
        assert_eq!(menu.selected_id(), Some(TrackRow::Sub(5)), "nothing checked: the first row");
        assert_eq!(row_of(&menu, TrackRow::OpenLang(LangId { stream: 11, slot: 1 })).value.as_deref(), Some("3 tracks"));

        focus_id(&mut menu, TrackRow::OpenLang(LangId { stream: 11, slot: 1 }));
        assert_eq!(menu.on_ok(store.view()), TrackOk::Navigated);
        assert_eq!(menu.page_path(), [TrackPage::OtherLanguages, TrackPage::Language(11)]);
        assert_eq!(menu.form.table.title(), Some("French"));
        assert_eq!(
            menu.ids(),
            [TrackRow::Sub(1), TrackRow::Sub(2), TrackRow::Sub(3)],
            "full, SDH, forced"
        );

        assert!(menu.pop(&ps, store.view()));
        assert_eq!(menu.selected_id(), Some(TrackRow::OpenLang(LangId { stream: 11, slot: 1 })), "the opener, by id");
        assert_eq!(menu.form.table.title(), Some("Other languages"));
        assert!(menu.pop(&ps, store.view()));
        assert_eq!(menu.selected_id(), Some(TrackRow::OpenOther), "the root's row");
        assert_eq!(menu.form.table.title(), None);
        teardown(&ps);
    }

    /// RIGHT on a drill-in enters it like OK; LEFT on a page pops, never switches tab.
    #[test]
    fn right_enters_a_language_and_left_leaves_it() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open(subs());
        focus_id(&mut menu, TrackRow::OpenOther);
        menu.on_right(&ps, store.view());
        focus_id(&mut menu, TrackRow::OpenLang(LangId { stream: 11, slot: 1 }));
        menu.on_right(&ps, store.view());
        assert_eq!(menu.page_path(), [TrackPage::OtherLanguages, TrackPage::Language(11)]);
        menu.on_left(&ps, store.view());
        menu.on_left(&ps, store.view());
        assert!(menu.page_path().is_empty());
        assert_eq!(menu.tab, 1, "still the Subtitles tab");
        teardown(&ps);
    }

    /// **Only image subtitles carry a format badge**: PGS shows one, a text track (SRT) none — on
    /// the Other languages page and on a language page alike.
    #[test]
    fn the_format_badge_is_only_on_image_subtitles() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, _store) = open(subs_with_vobsub());
        menu.push(TrackPage::OtherLanguages);
        let badges = |m: &TrackMenuState, id| row_of(m, id).badges.iter().map(|b| b.text().to_string()).collect::<Vec<_>>();
        assert_eq!(badges(&menu, TrackRow::Sub(5)), ["PGS"], "Dutch is an image subtitle");
        assert!(badges(&menu, TrackRow::Sub(4)).is_empty(), "German SRT shows no format");
        assert!(badges(&menu, TrackRow::OpenLang(LangId { stream: 11, slot: 1 })).is_empty(), "a drill-in is not a track");
        menu.push(TrackPage::Language(11));
        assert!(badges(&menu, TrackRow::Sub(1)).is_empty(), "French full is SRT");
        assert!(badges(&menu, TrackRow::Sub(2)).is_empty(), "the SDH chip would only repeat its label");
        assert_eq!(row_of(&menu, TrackRow::Sub(2)).label, "SDH");
        assert_eq!(badges(&menu, TrackRow::Sub(3)), ["PGS"], "a forced image track still shows its format");
        assert_eq!(row_of(&menu, TrackRow::Sub(3)).label, "Forced");
        // a VobSub SDH track: its raw codec ("dvd_subtitle") shows as the short name, after the kind
        menu.push(TrackPage::Language(16));
        assert!(badges(&menu, TrackRow::Sub(6)).is_empty(), "Italian text shows no format");
        assert_eq!(badges(&menu, TrackRow::Sub(7)), ["SDH", "VOBSUB"]);
        // a row whose only badge could be the codec: the same French forced track, unlabelled
        let (mut menu2, ps2, _s2) = open(vec![
            stream(1, 0, "French", "fra", ""),
            pgs(stream(2, 1, "French", "fra", "")),
        ]);
        menu2.push(TrackPage::OtherLanguages);
        menu2.push(TrackPage::Language(1));
        assert!(badges(&menu2, TrackRow::Sub(0)).is_empty());
        assert_eq!(badges(&menu2, TrackRow::Sub(1)), ["PGS"]);
        teardown(&ps);
        teardown(&ps2);
    }

    /// **Initial focus**: Other languages opens on the checked row (the language's drill-in when
    /// the active track is inside one), a language page on its active variant, else on its first.
    #[test]
    fn a_page_opens_on_the_active_track_when_it_is_inside() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open(subs());
        menu.active_sub = 2; // French SDH
        menu.rebuild(&ps, store.view(), 1, false);
        assert!(row_of(&menu, TrackRow::OpenOther).checked, "the active track lives behind it");
        assert_eq!(row_of(&menu, TrackRow::OpenOther).value.as_deref(), Some("French"));
        focus_id(&mut menu, TrackRow::OpenOther);
        menu.on_ok(store.view());
        assert_eq!(menu.selected_id(), Some(TrackRow::OpenLang(LangId { stream: 11, slot: 1 })), "the language holding the active track");
        let french = row_of(&menu, TrackRow::OpenLang(LangId { stream: 11, slot: 1 }));
        assert!(french.checked);
        assert_eq!(french.value.as_deref(), Some("SDH"), "reads the active variant");
        menu.on_ok(store.view());
        assert_eq!(menu.selected_id(), Some(TrackRow::Sub(2)), "the active variant");
        assert!(row_of(&menu, TrackRow::Sub(2)).checked);

        // a single-track active language: its pick row on Other languages
        let (mut menu, ps2, store) = open(subs());
        menu.active_sub = 4; // German
        menu.rebuild(&ps2, store.view(), 1, false);
        focus_id(&mut menu, TrackRow::OpenOther);
        menu.on_ok(store.view());
        assert_eq!(menu.selected_id(), Some(TrackRow::Sub(4)));

        // the active track is NOT in the language: its first track
        let (mut menu, ps3, store) = open(subs());
        menu.active_sub = 0; // English, on the root
        menu.push(TrackPage::OtherLanguages);
        menu.push(TrackPage::Language(11));
        assert_eq!(menu.selected_id(), Some(TrackRow::Sub(1)));
        teardown(&ps);
        teardown(&ps2);
        teardown(&ps3);
        let _ = store;
    }

    /// A pick on a language page commits like a root pick does and dismisses the panel.
    #[test]
    fn a_pick_on_a_language_page_commits_and_dismisses() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open(subs());
        menu.push(TrackPage::OtherLanguages);
        menu.push(TrackPage::Language(11));
        focus_id(&mut menu, TrackRow::Sub(2));
        match menu.on_ok(store.view()) {
            TrackOk::Commit { commit: TrackCommit::Subtitle { stream_id, .. }, keep_open } => {
                assert_eq!(stream_id, 12);
                assert!(!keep_open, "a track pick at any depth dismisses");
            }
            other => panic!("expected a subtitle commit, got {other:?}"),
        }
        assert_eq!(menu.active_sub, 2);
        teardown(&ps);
    }

    /// French as sidecars the client can draw only while each has a `key`: an empty one is not
    /// offered on this route, which is how a language leaves the offered list under a fixed item.
    fn sidecar_french(keys: [&str; 3]) -> Vec<metadata::Stream> {
        let mut v = subs();
        for (n, key) in keys.iter().enumerate() {
            v[1 + n].external = true;
            v[1 + n].codec = "srt".into();
            v[1 + n].key = key.to_string();
        }
        v
    }

    /// **A language page never outlives its language**: the live offered list loses French while
    /// its page is open, so the stack pops to the root (opener and root restored by id). A change
    /// that leaves the language offered (one of its tracks going away) refreshes the page in place
    /// and keeps the viewer's row.
    #[test]
    fn a_vanished_language_pops_to_the_root_and_a_changed_one_refreshes_in_place() {
        let _g = plx_base::testlock::serial();
        let store = store_with(sidecar_french(["/a.srt", "/b.srt", "/c.srt"]));
        let _ = plx_media::player::sidecar::reset();
        let ps = plx_media::route::PlaybackSession::IDLE;
        let mut menu = TrackMenuState::new(&ps, store.view(), 1, vec!["eng".into()]);
        focus_id(&mut menu, TrackRow::OpenOther);
        menu.on_ok(store.view());
        focus_id(&mut menu, TrackRow::OpenLang(LangId { stream: 11, slot: 1 }));
        menu.on_ok(store.view());
        assert_eq!(menu.ids().len(), 3);
        focus_id(&mut menu, TrackRow::Sub(2));
        let measure = plx_ui::fixture::FixtureMeasure;
        menu.update(0.016, &measure, &ps, store.view());
        assert_eq!(menu.page_path(), [TrackPage::OtherLanguages, TrackPage::Language(11)], "an unchanged signature leaves it");

        // one French sidecar is no longer offered: still a page, refreshed in place
        let store2 = store_with(sidecar_french(["/a.srt", "/b.srt", ""]));
        menu.update(0.016, &measure, &ps, store2.view());
        assert_eq!(menu.page_path(), [TrackPage::OtherLanguages, TrackPage::Language(11)], "French is still offered");
        assert_eq!(menu.selected_id(), Some(TrackRow::Sub(2)), "on the row the viewer was on");
        assert_eq!(menu.ids().len(), 2, "the departed track left the page");

        // none of French is offered: its page cannot be listed any more
        let store3 = store_with(sidecar_french(["", "", ""]));
        menu.update(0.016, &measure, &ps, store3.view());
        assert!(menu.page_path().is_empty(), "popped to the root");
        assert_eq!(menu.selected_id(), Some(TrackRow::OpenOther), "on the row that opened the stack");
        assert_eq!(menu.form.table.title(), None);
        teardown(&ps);
    }

    /// With no language left to list, an open Other languages page pops too.
    #[test]
    fn the_other_languages_page_pops_when_nothing_is_left_to_list() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open(subs());
        focus_id(&mut menu, TrackRow::OpenOther);
        menu.on_ok(store.view());
        let store2 = store_with(vec![stream(10, 0, "English", "eng", "")]);
        menu.update(0.016, &plx_ui::fixture::FixtureMeasure, &ps, store2.view());
        assert!(menu.page_path().is_empty());
        assert!(menu.form.index_of(&TrackRow::OpenOther).is_none(), "and the root no longer offers it");
        teardown(&ps);
    }

    /// Open the Other languages page, then `name`'s drill-in, the way a viewer's keys would.
    fn open_language_named(menu: &mut TrackMenuState, store: &plx_data::stores::metadata::MetadataStore, name: &str) {
        focus_id(menu, TrackRow::OpenOther);
        assert_eq!(menu.on_ok(store.view()), TrackOk::Navigated);
        let at = (0..menu.form.table.n_rows().max(0) as usize)
            .find(|&i| {
                matches!(menu.form.id_at(i), Some(TrackRow::OpenLang(_)))
                    && menu.form.table.sections.iter().flat_map(|s| &s.rows).nth(i).is_some_and(|r| r.label == name)
            })
            .unwrap_or_else(|| panic!("{name} has no drill-in on the Other languages page"));
        menu.focus_row(at as c_int);
        assert_eq!(menu.on_ok(store.view()), TrackOk::Navigated);
    }

    /// **Finding 1: the harness can pick a subtitle that sits behind Other languages.** An item
    /// whose subs are all non-"yours" has no root track row at all; `track:N` still resolves, and
    /// commits by the track's own id.
    #[test]
    fn the_harness_picks_a_subtitle_behind_other_languages() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open(vec![stream(21, 0, "German", "deu", ""), stream(22, 1, "Dutch", "nld", "")]);
        // page order: Dutch (A-Z) is the first Other-languages row, German the second
        let i = menu.sub_track_for_target("track:0").expect("track:0 resolves behind Other languages");
        assert_eq!(i, 1, "track N follows page order, root tracks first");
        match menu.commit_sub_track(i, store.view()) {
            TrackOk::Commit { commit: TrackCommit::Subtitle { stream_id, .. }, .. } => assert_eq!(stream_id, 22),
            other => panic!("expected a subtitle commit, got {other:?}"),
        }
        assert_eq!(menu.active_sub, 1);
        assert_eq!(menu.sub_track_for_target("track:1"), Some(0));
        assert_eq!(menu.sub_track_for_target("track:2"), None);
        teardown(&ps);
    }

    /// `track:N` counts root tracks first, then Other languages in page order, expanding a
    /// multi-track language's own ranked page.
    #[test]
    fn the_harness_track_order_is_root_then_other_pages_expanded() {
        let _g = plx_base::testlock::serial();
        let (menu, ps, _store) = open(subs());
        // root: English(0); Other A-Z: Dutch(5), French full/SDH/forced (1,2,3), German(4)
        let order: Vec<_> = (0..7).map(|n| menu.sub_track_for_target(&format!("track:{n}"))).collect();
        assert_eq!(order, [Some(0), Some(5), Some(1), Some(2), Some(3), Some(4), None]);
        teardown(&ps);
    }

    /// **Finding 2: the menu opens on Other languages when the active track is behind it** — a
    /// codeless or unrecognised-language subtitle is never "yours", so its own row is not on the
    /// root.
    #[test]
    fn opening_with_the_active_track_behind_other_languages_focuses_that_row() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open(vec![stream(10, 0, "English", "eng", ""), stream(11, 1, "Unknown", "", "")]);
        menu.active_sub = 1;
        menu.rebuild(&ps, store.view(), 1, false);
        assert_eq!(menu.selected_id(), Some(TrackRow::OpenOther));
        teardown(&ps);
    }

    /// The same landing on the live poll's fallback, when the viewer's own row left the root.
    #[test]
    fn the_poll_fallback_lands_on_other_languages_when_the_active_track_is_behind_it() {
        use plx_media::route::{enhancement_test_session, EnhTestFixture, SubtitleEffect};
        let _g = plx_base::testlock::serial();
        let _ = plx_media::player::sidecar::reset();
        // cur_sub_sid is 999: no such stream yet
        let (ps, _sid) = enhancement_test_session(EnhTestFixture { subtitle_effect: SubtitleEffect::Embedded, ..Default::default() });
        let yours = vec!["eng".into(), "spa".into(), "fra".into()];
        let store = store_with(vec![
            stream(10, 0, "English", "eng", ""),
            stream(11, 1, "Spanish", "spa", ""),
            stream(12, 2, "French", "fra", ""),
        ]);
        let mut menu = TrackMenuState::new(&ps, store.view(), 1, yours);
        focus_id(&mut menu, TrackRow::Sub(2));
        // the three languages leave, and the playing subtitle (999) is a codeless track
        let store2 = store_with(vec![stream(999, 0, "Unknown", "", "")]);
        menu.update(0.016, &plx_ui::fixture::FixtureMeasure, &ps, store2.view());
        assert_eq!(menu.active_sub, 0);
        assert_eq!(menu.selected_id(), Some(TrackRow::OpenOther), "the viewer's row is gone: the checked row, which is Other languages");
        teardown(&ps);
    }

    /// **Finding 3: a language page is its language, not a list position.** Two earlier tracks
    /// leave while French's page is open and Dutch now starts where French used to; the page must
    /// stay French (it must never show Dutch under a French stack).
    #[test]
    fn a_language_page_survives_earlier_tracks_leaving() {
        let _g = plx_base::testlock::serial();
        let before = vec![
            stream(10, 0, "English", "eng", ""),
            stream(11, 1, "German", "deu", ""),
            stream(12, 2, "French", "fra", ""),
            stream(13, 3, "French", "fra", "SDH"),
            stream(14, 4, "Dutch", "nld", ""),
            stream(15, 5, "Dutch", "nld", "SDH"),
        ];
        let (mut menu, ps, store) = open(before);
        open_language_named(&mut menu, &store, "French");
        assert_eq!(menu.form.table.title(), Some("French"));
        let after = store_with(vec![
            stream(12, 0, "French", "fra", ""),
            stream(13, 1, "French", "fra", "SDH"),
            stream(14, 2, "Dutch", "nld", ""),
            stream(15, 3, "Dutch", "nld", "SDH"),
        ]);
        menu.update(0.016, &plx_ui::fixture::FixtureMeasure, &ps, after.view());
        if !menu.page_path().is_empty() {
            assert_eq!(menu.form.table.title(), Some("French"), "a page stays its language");
            assert_eq!(menu.ids(), [TrackRow::Sub(0), TrackRow::Sub(1)], "and lists its tracks");
        }
        teardown(&ps);
    }

    /// If the page's language is gone altogether the page pops to the root.
    #[test]
    fn a_language_page_pops_when_its_language_is_gone_whatever_the_indices() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open(subs());
        open_language_named(&mut menu, &store, "French");
        let after = store_with(vec![
            stream(10, 0, "English", "eng", ""),
            stream(14, 1, "German", "deu", ""),
            pgs(stream(15, 2, "Dutch", "nld", "")),
            stream(16, 3, "Dutch", "nld", "SDH"),
        ]);
        menu.update(0.016, &plx_ui::fixture::FixtureMeasure, &ps, after.view());
        assert!(menu.page_path().is_empty(), "French is gone: popped to the root");
        teardown(&ps);
    }

    /// **Finding 4, shrink**: French drops to one offered track while its page is open. The page
    /// stays (a language page lists whatever its language still offers), and a pop returns to the
    /// Other languages page on French's now-direct row.
    #[test]
    fn a_language_page_that_drops_to_one_track_stays_and_pops_onto_the_single_row() {
        let _g = plx_base::testlock::serial();
        let store = store_with(sidecar_french(["/a.srt", "/b.srt", "/c.srt"]));
        let _ = plx_media::player::sidecar::reset();
        let ps = plx_media::route::PlaybackSession::IDLE;
        let mut menu = TrackMenuState::new(&ps, store.view(), 1, vec!["eng".into()]);
        open_language_named(&mut menu, &store, "French");
        let store2 = store_with(sidecar_french(["/a.srt", "", ""]));
        menu.update(0.016, &plx_ui::fixture::FixtureMeasure, &ps, store2.view());
        assert_eq!(menu.page_path().len(), 2, "the page stays open at one track");
        assert_eq!(menu.form.table.title(), Some("French"));
        assert_eq!(menu.ids(), [TrackRow::Sub(1)]);
        assert!(menu.pop(&ps, store2.view()));
        assert_eq!(menu.page_path(), [TrackPage::OtherLanguages]);
        assert_eq!(menu.selected_id(), Some(TrackRow::Sub(1)), "French is a direct row now");
        teardown(&ps);
    }

    /// **Finding 4, grow**: French is a single direct row on the Other languages page and gains
    /// tracks while the viewer sits on it; focus follows it to its drill-in.
    #[test]
    fn a_single_track_language_that_grows_keeps_the_focus_on_its_new_row() {
        let _g = plx_base::testlock::serial();
        let store = store_with(sidecar_french(["/a.srt", "", ""]));
        let _ = plx_media::player::sidecar::reset();
        let ps = plx_media::route::PlaybackSession::IDLE;
        let mut menu = TrackMenuState::new(&ps, store.view(), 1, vec!["eng".into()]);
        focus_id(&mut menu, TrackRow::OpenOther);
        menu.on_ok(store.view());
        focus_id(&mut menu, TrackRow::Sub(1));
        let store2 = store_with(sidecar_french(["/a.srt", "/b.srt", "/c.srt"]));
        menu.update(0.016, &plx_ui::fixture::FixtureMeasure, &ps, store2.view());
        assert_eq!(menu.page_path(), [TrackPage::OtherLanguages]);
        let french = menu.ids().into_iter().find(|id| matches!(id, TrackRow::OpenLang(_)));
        assert!(french.is_some(), "French is a drill-in now");
        assert_eq!(menu.selected_id(), french, "French, in its new shape");
        teardown(&ps);
    }

    /// The canon tells the three new pages and their openers apart.
    #[test]
    fn the_canon_tells_the_language_pages_apart() {
        let _g = plx_base::testlock::serial();
        let hash = |menu: &TrackMenuState| {
            let mut c = Canon::new();
            menu.canon(&mut c);
            c.finish()
        };
        let (mut menu, ps, store) = open(subs());
        focus_id(&mut menu, TrackRow::OpenOther);
        let root = hash(&menu);
        menu.on_ok(store.view());
        let other = hash(&menu);
        focus_id(&mut menu, TrackRow::OpenLang(LangId { stream: 11, slot: 1 }));
        let other_french = hash(&menu);
        menu.on_ok(store.view());
        let french = hash(&menu);
        let all = [root, other, other_french, french];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a, b);
            }
        }
        teardown(&ps);
    }

    /// **Every new page fits the panel in every shipped language**: the root with its Other
    /// languages row (also while the active track lives behind it), the page, and a language page,
    /// judged by the same gates as the rest of the menu.
    #[test]
    fn the_language_pages_fit_the_panel_in_every_language() {
        use plx_platform::i18n::{language_on_this_thread_for_test};
        let mut out = Vec::new();
        for language in plx_platform::i18n::SHIPPED {
            let _g = plx_base::testlock::serial();
            let _guard = language_on_this_thread_for_test(language);
            let (mut menu, ps, store) = open(subs_with_vobsub());
            let judge = |menu: &TrackMenuState, out: &mut Vec<_>| {
                out.extend(menu.form.table.menu_cap_failure(&plx_base::fontcov::advances::ShippedMeasure, language.tag()));
                out.extend(menu.form.table.app_fit_failures(plx_ui::table::MENU_MAX_W, language.tag()));
                out.extend(menu.form.table.app_fit_failures_hugged(language.tag()));
            };
            judge(&menu, &mut out);
            for active in [-1, 2] {
                menu.active_sub = active;
                menu.rebuild(&ps, store.view(), 1, false);
                judge(&menu, &mut out);
                menu.push(TrackPage::OtherLanguages);
                judge(&menu, &mut out);
                menu.push(TrackPage::Language(11));
                judge(&menu, &mut out);
                menu.pop(&ps, store.view());
                menu.push(TrackPage::Language(16)); // the VobSub + SDH row
                judge(&menu, &mut out);
            }
            teardown(&ps);
        }
        plx_ui::table::assert_no_fit_failures(&out);
    }
}

/// **The panel's resize and page slide** (`ui::panel_motion`, `docs/player-submenus.md`,
/// "Animation"): running vs resting, an interrupted transition, the layout the card lands on, and
/// the Audio/Subtitles tab switch.
#[cfg(test)]
mod motion_tests {
    use super::tests::{store_with, stream};
    use super::*;
    use plx_base::fontcov::advances::ShippedMeasure;
    use plx_media::route::reset_player_control_for_test;

    const DT: f32 = 1.0 / 60.0;

    fn teardown(ps: &plx_media::route::PlaybackSession) {
        reset_player_control_for_test(ps);
        plx_plex::plex::reset_servers_for_test();
    }

    fn subs() -> Vec<metadata::Stream> {
        vec![
            stream(10, 0, "English", "eng", ""),
            stream(11, 1, "French", "fra", ""),
            stream(12, 2, "French", "fra", "SDH"),
            stream(14, 4, "German", "deu", ""),
        ]
    }

    fn open() -> (TrackMenuState, plx_media::route::PlaybackSession, plx_data::stores::metadata::MetadataStore) {
        let _ = plx_media::player::sidecar::reset();
        let ps = plx_media::route::PlaybackSession::IDLE;
        let store = store_with(subs());
        let mut menu = TrackMenuState::new(&ps, store.view(), 1, vec!["eng".into()]);
        // the first update places the card AT its layout: the open is the appear animation's
        frame(&mut menu, &ps, &store);
        assert!(!menu.transitioning(), "a freshly opened panel is at rest");
        (menu, ps, store)
    }

    /// One loop frame, as the dispatcher runs it: forget last frame's motion, then update. Returns
    /// whether a spring moved (what keeps the present gate awake).
    fn frame(menu: &mut TrackMenuState, ps: &plx_media::route::PlaybackSession, store: &plx_data::stores::metadata::MetadataStore) -> bool {
        plx_machine::idle::frame_begin(DT);
        menu.update(DT, &ShippedMeasure, ps, store.view());
        plx_machine::idle::present_moving()
    }

    fn run(menu: &mut TrackMenuState, ps: &plx_media::route::PlaybackSession, store: &plx_data::stores::metadata::MetadataStore, n: usize) {
        for _ in 0..n {
            frame(menu, ps, store);
        }
    }

    fn shown(menu: &TrackMenuState) -> Rect {
        menu.shown_rect(&ShippedMeasure)
    }

    fn natural(menu: &TrackMenuState) -> Rect {
        menu.panel_rect(&ShippedMeasure)
    }

    fn push_style(menu: &mut TrackMenuState, store: &plx_data::stores::metadata::MetadataStore) {
        let i = menu.form.index_of(&TrackRow::Style).expect("Style row");
        menu.focus_row(i as c_int);
        assert_eq!(menu.on_ok(store.view()), TrackOk::Navigated);
    }

    /// **A push animates, reports motion each frame it runs and stops asking for frames at rest.**
    #[test]
    fn a_push_is_animating_then_asks_for_no_more_frames_at_rest() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open();
        let root = natural(&menu);
        push_style(&mut menu, &store);
        assert!(menu.transitioning(), "the page slide is running the moment the page is pushed");
        assert!(frame(&mut menu, &ps, &store), "a running transition keeps the present gate awake");
        assert!(menu.motion.sliding());
        assert_ne!(shown(&menu), natural(&menu), "the card is between the two layouts");
        assert_ne!(natural(&menu), root, "the Style page has its own layout");

        let mut frames_moving = 1;
        while frame(&mut menu, &ps, &store) {
            frames_moving += 1;
            assert!(frames_moving < 240, "the transition never settles");
        }
        assert!((6..120).contains(&frames_moving), "a transition spans a handful of frames, not {frames_moving}");
        assert!(!menu.transitioning());
        assert!(!menu.motion.sliding());
        assert_eq!(shown(&menu), natural(&menu), "the card lands EXACTLY on the layout");
        // idle is truly idle: a settled panel steps without a single frame requested
        for _ in 0..30 {
            assert!(!frame(&mut menu, &ps, &store), "a settled panel must not ask for frames");
        }
        teardown(&ps);
    }

    /// **A push then a pop mid-slide reverses from where it is and settles on the root.** The
    /// page that was arriving becomes the one returned to, carrying its own alpha, and the page
    /// that was live leaves from its own: no layer's alpha steps at the swap.
    #[test]
    fn an_interrupted_push_then_pop_reverses_and_settles_at_the_root_rect() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open();
        let root = natural(&menu);
        let root_shown = shown(&menu);
        assert_eq!(root_shown, root);
        push_style(&mut menu, &store);
        run(&mut menu, &ps, &store, 11);
        let mid = shown(&menu);
        assert_ne!(mid, root, "mid-slide the card is not at the root layout");
        let before = menu.motion.live_dx();
        assert!(before > 0.0, "a push arrives from the right: {before}");
        let (leaving_before, live_before) = menu.motion.alphas();
        assert!(live_before > 0.0 && live_before < 1.0, "caught mid-fade: {live_before}");

        assert!(menu.pop(&ps, store.view()));
        assert_eq!(shown(&menu), mid, "the reversal continues from the drawn rect, no jump");
        assert!(menu.motion.sliding(), "the slide reverses rather than ending");
        let (leaving_after, live_after) = menu.motion.alphas();
        assert_eq!(leaving_after, vec![live_before], "the page that was live leaves from its own alpha");
        let revived = leaving_before.last().copied().unwrap_or(0.0);
        assert_eq!(live_after, revived, "the page returned to continues from its own alpha");
        let after = menu.motion.live_dx();
        assert!(after <= 0.0, "a pop returns the root from the left, not the right: {after}");
        assert!(menu.transitioning());

        let mut n = 0;
        while frame(&mut menu, &ps, &store) {
            n += 1;
            assert!(n < 240, "the reversed transition never settles");
        }
        assert_eq!(shown(&menu), root, "settles on the root layout");
        assert!(!menu.transitioning());
        assert!(menu.pages.is_empty());
        assert_eq!(menu.selected_id(), Some(TrackRow::Style), "the opener is focused again");
        teardown(&ps);
    }

    /// **The two pages never read on top of each other**: on every frame of a push, a pop and an
    /// interrupted one, at most one layer is above the gate alpha (`panel_motion::GATE`), and the leaving page draws no
    /// focus pill (the arriving page owns the only one).
    #[test]
    fn the_two_pages_are_never_both_above_the_gate_and_only_one_draws_a_pill() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open();
        let check = |menu: &TrackMenuState, what: &str| {
            let (leaving, live) = menu.motion.alphas();
            let loudest = leaving.iter().copied().fold(0.0_f32, f32::max);
            assert!(loudest.min(live) <= plx_ui::panel_motion::GATE + 1e-4, "{what}: leaving {leaving:?} and live {live} overlap");
            assert_eq!(menu.motion.leaving_pills(), 0, "{what}: a leaving page draws a focus pill");
        };
        push_style(&mut menu, &store);
        let mut saw_leaving_only = false;
        let mut saw_live_only = false;
        for i in 0..90 {
            frame(&mut menu, &ps, &store);
            check(&menu, &format!("push frame {i}"));
            let (leaving, live) = menu.motion.alphas();
            saw_leaving_only |= !leaving.is_empty() && live == 0.0;
            saw_live_only |= leaving.is_empty() && live > 0.0 && live < 1.0;
        }
        assert!(saw_leaving_only, "the outgoing page is alone on screen at first");
        assert!(saw_live_only, "and the incoming page alone at the end of its fade");
        assert!(menu.pop(&ps, store.view()));
        for i in 0..90 {
            frame(&mut menu, &ps, &store);
            check(&menu, &format!("pop frame {i}"));
        }
        // interrupted at every depth of a push
        for stop in [1, 3, 6, 9, 12, 20] {
            push_style(&mut menu, &store);
            run(&mut menu, &ps, &store, stop);
            check(&menu, &format!("before the interrupt at {stop}"));
            assert!(menu.pop(&ps, store.view()));
            check(&menu, &format!("right after the interrupt at {stop}"));
            for i in 0..90 {
                frame(&mut menu, &ps, &store);
                check(&menu, &format!("interrupted at {stop}, frame {i}"));
            }
        }
        teardown(&ps);
    }

    /// **A push during a push is continuous too**: the page that was arriving leaves from the
    /// alpha and offset it had, the older leaving page keeps fading, and the new page starts clear.
    #[test]
    fn a_push_during_a_push_steps_no_layer() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open();
        push_style(&mut menu, &store);
        run(&mut menu, &ps, &store, 12);
        let (old_leaving, live) = menu.motion.alphas();
        let dx = menu.motion.live_dx();
        assert!(live > 0.0 && live < 1.0, "mid-fade: {live}");
        let i = menu.form.index_of(&TrackRow::OpenField(StyleField::Size)).expect("Size row");
        menu.focus_row(i as c_int);
        assert_eq!(menu.on_ok(store.view()), TrackOk::Navigated);
        let (leaving, new_live) = menu.motion.alphas();
        assert_eq!(new_live, 0.0, "the new page arrives from transparent");
        assert_eq!(leaving.last().copied(), Some(live), "the page that was arriving leaves from its own alpha");
        for (kept, was) in leaving.iter().zip(old_leaving.iter()) {
            assert_eq!(kept, was, "an older leaving page is not touched by the swap");
        }
        assert_eq!(menu.motion.leaving_pills(), 0);
        assert_ne!(dx, 0.0);
        run(&mut menu, &ps, &store, 240);
        assert!(!menu.motion.sliding());
        assert_eq!(shown(&menu), natural(&menu));
        teardown(&ps);
    }

    /// Rapid repeated pushes and pops (a held key, a double press) never stack layers or leave the
    /// card off its layout.
    #[test]
    fn rapid_pushes_and_pops_leave_one_slide_and_land_on_the_logical_page() {
        let _g = plx_base::testlock::serial();
        let (mut menu, ps, store) = open();
        let root = natural(&menu);
        for _ in 0..4 {
            push_style(&mut menu, &store);
            frame(&mut menu, &ps, &store);
            assert!(menu.pop(&ps, store.view()));
            frame(&mut menu, &ps, &store);
        }
        // and a push that goes deeper while the first is still sliding
        push_style(&mut menu, &store);
        run(&mut menu, &ps, &store, 2);
        let i = menu.form.index_of(&TrackRow::OpenField(StyleField::Size)).expect("Size row");
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
                ("style page (3 dubs)", [1824.0, 764.0, 440.0, 296.0]),
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
