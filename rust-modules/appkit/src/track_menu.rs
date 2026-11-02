Warning: truncated output (original token count: 98196)
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
            TrackPage::OtherLanguage…68196 tokens truncated…ert!(badges(&menu, TrackRow::Sub(2)).is_empty(), "the SDH chip would only repeat its label");
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
