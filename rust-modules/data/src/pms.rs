Warning: truncated output (original token count: 60956)
Total output lines: 4758

//! Plex library fetch/parse into the private catalog (was src/pms.c), read by the UI
//! via the retained publication (`hubs_snapshot()` → `HubsView`) and movie()/hub_item().
//! The fetch + JSON parse go through the typed `plx_plex::plex` client (serde DTOs) — no
//! hand-built paths or `Value` scraping here.
//!
//! **This is the data module `stores::hubs` (`docs/stores-as-machines.md`) is a machine over** —
//! each production `Bridge` owns a `stores::hubs::HubsStore`, whose `run`/`run_with_directory`
//! forward each `HubsCmd` variant into `request_refetch_hubs`/`reset`/`edit_item`/`page`/
//! `cancel_page` here against its own `PmsState`/`Arc<PmsAdapter>`, the same relationship the owned
//! `BrowseStore::run` has to Browse commands against its explicit state. Every step captures
//! its immutable request and emits it as a `StoreJob::Hubs` for the app's runner to launch or refuse. **The cross-module entry points
//! (`run_with_directory`, `land_with_directory`, `tick*`) cannot be scoped narrower than `pub`, and
//! that is a fact about Rust module topology, not an oversight**: `pms` and `stores` are both
//! top-level children of the crate root, so neither is an ancestor of the other, and `pub(in path)`
//! requires `path` to name an ancestor of the ITEM's own module. There is no visibility keyword
//! that means "visible to `stores::hubs` and nobody else" for an item defined here. What IS
//! enforceable, and is: (1) anything with no caller outside this file at all — `hub_state` — is
//! plain private, not `pub`; (2) every mutator `stores::hubs` (or a test) can reach —
//! `request_refetch_hubs`, `edit_item`, `tick`, `apply_landing`, `reset`, and the `_for_test`
//! seeds — asserts `plx_base::testlock::held()` under `#[cfg(test)]` before it touches the
//! crate-wide test-only state those seeds still share (see `lib.rs::testlock` and D5), which is the
//! runtime half of the same contract a compile-time visibility keyword cannot express across two
//! sibling modules. `ci/allow/mutators.txt`'s `# count: 0` already proves no PRODUCTION line
//! outside `pms`/`stores::hubs` spells the old direct-call form; this is the part that
//! keyword-level `pub(super)` genuinely cannot add on top of that count.
use crate::stores::{deck, paging};
use crate::viewstate::{ViewAt, ViewEdit};
use plx_machine::job::{Job, Port};
use plx_machine::machine::{Addr, MachineId, RequestId};
use plx_plex::plex::{ClientKey, ClientKeys, ServerId};
use std::os::raw::c_int;
use std::panic::catch_unwind;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

pub mod record;
pub mod initial;

/// The rows a paged store keeps cards for, and the cut applied to what a screen asks: defined in
/// `plx_base::hold`, because `ui::cards::HoldBand` applies the very same cut to know what the
/// store kept (the store and `ui` cannot name each other).
pub(crate) use plx_base::hold::{clamp_hold, HOLD_ROWS};

/// Cards one shelf holds at once: the size of a WINDOW over the row's listing, which the grid can
/// address (the owned Home's `MAX_ITEMS` is this constant). It is not a limit on the row: past it
/// the rest is reached by sliding the window (`land_page`, the merged deck's `stores::deck`).
///
/// The MERGED deck's window reaches it: three sources' Continue Watching is up to 36 cards, and a
/// Recently Added window reaches 24 from one server as the user scrolls (the window replaces its
/// overlapping pages). The home grid's focus ring and its OK dispatch clamp differently past this
/// number — the ring stops at the last addressable card while the press opens whatever column the
/// raw index names. Holding the window to this size keeps the two from ever disagreeing.
pub const MAX_SHELF_ITEMS: usize = 24;

/// `PmsMovie::kind` of a movie, a TV show, a season and an episode (a collection is 4).
pub const KIND_MOVIE: c_int = 0;
pub const KIND_SHOW: c_int = 1;
pub const KIND_SEASON: c_int = 2;
pub const KIND_EPISODE: c_int = 3;
pub const KIND_COLLECTION: c_int = 4;

pub fn listable(type_str: &str) -> bool {
    matches!(type_str, "movie" | "show" | "season" | "episode")
}

/// Items asked of each hub endpoint, per source. `/hubs?count=` is items-per-hub, so this bounds
/// a shelf, never the number of shelves.
pub(crate) const HUB_FETCH_COUNT: i64 = 12;

/// `PmsMovie::season_index` of an EPISODE whose server sent no `parentIndex`. Distinct from `0`
/// (Specials), so a `Scope::Season { index: 0 }` change does not reach a row whose season is merely
/// unknown. Every reader that shows or opens by season already tests `> 0`.
pub const SEASON_UNKNOWN: c_int = -1;

/// A catalog row — owned strings (the old C-ABI fixed `[u8; N]` buffers are gone; no C
/// consumer remains). Fields pub so the UI / route / player read them directly.
#[derive(Default, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, remote = "Self")]
pub struct PmsMovie {
    /// WHICH SERVER this row came from. Every other identity on it — `rk`, `show_rk`, `part` — is a
    /// server-local key that a second server reuses from 1 (docs/shared-servers.md §2 measured the
    /// collision), so the row is only addressable as the PAIR `(sid, rk)`; see
    /// [`plx_plex::plex::same_item`]. Stamped by [`parse_item`] from a value the SPAWNING thread
    /// captured, never from `plex::current_server()` inside the worker — `parse_item` runs on the
    /// hub, page and person workers, and by the time one of them parses, "the current server" may
    /// already be a different machine than the one whose bytes it is holding.
    #[serde(with = "record::server_id")]
    pub sid: ServerId,
    /// The LIBRARY on `sid` this row came from (`librarySectionID`), 0 when the server sent none.
    /// The pin's grain: a whole-server `/hubs` answers with rows from every library, so this is the
    /// only thing that can keep an UNPINNED library's items off Home without a per-library fetch.
    pub sec: i64,
    pub title: String,
    pub year: c_int,
    pub rating: String,
    pub dur_ns: i64,
    pub part: String,
    pub thumb: String,
    /// The item's OWN thumb where [`PmsMovie::thumb`] holds a substitute — i.e. an episode's 16:9
    /// still, empty on everything else. A landscape tile draws this; a portrait card draws `thumb`.
    pub still: String,
    pub art: String,
    pub summary: String,
    pub rk: String,
    pub vcodec: String,
    pub acodec: String,
    #[serde(with = "record::blur_bits")]
    pub blur: [[f32; 3]; 4],
    pub has_blur: bool,
    pub kind: c_int, // 0 = movie, 1 = show, 2 = season, 3 = episode, 4 = collection
    pub resume_ms: i64,  // viewOffset — drives the Continue Watching resume bar
    pub show_rk: String, // parent show rk (episode: grandparent; season: parent)
    /// The title's catalog id, decoded once from `guid` (`plex://movie|show/<24 hex>`); `None` when
    /// the listing carried none or it is not that form: the app then reads the item's own server
    /// record for it (`plex::models::resolve_catalog_hex`, via `WatchlistStore::want_hex`) and the
    /// next menu offers the row once the record named a portable id.
    pub guid: Option<[u8; 12]>,
    /// The parent show's catalog id, from `grandparentGuid` (episode); `None` as above.
    pub show_guid: Option<[u8; 12]>,
    pub season_index: c_int, // season number (episode: parentIndex, or [`SEASON_UNKNOWN`]; season: index)
    pub show_title: String, // episode: grandparentTitle; season: parentTitle
    pub ep_index: c_int, // episode only: episode number within the season
    /// Fully unwatched (movie/episode: no viewCount; show/season: zero viewed leaves).
    pub unwatched: bool,
    /// Fully **watched** — and deliberately NOT `!unwatched`, which is the trap this field exists to
    /// close. For a movie or episode the two are the same thing, but for a SHOW or SEASON
    /// `!unwatched` only means "at least one episode has been played", so a series you are three
    /// episodes into satisfies it. The tile mark is a claim of DONE (`ui::widgets::poster_mark`), so
    /// it needs `viewedLeafCount >= leafCount` instead: partly-watched sits with never-started under
    /// "no mark", because the honest statement about a show mid-run is the resume state of its next
    /// episode, which a poster in a grid does not have. Caught by a device capture — a library
    /// filtered to `unwatchedLeaves=1` had five tiles wearing a watched disc.
    ///
    /// The comparison is the house rule, not a new one: it is `metadata::Season::watched`'s, and the
    /// same one `fetch_detail` applies to a show — including the load-bearing `leaf_count > 0` half,
    /// without which a container the server sent no counts for is `0 >= 0` and reads as watched.
    pub watched: bool,
    /// `originallyAvailableAt`, verbatim (`YYYY-MM-DD`) or empty — the RELEASE DATE an episode
    /// shelf trails under a focused tile (`Library Screens.dc.html` E: "focus adds the episode's
    /// name and its one trailing fact — time left on Continue Watching, release date on Recently
    /// Released"). Formatted by `plx_ui::fmt::pretty_date`, which already takes `year` as the
    /// fallback for an item the server dated only to a year.
    pub aired: String,
    /// A collection's member count (`childCount`) — its tile's caption, "12 items". 0 on every
    /// other kind, where the listing's count fields mean leaves rather than members.
    pub child_count: i64,
    // ---- CONTRACT FREEZE (library/collections redesign, P2 fills the parse in `parse_item`) ----
    // The facts the "Under posters" caption reads (`plx_ui::caption_policy`) and the collection
    // tile's caption. All default to "unknown"; `child_count` above is generalised by P2 to a
    // show's season count as well (a season-count reader checks `kind`).
    /// `subtype` verbatim (a collection's `movie` / `show` / `episode` …), or `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,
    /// A SMART collection (`smart="1"`): a saved filter, not a hand-picked set.
    #[serde(default, skip_serializing_if = "is_false")]
    pub smart: bool,
    /// The first `Genre` tag, or `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    /// The first media's `videoResolution` (`4k`, `1080`, `720`, `sd` …), or `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_resolution: Option<String>,
    /// The first video stream is HDR (HDR10 / HLG / Dolby Vision).
    #[serde(default, skip_serializing_if = "is_false")]
    pub hdr: bool,
    /// `audienceRating` (0..10), or `None` when the server sent none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience_rating: Option<f32>,
    /// `leafCount` — a show/season's episode count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leaf_count: Option<u32>,
    /// `viewedLeafCount` — episodes watched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewed_leaf_count: Option<u32>,
    /// `addedAt`, seconds since the epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added_at: Option<i64>,
    /// Every version of a leaf that has more than one (`Media[]`, in the server's order), for the
    /// item menu's Play Version row; empty for a single-version item.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<MediaVersion>,
    /// Listing-only sort facts of a movie, show, season or episode row. No detail fetch is needed
    /// to caption a library grid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_info: Option<SortInfo>,
}

// Remote derive keeps the field rules in one place while allowing non-inlined trait entry
// points. The retained stores and recording adapters otherwise inline separate copies of this
// large row's typed codec into their generic visitors (tens of KB in the ARM release binary).
impl serde::Serialize for PmsMovie {
    #[inline(never)]
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        PmsMovie::serialize(self, serializer)
    }
}

impl<'de> serde::Deserialize<'de> for PmsMovie {
    #[inline(never)]
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        PmsMovie::deserialize(deserializer)
    }
}

/// Facts lost by the compact poster projection before sort captions were introduced.
/// Values and provider images always stay paired; missing values remain unknown.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SortInfo {
    pub rating: Option<f64>,
    pub rating_image: String,
    pub audience_image: String,
    pub user_rating: Option<f64>,
    pub view_count: Option<i64>,
    pub last_viewed_at: Option<i64>,
    pub bitrate_kbps: Option<i64>,
    pub width: i64,
    pub height: i64,
    pub media_count: usize,
}

impl SortInfo {
    fn of(it: &plx_plex::plex::Metadata) -> Self {
        let score = |n: f64| (n.is_finite() && n > 0.0 && n <= 10.0).then_some(n);
        let media = it.media_version(None).map(|(_, media)| media);
        Self {
            rating: score(it.rating), rating_image: it.rating_image.clone(),
            audience_image: it.audience_rating_image.clone(), user_rating: score(it.user_rating),
            view_count: (it.view_count >= 0).then_some(it.view_count),
            last_viewed_at: (it.last_viewed_at > 0).then_some(it.last_viewed_at),
            bitrate_kbps: media.and_then(|m| (m.bitrate > 0).then_some(m.bitrate)),
            width: media.map_or(0, |m| m.width), height: media.map_or(0, |m| m.height),
            media_count: it.media.len(),
        }
    }
}

fn is_false(b: &bool) -> bool { !*b }

/// **One version of an item, as the version picker describes it**: the facts a listing carries
/// for each `Media[]` entry, keyed by the identity a play is asked for by
/// ([`plx_plex::plex::MediaId`], never a position).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MediaVersion {
    pub id: plx_plex::plex::MediaId,
    /// Whole-file bitrate, kbps.
    pub kbps: i64,
    /// PMS's resolution class (`4k`, `1080`), with the frame size as its fallback.
    pub resolution: String,
    pub width: i64,
    pub height: i64,
    pub vcodec: String,
    pub acodec: String,
    pub channels: i64,
    pub container: String,
    /// The first part's size in bytes, 0 when unsent.
    pub size: i64,
    /// A video stream is HDR or Dolby Vision. Only a response that carries streams can say so (a
    /// listing usually does not), so `false` is "not known to be".
    pub hdr: bool,
}

/// The versions worth a picker: every `Media[]` entry with an id when there are at least two,
/// else none.
pub fn versions_of(it: &plx_plex::plex::Metadata) -> Vec<MediaVersion> {
    if it.media.len() < 2 || it.media.iter().any(|m| m.id.0 == 0) {
        return Vec::new();
    }
    it.media
        .iter()
        .map(|m| {
            let part = m.part.first();
            MediaVersion {
                id: m.id,
                kbps: m.bitrate,
                resolution: clean(&m.video_resolution),
                width: m.width,
                height: m.height,
                vcodec: clean(&m.video_codec),
                acodec: clean(&m.audio_codec),
                channels: m.audio_channels,
                container: clean(part.map_or(&m.container, |p| if p.container.is_empty() { &m.container } else { &p.container })),
                size: part.map_or(0, |p| p.size),
                hdr: part.is_some_and(|p| p.stream.iter().any(|s| s.stream_type == 1 && s.is_hdr())),
            }
        })
        .collect()
}

impl PmsMovie {
    /// **The Watchlist title this row stands for**, as `(kind, catalog id, from_episode)`: a movie or
    /// a show is itself, a season or an episode is its SHOW (`from_episode`, so the labels say
    /// "Show"). The id is the one the row was decoded with, `None` when the server sent no
    /// `plex://movie|show` guid; a collection (kind 4) stands for no title at all. THE rule for the
    /// item menu's row and the Home hero's bookmark disc, so the two cannot name different titles.
    pub fn watchlist_subject(&self) -> Option<(crate::watchlist::model::Kind, Option<[u8; 12]>, bool)> {
        use crate::watchlist::model::Kind;
        match self.kind {
            0 => Some((Kind::Movie, self.guid, false)),
            1 => Some((Kind::Show, self.guid, false)),
            2 | 3 => Some((Kind::Show, self.show_guid, true)),
            _ => None,
        }
    }

    /// Played fraction for the amber resume bar, or None when not in progress — THE one
    /// resume-bar rule, shared by the home shelves and the Library grid (it was copy-pasted
    /// into both screens before), and the definition of `PosterMark::InProgress`.
    ///
    /// **A resume point at or past the end is NOT in progress.** That is a finished item whose
    /// `viewOffset` the server never cleared, and counting it as in-progress drew a 100%-full bar
    /// that read as a rendering bug — and, once the poster's mark became the watched disc
    /// (2026-08-13), also suppressed the disc that item should be wearing, so a finished movie could
    /// end up with a full bar and no check. `ui::detail::ep_state` has always applied this rule to
    /// an episode still; now a poster and the filmstrip beside it cannot describe one item two ways.
    pub fn resume_frac(&self) -> Option<f32> {
        (self.resume_ms > 0 && self.dur_ns > 0
            && self.resume_ms * 1_000_000 < self.dur_ns)
            .then(|| (self.resume_ms as f32 * 1_000_000.0 / self.dur_ns as f32).clamp(0.0, 1.0))
    }
}

// Published as one immutable allocation, on the main thread. Owned readers retain the Arc,
// so a later store commit cannot invalidate their data. Legacy accessors below still have the
// old main-thread/until-next-commit lifetime and are retired with their screens.
#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HomeCatalog {
    items: Vec<Arc<PmsMovie>>,
    hubs: Vec<HubRow>,
    heroes: Vec<HeroSlot>,
}
static EMPTY_HOME: LazyLock<Arc<HomeCatalog>> = LazyLock::new(|| Arc::new(HomeCatalog::default()));

/// One Hubs owner's main-thread-only logical state (`docs/stores-as-machines.md`). Production
/// gains one through `stores::hubs::HubsStore`; the worker-touched half is [`PmsAdapter`].
pub struct PmsState {
    /// The publish order of view-state edits; each source holds a clone (see `Src::ledger`).
    pub(crate) ledger: crate::viewledger::ViewLedger,
    published: Option<Arc<HomeCatalog>>,
    /// The source table, in display order: our own servers first, then each shared one. Main
    /// thread only, so no lock is needed once this lives per-owner rather than behind a global.
    srcs: Vec<Src>,
    /// What the source table was last built from — the registry's exact roster generation and the
    /// pinned-library set. See the retired `SEEN` static's doc.
    seen: u64,
    /// …and what the roster SAID at the time. See the retired `SEEN_FACTS` static's doc.
    seen_facts: u32,
    /// Bumped by every authoritative fetch. See the retired `HUB_GEN` static's doc.
    hub_gen: u32,
    /// The retained Browse directory's semantic pin fingerprint as of the last merge. See the
    /// retired `LAST_SECTIONS_GEN` static's doc.
    last_sections_gen: u32,
    /// Moves every time the published catalog is replaced. See the retired `CATALOG_GEN` static's
    /// doc.
    pub catalog_gen: u32,
    /// Which sources may have a hub fetch out this tick ([`crate::stores::fanout`]).
    fanout: crate::stores::fanout::Fanout,
    /// A Continue Watching move waiting for the servers whose lanes it had to read.
    deck_ask: Option<DeckAsk>,
    /// The rows (catalog order, inclusive) the screen holds cards for; see [`HOLD_ROWS`].
    hold: (usize, usize),
    /// Moves whenever a quiet read stages an answer, so a held verdict is keyed on what it judged.
    staged_gen: u32,
    /// The last [`HubsCmd::Publish`] verdict that was to WAIT, and what it was a verdict on: the
    /// same question asked again next frame is answered from this, not by merging again.
    publish_memo: Option<PublishKey>,
    /// The fetches this owner's steps have asked for and nobody has started yet, in the order they
    /// were asked ([`crate::stores::StoreJob::Hubs`]). A step only pushes here; the application's
    /// runner takes them ([`crate::stores::hubs::HubsStore::take_jobs`]) and starts each, because
    /// starting a worker is IO and a step does none. A `Reset` clears it: the sources the jobs were
    /// asked for are gone with it.
    pub(crate) jobs: Vec<crate::stores::StoreJob>,
}

/// Everything a [`publish_staged`] verdict depends on.
#[derive(Clone, Copy, PartialEq, Eq)]
struct PublishKey {
    catalog_gen: u32,
    staged_gen: u32,
    hold: (usize, usize),
    visible: bool,
    focused_hub: Option<usize>,
    press_live: bool,
}

/// The id of the merged Continue Watching row, which a page ask names instead of one server's hub.
const DECK_ID: &str = "home.continue";

/// One move of the Continue Watching deck that is waiting on server reads: it runs when every
/// server it asked has landed or is retrying, so one failing server never holds the others.
struct DeckAsk {
    before: bool,
    /// How many pages the move is: more than one once a window of hidden copies was passed over.
    pages: usize,
    waiting: Vec<ServerId>,
}

impl Default for PmsState {
    fn default() -> Self {
        Self {
            ledger: Default::default(),
            published: None,
            srcs: Vec::new(),
            seen: u64::MAX,
            seen_facts: u32::MAX,
            hub_gen: 0,
            last_sections_gen: 0,
            catalog_gen: 0,
            fanout: Default::default(),
            deck_ask: None,
            hold: (0, HOLD_ROWS - 1),
            staged_gen: 0,
            publish_memo: None,
            jobs: Vec::new(),
        }
    }
}

/// The `Arc`'d worker half of one Hubs owner: the landing mailbox and the request-id minter. A
/// job's [`HubsPort`] holds a clone of the owning `Bridge`'s `Arc<PmsAdapter>` from the step that
/// emitted it; rotating the store's live `Arc` (on `HubsCmd::Reset`) orphans that clone harmlessly
/// — the old worker can still land, but only into a mailbox nothing reads any more.
pub struct PmsAdapter {
    results: Mutex<Vec<Landing>>,
    /// Request-id allocation, shared across sources so an addressed Hubs result has a unique
    /// request id even when two servers are both on their first fetch. Per-adapter (not
    /// process-wide) is enough: a still-running old worker captured the RETIRED adapter and can
    /// only ever mint (or land) into it, never into the one a reset rotated in.
    next_request: AtomicU32,
    /// Fetches the runner has launched ([`launch_hubs_job`]) whose landing [`take_landings`] has not
    /// yet taken: `+1` on the main thread before the worker exists ([`HubsPort::claim`]), `-1` when
    /// the OS refuses the spawn, `-n` when a take moves `n` landings out. It is the claim dump
    /// mode's `take_all_owed` reads ([`owed`]): the blocking dump waits on it, the paced one starves
    /// the take and the frame dump holds the iteration. It has to live here rather than on
    /// [`Src::fetching`], which clears when a landing is APPLIED, after the take that delivered it.
    /// Every launched worker answers exactly once (a panicking fetch posts a failure), so it
    /// reaches zero. It is raised at the LAUNCH, never at the emission of a job: a job the runner
    /// never launches (a replay's recorded admission, a refusal, a client the registry no longer
    /// holds) holds no claim, and `Port` has no drop guard that could lower one. It lives on the
    /// adapter, so a reset's rotation retires the old count with the old mailbox.
    owed: AtomicU32,
    /// Fetches the runner REFUSED since the last Hubs step, by request id: the launch did not
    /// happen (the OS would not start a thread, the registry no longer holds the client the step
    /// decided on, or a replay's recorded admission was `false`). Nothing will ever fill the
    /// mailbox for these, so the next step settles each as the failure it is ([`settle_refused`])
    /// before it reads anything else. A request id is minted by this adapter alone, so it names one
    /// request; the source that still has it in flight is the one it belongs to. It is NOT the
    /// landing mailbox: only `results` becomes an `AppMsg::HubsResult`, which the recorder writes,
    /// so a refusal reaches no recorded byte and replay settles it from its own recorded admission.
    /// It lives on the adapter, so a reset's rotation retires it with the mailbox.
    refused: Mutex<Vec<u32>>,
    /// Test only: how many times [`take_landings`] has finished, so a fake worker can post only
    /// AFTER a take has found its mailbox empty.
    #[cfg(any(test, feature = "test-support"))]
    takes: AtomicU32,
}

impl Default for PmsAdapter {
    fn default() -> Self {
        Self {
            results: Mutex::new(Vec::new()),
            next_request: AtomicU32::new(1),
            owed: AtomicU32::new(0),
            refused: Mutex::new(Vec::new()),
            #[cfg(any(test, feature = "test-support"))]
            takes: AtomicU32::new(0),
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
pub fn owed_count_for_test(adapter: &PmsAdapter) -> u32 { adapter.owed.load(Ordering::SeqCst) }

/// Test hook: the request ids of the launches refused since the last step, in the order they were
/// answered. The next step settles them ([`settle_refused`]) and empties the list.
#[cfg(any(test, feature = "test-support"))]
pub fn refused_for_test(adapter: &PmsAdapter) -> Vec<u32> {
    adapter.refused.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Test hook: how many authoritative hub refetches this owner has started (`RefetchHubs` bumps it).
#[cfg(any(test, feature = "test-support"))]
pub fn hub_gen_for_test(state: &PmsState) -> u32 { state.hub_gen }

/// Is a worker the runner launched ([`launch_hubs_job`]) still owing this adapter a landing? Read by the
/// landing gate's dump mode, which delivers the answer a request it issued owes
/// (`plx_machine::landgate::Gate::take_all_owed`): the blocking dump waits for it, the paced dump
/// (the frame dump's) returns nothing and names the store until it is in.
pub fn owed(adapter: &PmsAdapter) -> bool {
    adapter.owed.load(Ordering::SeqCst) > 0
}

/// Has the source `sid` nothing out and nothing waiting for room: its read has landed or failed
/// (a refused launch fails at the next step, [`settle_refused`]), and the fan-out gate is not holding one back for it? True
/// for a slot the roster does not hold, which has nothing to wait for. Unlike [`owed`] this is ONE
/// source's answer, so a slow friend's server does not make everyone else's landing wait. A QUIET
/// read in flight does not count: this asks whether a prompted read is out.
pub fn source_settled(state: &PmsState, sid: ServerId) -> bool {
    // a quiet read is nobody's wait: the return hold ends on the prompted read alone
    state.srcs.iter().find(|s| s.sid == sid).is_none_or(|s| s.quiet || (!s.fetching && !s.deferred))
}

fn published_home(state: &PmsState) -> &Arc<HomeCatalog> {
    state.published.as_ref().unwrap_or(&EMPTY_HOME)
}

#[cfg(any(test, feature = "test-support"))]
fn catalog(state: &PmsState) -> &Vec<Arc<PmsMovie>> {
    &published_home(state).items
}

/// catalog row `i`, or None. [`commit`] is the one mutation and re-resolves the open surfaces
/// itself.
#[cfg(any(test, feature = "test-support"))]
pub fn movie(state: &PmsState, i: usize) -> Option<&PmsMovie> {
    catalog(state).get(i).map(|m| &**m)
}
/// Catalog index of the row `(sid, rk)` names, or -1.
///
/// **Server-scoped, and that is the whole point.** This used to scan `m.rk == rk` over one flat
/// catalog, which is unambiguous only while every row comes from one machine. On a Continue
/// Watching shelf merged across servers it is not: a friend's episode and one of ours can carry the
/// same ratingKey, so a bare-key scan returns whichever row is EARLIER — and the caller that
/// exposed it was `detail::mount_rk` (which then mounts the wrong backdrop, blur envelope and
/// selection). -1 stays "not in the hub catalog", which every caller already handles as
/// "off-catalog".
///
/// The item menu's Play-from-Start was the other caller and is not one any more: it carries the row
/// it was opened on (`screens::registry::ItemMenuArg`'s row), because a Library, Search or person-page tile is in no
/// hub at all and this answered -1 for every one of them.
#[cfg(any(test, feature = "test-support"))]
pub fn index_of_rk(state: &PmsState, sid: ServerId, rk: &str) -> c_int {
    catalog(state)
        .iter()
        .position(|m| plx_plex::plex::same_item((m.sid, &m.rk), (sid, rk)))
        .map(|i| i as c_int)
        .unwrap_or(-1)
}

// ---- helpers ----
/// owned copy of a metadata string with newlines flattened to spaces (single-line UI fields)
pub(crate) fn clean(s: &str) -> String {
    s.chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect()
}

/// The facts the "Under posters" caption reads, lifted off the wire row: unknown stays `None` /
/// `false` rather than a zero a reader could mistake for a value.
fn fill_caption_facts(m: &mut PmsMovie, it: &plx_plex::plex::Metadata) {
    let some = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_owned());
    m.subtype = some(&it.subtype);
    m.smart = m.kind == KIND_COLLECTION && it.smart;
    // The wire half is `Metadata::listing_facts`, the one rule the Watchlist's matched copies read too.
    let facts = it.listing_facts();
    m.genre = facts.genre;
    m.video_resolution = facts.video_resolution;
    m.hdr = facts.hdr;
    m.audience_rating = facts.audience_rating;
    m.leaf_count = facts.leaf_count;
    m.viewed_leaf_count = facts.viewed_leaf_count;
    m.added_at = (it.added_at > 0).then_some(it.added_at);
}

/// Parse one Plex `Metadata` item (from a section listing OR a hub) into a catalog row.
/// pub: the Library browse store (`browse.rs`) and the person page (`person.rs`) map their
/// listings with it too.
///
/// `sid` is the server the response came from and is **passed in, never looked up**: all three
/// callers run this on a worker thread, and the house rule (`browse.rs`'s spawn site states it
/// outright) is that a worker reads no statics. It is also the only correct answer — the current
/// server can change while a page fetch is in flight, and the rows in hand belong to the machine
/// that was asked, not to whichever one is current when they finish parsing.
pub fn parse_item(it: &plx_plex::plex::Metadata, sid: ServerId) -> PmsMovie {
    let mut m = PmsMovie {
        sid,
        sec: it.library_section_id,
        ..Default::default()
    };
    m.aired = clean(&it.originally_available_at);
    m.guid = it.guid_hex();
    m.show_guid = it.show_guid_hex();
    m.kind = match it.kind.as_str() {
        "show" => KIND_SHOW,
        "season" => 2,
        "episode" => 3,
        "collection" => KIND_COLLECTION,
        _ => 0,
    };
    match m.kind {
        3 => {
            // episode: parent show = grandparent, season number = parentIndex
            m.show_rk = clean(&it.grandparent_rating_key);
            m.season_index = it.parent_index.map_or(SEASON_UNKNOWN, |i| i as c_int);
            m.show_title = clean(&it.grandparent_title);
            m.ep_index = it.index as c_int;
        }
        2 => {
            // season: parent show = parent, season number = index
            m.show_rk = clean(&it.parent_rating_key);
            m.season_index = it.index as c_int;
            m.show_title = clean(&it.parent_title);
        }
        _ => {}
    }
    // A COLLECTION HAS NO WATCH OR RESUME STATE of its own, whatever counters the server sends
    // with it. This is the one place that says so: both flags are false and `resume_ms` is zero
    // below, and every reader (the poster mark, `resume_frac`, the item menu) trusts the row.
    //
    // shows/seasons count leaves (a show with any watched episode is no longer "unwatched");
    // movies/episodes key on viewCount absence (docs/pms-api.md §2)
    m.unwatched = match m.kind {
        1 | 2 => it.viewed_leaf_count == 0 && it.leaf_count > 0,
        KIND_COLLECTION => false,
        _ => it.view_count == 0,
    };
    // …and DONE is its own question, not the negation of that one: for a container it takes ALL the
    // leaves, so a show three episodes in is neither (see the `watched` field's doc).
    m.watched = match m.kind {
        1 | 2 => it.leaf_count > 0 && it.viewed_leaf_count >= it.leaf_count,
        KIND_COLLECTION => false,
        _ => it.view_count > 0,
    };
    // `childCount` is a collection's member count and a show's season count; on a movie, episode
    // or season it is not sent, so the other kinds stay 0.
    if matches!(m.kind, 1 | KIND_COLLECTION) {
        m.child_count = it.child_count.max(0);
    }
    fill_caption_facts(&mut m, it);
    // A show's, a season's and an episode's listing row carries the same sort facts a movie's does
    // (`lastViewedAt`, `viewCount`, the scores and their providers, an episode's media); a container's
    // episode counts are `leaf_count` / `viewed_leaf_count` above. Only a collection keeps none.
    if matches!(m.kind, 0 | KIND_SHOW | KIND_SEASON | KIND_EPISODE) { m.sort_info = Some(SortInfo::of(it)); }
    m.title = clean(&it.title);
    m.year = it.year as c_int;
    m.rating = clean(&it.content_rating);
    m.dur_ns = if it.duration > 0 {
        it.duration * 1_000_000
    } else {
        0
    };
    m.resume_ms = if m.kind == KIND_COLLECTION { 0 } else { it.view_offset };
    // poster: prefer the show poster for episodes (grandparentThumb) so a landscape
    // episode still doesn't fill a portrait card
    let thumb = if it.grandparent_thumb.is_empty() {
        &it.thumb
    } else {
        &it.grandparent_thumb
    };
    m.thumb = clean(thumb);
    // …and the item's OWN thumb, unsubstituted. The line above is right for a POSTER shelf and
    // wrong for a landscape one, and both exist: an episode's own thumb is a 16:9 still, so Home's
    // portrait cards want the show poster, while a 420x236 tile wants the still — with the
    // substitution applied, a search for a show drew the same fanart on every episode in the row.
    //
    // Kept as a second field rather than resolved per caller because `parse_item` runs on a worker
    // and cannot know which shelf will draw the row. Empty on a movie, where `thumb` already IS
    // the item's own.
    // **Keyed on the item BEING an episode, not on the show poster existing.** It used to be
    // `grandparent_thumb.is_empty()`, which is a proxy that fails in the one direction that
    // matters: an episode whose show has no poster kept its own 16:9 still ONLY in `thumb`, and
    // `widgets::still_key` prefers `art` over `thumb` — so a landscape tile drew the show's shared
    // backdrop while the episode's own still sat right there unused, which is the opposite of the
    // documented still -> art -> poster chain.
    m.still = if m.kind == 3 {
        clean(&it.thumb)
    } else {
        String::new()
    };
    m.art = clean(&it.art);
    m.summary = clean(&it.summary);
    m.rk = clean(&it.rating_key);
    // the default version: codecs + Part[0].key (movies/episodes; a show container has none)
    if let Some((_, md)) = it.media_version(None) {
        m.vcodec = clean(&md.video_codec);
        m.acodec = clean(&md.audio_codec);
        if let Some(p0) = md.part.first() {
            m.part = clean(&p0.key);
        }
    }
    if m.kind == 0 || m.kind == 3 {
        m.versions = versions_of(it);
    }
    // UltraBlurColors -> the ambient gradient. `UltraBlurColors::corners` owns the corner ORDER and
    // the all-black-envelope guard (shared with the detail store, which keys the same wash off the
    // LOADED item); `de_ultrablur` already accepted both the array and object shapes PMS returns
    // (D-1), so blur populates where the old object-only read left it blank.
    if let Some(blur) = it.ultra_blur_colors.and_then(|u| u.corners()) {
        m.blur = blur;
        m.has_blur = true;
    }
    m
}

// The full-library browse path lives in `crate::browse` (the Library screen's per-section
// PAGED catalog — sparse store + off-thread page fetches via `section_items_query`). This
// module stays hub-only; `browse` reuses `parse_item` above for its listings.

// ---- home hubs: each hub is a titled slice of the catalog ----
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HubRow {
    title: String,
    hub_id: String, // locale-independent hubIdentifier ("home.continue", "home.movies.recent", …)
    // Provider listing path. Part of an identified hub's identity as well as the fallback when
    // `hubIdentifier` is absent: PMS reuses one identifier for section-specific Home rows while
    // publishing distinct keys. Kept verbatim — observed keys carry content-defining query terms
    // (`type`, `sectionID`, filters/sort), not the parent `/hubs?count=…` request's page size.
    key: String,
    /// Which SERVER this shelf's items came from, as the owner's handle ("friend") — empty
    /// whenever the row came from the signed-in user's own server, which is every row today.
    /// Empty is the ABSENCE of an annotation, not an empty one: the home shelf heading draws no
    /// separator and no second run at all for it (`ui::cards::heading_flow`), so the annotation costs
    /// a single-server library nothing — no gap, no dot, no draw call. (The heading's INK changed in
    /// the same pass, which is a separate, deliberate harmonization; `heading_flow`'s doc has it.)
    /// Populated by the multi-server data layer when it lands.
    source: String,
    /// Every item the shelf's listing holds, which `len` caps — a linked collection heading's
    /// "· N" (`HubRef::total`). 0 when the server named no total.
    total: usize,
    start: usize,
    len: usize,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    more: bool,
    /// The row's landing count ([`Shelf::epoch`]); not recorded.
    #[serde(skip)]
    epoch: u32,
}
fn hubs(state: &PmsState) -> &Vec<HubRow> {
    &published_home(state).hubs
}

// ---- rotating hero pool: curated catalog indices (Continue Watching then Recently Added) ----
const HERO_MAX: usize = 8;

/// One page of the rotating billboard: a catalog index, plus the handle of the SERVER the shelf it
/// was drawn from came from ("friend") — empty for the signed-in user's own, exactly as
/// [`HubRow::source`] means it.
///
/// The handle is carried on the SLOT rather than looked up from the item's shelf at draw time, and
/// that is the point of the type existing at all: the pool is the one place in the app that lifts
/// items OUT of their shelf order ([`own_items_first`] promotes an owned page to the front), so a
/// pool entry that only knew its catalog index would have to find its way back to a hub through a
/// range scan to answer "whose is this" — for a fact the build already had in its hand.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HeroSlot {
    idx: usize,
    source: String,
}
/// A retained publication, independent of subsequent store commits. Cloning copies one Arc,
/// never a movie or string. The status is captured alongside the catalog, including failed
/// fetches which keep the previous content and therefore do not move its generation.
#[derive(Clone)]
pub struct HubsSnapshot {
    data: Arc<HomeCatalog>,
    generation: u32,
    state: HubState,
}

pub fn hubs_snapshot(state: &PmsState) -> HubsSnapshot {
    HubsSnapshot {
        data: Arc::clone(published_home(state)),
        generation: state.catalog_gen,
        state: hub_state(state),
    }
}

impl HubsSnapshot {
    /// An explicit empty retained publication; fixture construction must not capture globals.
    #[cfg(any(test, feature = "test-support"))]
    pub fn empty_for_test() -> Self {
        Self { data: Arc::new(HomeCatalog::default()), generation: 0, state: HubState::Loading }
    }

    pub fn view(&self) -> HubsView<'_> {
        HubsView { data: &self.data, generation: self.generation, state: self.state }
    }
}

/// Frame-borrowed Home data. Every reference is tied to the retained publication, not a static
/// catalog that an effect could replace. The bridge owns the snapshot; a screen reads this and keeps nothing of the publication past the frame (Home keeps the `Arc` of each card a landing removes, not the catalog).
#[derive(Clone, Copy)]
pub struct HubsView<'a> {
    data: &'a Arc<HomeCatalog>,
    pub generation: u32,
    pub state: HubState,
}

#[derive(Clone, Copy)]
pub struct HubRef<'a> {
    pub identity: Option<HubIdentity<'a>>,
    pub title: &'a str,
    pub source: &'a str,
    /// Every item the listing holds, or zero when unknown. `items` is the current window.
    pub total: usize,
    pub items: &'a [Arc<PmsMovie>],
    /// Server offset of the current window, used to request an earlier page.
    pub offset: usize,
    pub more: bool,
    /// Page reads landed on this row, one that changed nothing included: what tells the screen an
    /// ask was answered when `items` and `more` did not move (`plx_ui::cards::CardSource::page_epoch`).
    pub epoch: u32,
}

/// Provider identities are tagged: a listing key cannot collide with an identifier that
/// happens to contain the same bytes. An identifier is scoped by both server and its
/// provider-published listing key: PMS reuses `home.television.recent` for section-specific rows,
/// while a cross-section row keeps one key even when its leading item's library changes. Neither
/// display text, item content nor position participates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HubIdentity<'a> {
    ContinueWatching,
    Identifier { sid: ServerId, id: &'a str, key: &'a str },
    Key { sid: ServerId, key: &'a str },
}

fn stable_hub_identity<'a>(row: &'a HubRow, items: &[Arc<PmsMovie>]) -> Option<HubIdentity<'a>> {
    if row.len == 0 { return None; }
    let sid = items.get(row.start)?.sid;
    if row.hub_id == "home.continue" { Some(HubIdentity::ContinueWatching) }
    else if !row.hub_id.is_empty() {
        Some(HubIdentity::Identifier { sid, id: &row.hub_id, key: &row.key })
    }
    else if !row.key.is_empty() { Some(HubIdentity::Key { sid, key: &row.key }) }
    else { None }
}

#[derive(Clone, Copy)]
pub struct HeroRef<'a> {
    pub item: &'a PmsMovie,
    pub source: &'a str,
}

impl<'a> HubsView<'a> {
    pub fn hub_count(self) -> usize { self.data.hubs.len() }
    pub fn hero_count(self) -> usize { self.data.heroes.len() }
    pub fn hub(self, index: usize) -> Option<HubRef<'a>> {
        let row = self.data.hubs.get(index)?;
        let end = row.start.checked_add(row.len)?;
        Some(HubRef {
            identity: stable_hub_identity(row, &self.data.items),
            title: &row.title, source: &row.source, total: row.total,
            offset: row.offset, more: row.more, epoch: row.epoch,
            items: self.data.items.get(row.start..end)?,
        })
    }
    pub fn hero(self, index: usize) -> Option<HeroRef<'a>> {
        let slot = self.data.heroes.get(index)?;
        Some(HeroRef { item: &**self.data.items.get(slot.idx)?, source: &slot.source })
    }

    /// Server-scoped catalog lookup by `(sid, rk)`, over the retained publication rather than a
    /// global — see [`index_of_rk`]'s doc for why the scan must not compare `rk` alone.
    pub fn find(self, sid: ServerId, rk: &str) -> Option<&'a PmsMovie> {
        self.data.items.iter().find(|m| plx_plex::plex::same_item((m.sid, &m.rk), (sid, rk))).map(|m| &**m)
    }
}

/// **Own items first — an ORDERING, not a filter** (Shared Sources, deliverable C).
///
/// A borrowed item may not hold the FIRST rotation while the owner contributes at least one, so the
/// app's front door opens on your own library and a friend's film arrives one 8-second flip in,
/// attributed. Everything else about the pool is untouched: it stays merged, in the order the
/// shelves produced it, and the promoted page is lifted out and re-inserted rather than sorted, so
/// `[B1, B2, O1, O2, B3]` becomes `[O1, B1, B2, O2, B3]` — one page moves, nothing is dropped and
/// nothing else is reordered.
///
/// Filtering instead would leave a borrowed-only account with **no hero at all**, and would overrule
/// a pin the user made; that is why a pool with nothing of our own in it is left exactly as it is and
/// opens on a borrowed page. This is also why the rule needs no switch of its own: the pool is built
/// from included sources only, so a borrowed hero is always the consequence of a pin.
fn own_items_first(pool: &mut Vec<HeroSlot>) {
    if pool.first().map(|s| s.source.is_empty()).unwrap_or(true) {
        return; // nothing pooled, or one of ours already opens the door
    }
    if let Some(k) = pool.iter().position(|s| s.source.is_empty()) {
        let own = pool.remove(k);
        pool.insert(0, own);
    }
}

/// number of home hubs
pub fn hub_count(state: &PmsState) -> usize {
    hubs(state).len()
}
/// Item count in hub `i`, read straight off the published catalog. Test-only: production reads
/// the retained publication ([`HubsView::hub`]), and this is what other modules' store and
/// dispatcher tests assert a landing with.
#[cfg(any(test, feature = "test-support"))]
pub fn hub_len(state: &PmsState, i: usize) -> usize {
    hubs(state).get(i).map(|h| h.len).unwrap_or(0)
}

/// item `col` of hub `hub`, or None
#[cfg(any(test, feature = "test-support"))]
pub fn hub_item(state: &PmsState, hub: usize, col: usize) -> Option<&PmsMovie> {
    let h = hubs(state).get(hub)?;
    if col < h.len {
        movie(state, h.start + col)
    } else {
        None
    }
}

/// Refetch the home hubs OFF the main thread — every source on a worker, the owned one included,
/// landing through [`pump`] like any other fetch. **MAIN THREAD, NON-BLOCKING.**
///
/// No reconcile call here, and none is owed anywhere: [`commit`] performs the re-selection and the
/// repaint itself, at the only moment the catalog those surfaces index into actually moves.
fn request_refetch_hubs_with_scope(state: &mut PmsState, adapter: &Arc<PmsAdapter>, scope: &BrowseScope,
    launch: &mut dyn FnMut(HubRequest)) -> crate::stores::EndpointRefreshSet {
    // A test reaching this outside `plx_base::testlock::serial()` races some other module's test — see
    // `lib.rs::testlock`.
    #[cfg(any(test, feature = "test-support"))]
    plx_base::testlock::assert_held("the pms hub catalog (request_refetch_hubs)");
    state.hub_gen = state.hub_gen.wrapping_add(1); // supersede every retry already in flight
    sync_roster_with_scope(state, scope);
    let gen = state.hub_gen;
    let mut srcs = std::mem::take(&mut state.srcs);
    // A superseded worker's landing is dropped on the generation above, so releasing the
    // single-flight latches here cannot double-apply anything — and without it a source whose
    // worker was in flight across this call would stay latched and never fetch again. An
    // authoritative request supersedes any older flight, so release every latch here.
    let mut endpoints = crate::stores::EndpointRefreshSet::default();
    for s in srcs.iter_mut() {
        s.fetching = false;
        s.abandoned.clear();
        s.adoptable = false;
        s.page = None;
        s.staged = None; // a prompted read supersedes what a quiet one brought
        s.quiet = false;
        s.quiet_owed = false;
    }
    let mut turn = fanout_turn(&mut state.fanout, &srcs, |_| true);
    for s in srcs.iter_mut() {
        if let Some(request) = retry_now_gated(&mut turn, gen, adapter, s, scope, launch) { endpoints.insert(request); }
    }
    state.srcs = srcs;
    endpoints
}

/// Apply `edit` ([`ViewEdit`]) to every shelf row `at` covers and re-commit Home under the retained Browse
/// scope. Returns whether anything
/// matched. **MAIN THREAD** — it rebuilds the catalog the UI holds `&'static` rows out of.
///
/// It edits each source's own last PROJECTION and re-runs the pure [`merge`], rather than splicing
/// the committed catalog: `HubRow` addresses its cards as a `start`/`len` window into one flat
/// `Vec`, and the hero pool holds indices into the same, so removing a row by hand means fixing up
/// every window behind it and every pool slot — three chances to leave the three statics disagreeing,
/// which is the exact class `commit`'s doc says they move together to avoid. Re-merging is arithmetic
/// the module already trusts, and it also lets a shelf that lost a card refill from the budget.
///
/// This is one half of a pair and is useless alone: it is what the user SEES, and the refetch the
/// write's landing kicks is what the server SAYS. Where they disagree the refetch wins, silently.
#[cfg(test)]
fn edit_item(state: &mut PmsState, at: &ViewAt, edit: ViewEdit) -> bool {
    edit_item_with_scope(state, at, edit, &BrowseScope::standalone())
}

fn edit_item_with_scope(
    state: &mut PmsState,
    at: &ViewAt,
    edit: ViewEdit,
    scope: &BrowseScope,
) -> bool {
    // Same test-only catalog guard as `request_refetch_hubs` — the catalog is `PmsState`, a
    // field of the per-`Bridge` `HubsStore`, not a crate-global; see `lib.rs::testlock` and D5.
    #[cfg(any(test, feature = "test-support"))]
    plx_base::testlock::assert_held("the pms hub catalog (edit_item)");
    let mut hit = false;
    for s in state.srcs.iter_mut() {
        if let Some(b) = s.last.as_mut() {
            hit |= apply_edit(b, at, edit);
        }
        // what a quiet read brought is the server from before this edit; it must carry the edit
        // when it lands, and it is not what the person sees, so it never decides `hit`
        if let Some(b) = s.staged.as_mut() { apply_edit(b, at, edit); }
        // a removed card leaves the facts with it, wherever it sorted in its lane (`Held::feed`
        // drops only copies inside the span of the rows that remain)
        if matches!(edit, ViewEdit::LeftDeck) { s.copies.forget(|m| at.is_item(m.sid, &m.rk)); }
        s.feed_copies(); // a removed card, a marked one
    }
    if !hit {
        return false; // the item is on no shelf (a Library-grid or Related item): nothing to redraw
    }
    let build = merge_with_scope(&state.srcs, scope);
    adopt_browse_scope(state, scope);
    commit(state, build);
    true
}

/// [`edit_item_with_scope`] on ONE source's projection. Pure — no statics, no I/O — so the rule is
/// graded on the host rather than inferred from a screenshot.
fn apply_edit(b: &mut SourceBuild, at: &ViewAt, edit: ViewEdit) -> bool {
    // A deck removal is about the one row pressed; a watched change reaches what the scope names.
    let mine = |m: &PmsMovie| match edit {
        ViewEdit::Watched(_) | ViewEdit::Facts { .. } | ViewEdit::Offset { .. } => at.covers(m),
        ViewEdit::LeftDeck => at.is_item(m.sid, &m.rk),
    };
    match edit {
        ViewEdit::Watched(_) | ViewEdit::Facts { .. } | ViewEdit::Offset { .. } => {
            let mut hit = false;
            for c in b.cw.iter_mut().chain(b.lane.rows.iter_mut()) {
                if mine(&*c.m) && edit.changes(&*c.m) {
                    edit.apply(Arc::make_mut(&mut c.m));
                    hit = true;
                }
            }
            for m in b.shelves.iter_mut().flat_map(|s| s.items.iter_mut()) {
                // test the shared card first: `make_mut` clones only the card that matches
                if mine(&**m) && edit.changes(&**m) {
                    edit.apply(Arc::make_mut(m));
                    hit = true;
                }
            }
            hit
        }
        ViewEdit::LeftDeck => {
            let before = b.cw.len();
            b.cw.retain(|c| !mine(&*c.m));
            b.lane.remove(|row| mine(&*row.m)) || b.cw.len() != before
        }
    }
}

/// The fields one row's watch state is spread over, moved together: `watched`, `unwatched` and
/// `resume_ms`, and for a show or season `viewed_leaf_count` too.
///
/// `resume_ms` goes with them, and it is the half that is easy to miss: [`PmsMovie::resume_frac`]
/// takes PRECEDENCE over the watched flag at the mark (`ui::widgets::poster_mark` — a re-watch in
/// flight outranks a finished item), so a row left holding its old `viewOffset` would wear the
/// progress bar it had before and show no tick at all — the press would read as having done
/// nothing. An unscrobble genuinely clears `viewOffset` server-side, and a scrobbled item leaves
/// the deck; where the server disagrees, its own refetch is a moment behind this and wins.
///
/// `pub` because the hub catalog stopped being the only store an optimistic edit reaches:
/// `browse`, `search` and `person` each hold their own rows and each flips them the same way, and
/// three copies of "which fields" is three chances for one of them to leave the resume bar on.
pub fn set_watched(m: &mut PmsMovie, on: bool) {
    m.watched = on;
    m.unwatched = !on;
    m.resume_ms = 0;
    // A container's episode counts follow its mark, or a show sorted by Unplayed would keep wearing
    // "9 episodes" under a watched tick until the listing is read again.
    if matches!(m.kind, KIND_SHOW | 2) {
        if let Some(leaves) = m.leaf_count { m.viewed_leaf_count = Some(if on { leaves } else { 0 }); }
    }
}

/// The catalog/hubs/pool triple the merge produces, before it is committed.
type HubBuild = (Vec<Arc<PmsMovie>>, Vec<HubRow>, Vec<HeroSlot>);

// ---- one source's contribution -----------------------------------------------------------------

/// A Continue Watching entry, carrying the sort key the MERGE needs. `lastViewedAt` used to be read
/// off the wire DTO and thrown away at parse time, because one server's hub arrived already in the
/// right order; across sources the order has to be re-established after the fact, so the key has to
/// survive the projection.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CwItem {
    pub(crate) last_viewed_at: i64,
    pub(crate) m: Arc<PmsMovie>,
    /// Where the listing behind `/hubs/continueWatching/items` holds this card; 0 until the deck
    /// is placed, when a preview card takes its place in the preview.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(crate) position: usize,
}

fn is_zero(n: &usize) -> bool { *n == 0 }

/// One shelf as a source projected it: rows already parsed, filtered and stamped with the server
/// they came from, so the merge is pure arithmetic over owned data and never touches a wire DTO.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Shelf {
    title: String,
    hub_id: String,
    key: String,
    items: Vec<Arc<PmsMovie>>,
    #[serde(default)]
    positions: Vec<usize>,
    /// Every item the hub's listing holds (`plex::Hub::total`) — a collection shelf's "· N".
    #[serde(default)]
    total: usize,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    end: usize,
    #[serde(default)]
    more: bool,
    /// Cards the row showed when it gave them up to the ring; 0 while it holds cards. A row with
    /// `shown > 0` and no items is a descriptor: Home publishes that many placeholder slots for it.
    #[serde(default, skip_serializing_if = "is_zero")]
    shown: usize,
    /// How the window relates to its listing, and the keys the row has shown. Absent for a row that
    /// has not paged, so an older recording decodes unchanged.
    #[serde(default, skip_serializing_if = "paging::RowState::is_default")]
    row: paging::RowState,
    /// How many page reads have landed on this row, a rescan that changed nothing included
    /// ([`HubRef::epoch`]). Bookkeeping of this run: not recorded, not hashed.
    #[serde(skip)]
    epoch: u32,
}

impl Shelf {
    fn is(&self, id: &str, key: &str) -> bool { self.hub_id == id && self.key == key }
    /// A descriptor: the row gave its cards up to the ring and keeps what is needed to ask for them.
    fn released(&self) -> bool { self.items.is_empty() && self.shown > 0 }
}

/// ONE source's whole contribution to Home — its Continue Watching items (merged with everyone
/// else's into a single shelf) and its own shelves (kept whole, annotated with its owner's handle).
///
/// Owned data only, so a worker can build it and hand it over through the mailbox, and a source can
/// KEEP the last one it answered with across a failure.
#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceBuild {
    cw: Vec<CwItem>,
    shelves: Vec<Shelf>,
    /// This server's part of the Continue Watching deck beyond its preview (`stores::deck`). Absent
    /// until the row has a listing to read, so an older recording decodes unchanged.
    #[serde(default, skip_serializing_if = "deck::Lane::is_default")]
    lane: deck::Lane,
}

/// A page read the user walked away from whose worker has not landed yet.
struct Abandoned {
    seq: u32,
    page: PageQuery,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PageQuery {
    id: String,
    key: String,
    start: usize,
    #[serde(default)]
    before: bool,
    #[serde(default)]
    hidden: Vec<i64>,
    /// Not a move of the window: the row came back into the ring and is read again where it stood.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    reload: bool,
}

/// Whether a hub's listing is one the pager reads: every row whose key [`is_pageable_hub_key`]
/// admits pages, whatever it is called. A row whose key is not admitted keeps its preview.
fn pages(key: &str) -> bool {
    plx_plex::plex::is_pageable_hub_key(key)
}

fn find_shelf<'a>(shelves: &'a [Shelf], id: &str, key: &str) -> Option<&'a Shelf> {
    shelves.iter().find(|shelf| shelf.is(id, key))
}

fn request_page(state: &mut PmsState, adapter: &PmsAdapter, sid: ServerId,
    id: &str, key: &str, before: bool, scope: &BrowseScope, launch: &mut dyn FnMut(HubRequest)) -> Option<crate::stores::EndpointRefresh> {
    let Some(source) = state.srcs.iter_mut().find(|source| source.sid == sid) else { return None };
    if refresh_src_lifecycle(source, &scope.clients) { return None; }
    if source.adoptable && !source.fetching && source.page.is_none() && source.abandoned.last()
        .is_some_and(|a| a.page.id == id && a.page.key == key && a.page.before == before) {
        // The read the user walked away from is still out and is the one being asked for again.
        // Nothing has been asked of this source since, so the window it was read for is the one
        // that stands and the read answers this ask too.
        if let Some(read) = source.abandoned.pop() {
            source.seq = read.seq;
            source.page = Some(read.page);
            source.fetching = true;
            source.adoptable = false;
        }
        return None;
    }
    if source.abandoned.iter().any(|a| a.page.id == id && a.page.key == key) { return None; }
    if source.fetching || source.page.is_some() || source.state != HubState::Ready { return None; }
    let Some(shelf) = source.…30956 tokens truncated…ight; a pending page request is re-sent by the same kick.
/// Behind the fan-out gate: the sources it has no room for are kicked by the following ticks.
fn retry_now_gated(turn: &mut crate::stores::fanout::FanoutTurn, gen: u32, adapter: &PmsAdapter, s: &mut Src,
    scope: &BrowseScope, launch: &mut dyn FnMut(HubRequest)) -> Option<crate::stores::EndpointRefresh> {
    s.retry_n = 0;
    s.retry_s = 0.0;
    kick_gated(turn, gen, adapter, s, scope, launch)
}

/// Move the worker mailbox into one owned batch. No source or catalog state changes here.
///
/// Releases the claim ([`owed`]) for every landing it moves out, in the same call: dump mode's
/// `take_all_owed` reads the claim after each take. Saturating, because a test can queue a
/// landing straight into the mailbox without a spawn.
pub fn take_landings(adapter: &PmsAdapter) -> Vec<Landing> {
    let taken = std::mem::take(&mut *adapter.results.lock().unwrap_or_else(|e| e.into_inner()));
    let n = u32::try_from(taken.len()).unwrap_or(u32::MAX);
    if n != 0 {
        let _ = adapter.owed.try_update(Ordering::SeqCst, Ordering::SeqCst, |owed| Some(owed.saturating_sub(n)));
    }
    #[cfg(any(test, feature = "test-support"))]
    adapter.takes.fetch_add(1, Ordering::SeqCst);
    taken
}

/// Apply one explicitly supplied batch through the live landing rules — land finished fetches,
/// then count each source down to its next automatic attempt. The supplier runs after roster
/// reconciliation, exactly where the live mailbox used to be drained. Keeping it separate lets an
/// adapter observe or substitute arrivals without a second state-application path. This is NOT an
/// offline replay mode: retry scheduling and job emission remain live.
#[cfg(any(test, feature = "test-support"))]
fn pump_with_landings(state: &mut PmsState, adapter: &Arc<PmsAdapter>, dt: f32,
    take: impl FnOnce() -> Vec<Landing>) -> crate::stores::EndpointRefreshSet {
    step_landings(state, adapter, Some(dt), take)
}

/// Test-only compatibility tick for fixtures without a retained directory.
#[cfg(any(test, feature = "test-support"))]
pub fn tick(state: &mut PmsState, adapter: &Arc<PmsAdapter>, dt: f32) -> crate::stores::EndpointRefreshSet {
    // Same test-only catalog guard as `request_refetch_hubs` — the catalog is `PmsState`, a
    // field of the per-`Bridge` `HubsStore`, not a crate-global; see `lib.rs::testlock` and D5.
    #[cfg(any(test, feature = "test-support"))]
    plx_base::testlock::assert_held("the pms hub catalog (tick)");
    pump_with_landings(state, adapter, dt, Vec::new)
}

/// The owned store's tick never consumes the worker mailbox. Arrivals are delivered separately
/// by the dispatcher; a worker finishing during its drain belongs to the next frame's ingest.
pub fn tick_with_directory(
    state: &mut PmsState,
    adapter: &Arc<PmsAdapter>,
    dt: f32,
    directory: crate::stores::browse::DirectoryView<'_>,
    clients: &ClientKeys,
) -> crate::stores::EndpointRefreshSet {
    #[cfg(any(test, feature = "test-support"))]
    plx_base::testlock::assert_held("the pms hub catalog (owned tick)");
    emitting(state, adapter, |state, launch| step_landings_with_scope(state, adapter, Some(dt), Vec::new,
        &BrowseScope::retained(directory, clients), launch))
}

/// An addressed arrival may update the catalog behind another page, but must not advance retry
/// timers or start a new hubs fetch there. The visible Home alone owes the store's tick.
#[cfg(any(test, feature = "test-support"))]
fn apply_landing(state: &mut PmsState, adapter: &Arc<PmsAdapter>, landing: &Landing) -> crate::stores::EndpointRefreshSet {
    #[cfg(any(test, feature = "test-support"))]
    plx_base::testlock::assert_held("the pms hub catalog (apply_landing)");
    step_landings(state, adapter, None, || vec![landing.clone()])
}

/// Test-only compatibility landing for fixtures without a retained directory.
#[cfg(any(test, feature = "test-support"))]
pub fn land(state: &mut PmsState, adapter: &Arc<PmsAdapter>, landing: &Landing) -> crate::stores::StoreOutcome {
    let before = state.catalog_gen;
    let endpoints = apply_landing(state, adapter, landing);
    crate::stores::StoreOutcome { changed: state.catalog_gen != before, endpoints }
}

/// Apply one addressed landing under the same retained directory policy the frame publishes.
pub fn land_with_directory(
    state: &mut PmsState,
    adapter: &Arc<PmsAdapter>,
    landing: &Landing,
    directory: crate::stores::browse::DirectoryView<'_>,
    clients: &ClientKeys,
) -> crate::stores::StoreOutcome {
    let before = state.catalog_gen;
    let endpoints = emitting(state, adapter, |state, launch| step_landings_with_scope(state, adapter, None,
        || vec![landing.clone()], &BrowseScope::retained(directory, clients), launch));
    crate::stores::StoreOutcome { changed: state.catalog_gen != before, endpoints }
}

/// `stores::hubs::HubsStore`'s one door onto every [`HubsCmd`](crate::stores::hubs::HubsCmd) (D3):
/// the match used to live in `stores/hubs.rs::run`, calling four `pub` mutators across the
/// module boundary. Relocating the match here is what lets those four go private.
#[cfg(any(test, feature = "test-support"))]
pub fn run(state: &mut PmsState, adapter: &Arc<PmsAdapter>, cmd: crate::stores::hubs::HubsCmd) -> crate::stores::StoreOutcome {
    run_with_scope(state, adapter, cmd, &BrowseScope::standalone())
}

pub fn run_with_directory(
    state: &mut PmsState,
    adapter: &Arc<PmsAdapter>,
    cmd: crate::stores::hubs::HubsCmd,
    directory: crate::stores::browse::DirectoryView<'_>,
    clients: &ClientKeys,
) -> crate::stores::StoreOutcome {
    if matches!(cmd, crate::stores::hubs::HubsCmd::Publish { .. }) && !has_staged(state) {
        return crate::stores::StoreOutcome::default(); // the per-frame question, before anything is built
    }
    run_with_scope(state, adapter, cmd, &BrowseScope::retained(directory, clients))
}

/// One addressed command. Launches refused since the last step are settled first
/// ([`settle_refused`]), so the command reads the state they left, and their recovery joins the
/// command's own. The fetches the command asks for are left on the owner's outbox.
fn run_with_scope(
    state: &mut PmsState,
    adapter: &Arc<PmsAdapter>,
    cmd: crate::stores::hubs::HubsCmd,
    scope: &BrowseScope,
) -> crate::stores::StoreOutcome {
    let mut endpoints = settle_refused(state, adapter);
    let out = emitting(state, adapter, |state, launch| run_settled(state, adapter, cmd, scope, launch));
    endpoints.merge(out.endpoints);
    crate::stores::StoreOutcome { changed: out.changed, endpoints }
}

fn run_settled(
    state: &mut PmsState,
    adapter: &Arc<PmsAdapter>,
    cmd: crate::stores::hubs::HubsCmd,
    scope: &BrowseScope,
    launch: &mut dyn FnMut(HubRequest),
) -> crate::stores::StoreOutcome {
    use crate::stores::hubs::HubsCmd;
    match cmd {
        paging @ (HubsCmd::Page { .. } | HubsCmd::CancelPage { .. }) => {
            let before = state.catalog_gen;
            let endpoints = run_page_cmd(state, adapter, scope, paging, launch);
            crate::stores::StoreOutcome { changed: before != state.catalog_gen, endpoints }
        }
        HubsCmd::RefetchHubs => crate::stores::StoreOutcome {
            changed: true,
            endpoints: request_refetch_hubs_with_scope(state, adapter, scope, launch),
        },
        HubsCmd::Hold { lo, hi } => {
            let before = state.catalog_gen;
            let endpoints = run_hold(state, adapter, scope, lo, hi, launch);
            crate::stores::StoreOutcome { changed: before != state.catalog_gen, endpoints }
        }
        HubsCmd::Retry => {
            let gen = state.hub_gen;
            let mut srcs = std::mem::take(&mut state.srcs);
            let mut endpoints = crate::stores::EndpointRefreshSet::default();
            let mut turn = fanout_turn(&mut state.fanout, &srcs, |_| true);
            for source in srcs.iter_mut() {
                if let Some(request) = retry_now_gated(&mut turn, gen, adapter, source, scope, &mut *launch) { endpoints.insert(request); }
            }
            state.srcs = srcs;
            crate::stores::StoreOutcome { changed: true, endpoints }
        },
        HubsCmd::ApplyView { at, edit } => crate::stores::StoreOutcome::changed(
            edit_item_with_scope(state, &at, edit, scope)),
        HubsCmd::Reset => {
            reset_with_scope(state, adapter, scope);
            crate::stores::StoreOutcome::changed(true)
        }
        HubsCmd::RefetchSourceQuiet { sid } => crate::stores::StoreOutcome {
            changed: false,
            endpoints: refetch_source_quiet(state, adapter, scope, sid, launch),
        },
        HubsCmd::Publish { home_top_visible, focused_hub, press_live } => {
            let before = state.catalog_gen;
            publish_staged(state, scope, home_top_visible, focused_hub, press_live);
            crate::stores::StoreOutcome::changed(state.catalog_gen != before)
        }
    }
}

/// TEST ONLY: one step, with each fetch it asks for handed to `launch` as the runner would
/// start it. `launch` says whether anything was started; `false` is the refusal the runner
/// answers (`Port::refused`), so the source stays in flight until the next step settles it. A
/// data-layer test reads the requests a step made, and answers them, without a thread or a
/// runner. `cmd: None` is the store's tick.
#[cfg(any(test, feature = "test-support"))]
pub fn controlled_work(state: &mut PmsState, adapter: &Arc<PmsAdapter>, cmd: Option<crate::stores::hubs::HubsCmd>, dt: f32,
    launch: &mut dyn FnMut(HubRequest) -> bool) -> crate::stores::StoreOutcome {
    let out = match cmd {
        Some(cmd) => run_with_scope(state, adapter, cmd, &BrowseScope::standalone()),
        None => {
            let before = state.catalog_gen;
            let endpoints = pump_with_landings(state, adapter, dt, Vec::new);
            crate::stores::StoreOutcome { changed: state.catalog_gen != before, endpoints }
        }
    };
    start_for_test(state, launch);
    out
}

/// TEST ONLY: hand every job on the outbox to `launch` (see [`controlled_work`]).
#[cfg(any(test, feature = "test-support"))]
fn start_for_test(state: &mut PmsState, launch: &mut dyn FnMut(HubRequest) -> bool) {
    for job in std::mem::take(&mut state.jobs) {
        let crate::stores::StoreJob::Hubs(job, port) = job else { unreachable!("another store's job on the Hubs outbox") };
        let addr = job.addr;
        if !launch(job.req) { port.refused(addr); }
    }
}

#[cfg(any(test, feature = "test-support"))]
fn step_landings(state: &mut PmsState, adapter: &Arc<PmsAdapter>, dt: Option<f32>, take: impl FnOnce() -> Vec<Landing>) -> crate::stores::EndpointRefreshSet {
    emitting(state, adapter, |state, launch| step_landings_with_scope(state, adapter, dt, take,
        &BrowseScope::standalone(), launch))
}

/// TEST ONLY: [`step_landings_with_scope`] on the standalone scope, the requests the step makes
/// handed to `launch` as they are asked.
#[cfg(test)]
fn step_landings_with(state: &mut PmsState, adapter: &PmsAdapter, dt: Option<f32>, take: impl FnOnce() -> Vec<Landing>,
    launch: &mut dyn FnMut(HubRequest)) -> crate::stores::EndpointRefreshSet {
    step_landings_with_scope(state, adapter, dt, take, &BrowseScope::standalone(), launch)
}

fn step_landings_with_scope(state: &mut PmsState, adapter: &PmsAdapter, dt: Option<f32>, take: impl FnOnce() -> Vec<Landing>,
    scope: &BrowseScope,
    launch: &mut dyn FnMut(HubRequest)) -> crate::stores::EndpointRefreshSet {
    let mut endpoints = settle_refused(state, adapter);
    sync_roster_with_scope(state, scope);
    let landed = take();
    let any_landed = !landed.is_empty();
    let cur = state.hub_gen;
    let mut srcs = std::mem::take(&mut state.srcs);
    let mut dirty = false;
    let mut paged = false;
    let mut refreshed = false;
    for l in landed {
        // A landing from before the last authoritative fetch describes a server (or an account) we
        // have since moved off, and one whose seq has been superseded describes an attempt this
        // source has already replaced. Either is dropped whole — neither committed nor blamed.
        let Some(s) = srcs.iter_mut().find(|s| s.sid == l.sid) else {
            continue; // its source left the roster while it was out
        };
        // The client OBJECT the request was made through is `resource.instance_gen()`, never the
        // landing's `instance`: a replayed landing carries the recording's logical id there, and
        // only the resource it was bound to is the registry's.
        let lifecycle_matches = l.client.is_none_or(|client| {
            scope.clients.key(l.sid).is_some_and(|k| {
                k.instance == client.resource.instance_gen() && k.token == l.token_gen
            })
        });
        if l.gen != cur || l.seq != s.seq || !lifecycle_matches {
            if l.gen == cur && l.seq == s.seq && !lifecycle_matches {
                s.staged = None;
                s.quiet = false;
                s.quiet_owed = false;
                s.fetching = false;
                s.abandoned.clear();
                s.adoptable = false;
                s.page = None;
                s.state = HubState::Loading;
                s.retry_s = 0.0;
            } else if l.gen == cur {
                // a read the user withdrew has finished: its row is free to be asked again
                s.abandoned.retain(|a| a.seq != l.seq);
            }
            continue;
        }
        s.fetching = false;
        if std::mem::take(&mut s.quiet) {
            // a quiet read stages its answer and publishes nothing; a failure is not a failed source
            match l.build {
                Some(mut b) => {
                    replay_since_request(s, &mut b);
                    if let Some(old) = s.last.as_ref() { carry_descriptors(old, &mut b); }
                    s.staged = Some(b);
                    s.quiet_fails = 0;
                    state.staged_gen = state.staged_gen.wrapping_add(1);
                }
                // no rediscovery: it is for the person's own reads; this one backs off and the
                // next ask (or the prompted retry) finds out
                None => quiet_failed(s),
            }
            continue;
        }
        match l.build {
            Some(mut b) => {
                replay_since_request(s, &mut b);
                if let Some(page) = s.page.take() {
                    drop_staging(s); // the window moved under it
                    dirty |= if page.id == DECK_ID { land_lane(s, b) } else if page.reload { land_reload(s, &page, b) } else { land_page(s, &page, b) };
                    paged = true;
                    s.retry_n = 0;
                    s.retry_s = 0.0;
                } else {
                    landed_ok(s, b);
                    dirty = true;
                    refreshed = true;
                }
            }
            None => { endpoints.insert(landed_fail(s)); }
        }
    }
    if state.deck_ask.as_ref().is_some_and(|ask| deck_ready(ask, &srcs)) {
        if let Some(ask) = state.deck_ask.take() {
            // a window with no card to show asks on, by the same path, a page further
            if move_deck(&mut srcs, scope, ask.before, ask.pages).is_none() {
                state.srcs = srcs;
                endpoints.merge(ask_deck(state, adapter, ask.before, ask.pages + 1, scope, &mut *launch));
                srcs = std::mem::take(&mut state.srcs);
            }
        }
        dirty = true;
        paged = true;
    }
    // Anything but Ready with nothing in flight is a state only a fetch can leave: Failed (with a
    // backoff owed) or a Loading whose worker landed stale and was dropped — the latter owes
    // nothing, so it re-kicks on the spot rather than wedging that source on a spinner forever.
    if let Some(dt) = dt {
        for s in srcs.iter_mut() { s.quiet_after = (s.quiet_after - dt).max(0.0); }
        let mut turn = fanout_turn(&mut state.fanout, &srcs, |s| source_waiting(s) && s.retry_s - dt <= 0.0);
        for s in srcs.iter_mut() {
            if source_waiting(s) && retry_due(s, dt) {
                if let Some(request) = kick_gated(&mut turn, cur, adapter, s, scope, &mut *launch) { endpoints.insert(request); }
            }
        }
    }
    // …and re-merge when the SECTION TABLE moves, not only when a build lands. `feeds_home` reads
    // the pinned set, which is derived from that table — so both of the ways the set changes were
    // invisible to Home before this:
    //
    //   * a share's sections are discovered on a WORKER (`browse::maybe_discover`), strictly after
    //     the boot fetch. Until they land the share is in the roster but has no pinned library, so
    //     the merge that ran on its hub landing excluded it — and nothing re-ran.
    //   * a pin toggled by hand (now bumping the same generation).
    //
    // `merge` is pure over the builds each source already answered with: no request, no allocation
    // beyond the rebuilt catalog. Cheap enough to run on a generation change rather than to try to
    // predict which changes matter.
    let scope_moved = browse_scope_moved(state, scope);
    if dirty || scope_moved { settle_deck(&mut srcs, refreshed); }
    let build = (dirty || scope_moved).then(|| {
        let t0 = std::time::Instant::now();
        let (mut build, _) = merge_held(&mut srcs, scope, state.hold);
        if paged && !refreshed && !scope_moved { preserve_heroes(state, &mut build); }
        let took = t0.elapsed();
        (build, took)
    });
    state.srcs = srcs;
    if any_landed {
        // A landing that COMMITS repaints from inside `commit`; this is the one that does not —
        // a failure rewrites no shelf but does change the status caption, under a Home screen that
        // may have gone idle with nothing else on it to move.
        plx_machine::idle::invalidate();
    }
    if let Some((build, took)) = build {
        let bytes = catalog_bytes(&build.0);
        let n = commit(state, build);
        // The harness (`tests/mock_fps.py`) reads the first two numbers; new fields go AFTER them.
        plx_base::eventlog::log(&format!(
            "hubs: landed — {n} items, {} shelves (merge {} us, catalog ~{} KB)",
            hub_count(state),
            took.as_micros(),
            bytes / 1024
        ));
    }
    if dt.is_some() { endpoints.merge(pump_owed_quiet(state, adapter, scope, &mut *launch)); }
    endpoints.merge(pump_reloads(state, adapter, scope, launch));
    endpoints
}

/// A source that has answered with `n` placeholder rows in one shelf (test fixture). Only the SHAPE
/// is real — `project` would have dropped these rows for having no title/poster; what the tests
/// using it assert is the landing/merge bookkeeping, which never looks inside a row.
///
/// Two fields ARE filled, and both because the hero pool reads them: `art`, since `merge` skips a
/// row with no landscape artwork (it would make a blank billboard), and a distinct `rk` per row,
/// since the pool dedups by item IDENTITY and n rows sharing the empty key are ONE film to it. A
/// fixture of bare defaults therefore committed shelves with an EMPTY pool — a Home that has
/// content but cannot page — and the Home pager test needs somewhere to page to. A fixture
/// that cannot express the app's ordinary state quietly limits what can be tested through it.
#[cfg(any(test, feature = "test-support"))]
fn build_test(n: usize) -> SourceBuild {
    SourceBuild {
        cw: Vec::new(),
        lane: Default::default(),
        shelves: vec![Shelf { epoch: 0,
            title: "Continue Watching".into(),
            hub_id: "home.continue".into(),
            key: String::new(),
            items: (0..n)
                .map(|i| Arc::new(PmsMovie {
                    rk: (i + 1).to_string(),
                    // One backdrop per item, as a real catalog has: a test that asks WHICH
                    // backdrop was requested (Home's neighbour preload) needs them told apart.
                    art: format!("/art/{}", i + 1),
                    ..PmsMovie::default()
                }))
                .collect(),
            total: 0, offset: 0, end: 0, more: false, shown: 0, positions: (0..n).collect(), row: Default::default(),
        }],
    }
}

/// Test hook: put the store in a known place — one source in `state`, having answered with `items`
/// rows in one shelf. Home's read-out is a pure projection of that pair, and the states a host test
/// cannot reach for real (a live server answering, or refusing) are exactly the ones worth pinning.
#[cfg(any(test, feature = "test-support"))]
pub fn seed_for_test(state: &mut PmsState, adapter: &Arc<PmsAdapter>, items: usize, hub_state: HubState) {
    plx_base::testlock::assert_held("the pms hub catalog (seed_for_test)");
    seed_with_scope_for_test(state, adapter, ServerId::UNSET, items, hub_state, &BrowseScope::standalone());
}

/// Seed a Hubs source that belongs to a real retained Browse directory. Full Bridge fixtures use
/// this instead of installing an `UNSET` source that owner-scoped roster reconciliation must drop.
#[cfg(any(test, feature = "test-support"))]
pub fn seed_for_directory_test(
    state: &mut PmsState,
    adapter: &Arc<PmsAdapter>,
    sid: ServerId,
    items: usize,
    hub_state: HubState,
    directory: crate::stores::browse::DirectoryView<'_>,
) {
    plx_base::testlock::assert_held("the pms hub catalog (seed_for_directory_test)");
    assert!(directory.sections().iter().any(|section| section.sid == Some(sid)),
        "a directory-scoped Hubs fixture requires its server in the retained Browse directory");
    seed_with_scope_for_test(state, adapter, sid, items, hub_state, &BrowseScope::retained(directory, &registry_keys_for_test()));
}

/// Two-library Home fixture for the application-boundary watch-state regression. Both rows remain
/// in the source projection; the retained directory alone decides which one is published.
#[cfg(any(test, feature = "test-support"))]
pub fn seed_two_library_home_for_test(
    state: &mut PmsState,
    sid: ServerId,
    directory: crate::stores::browse::DirectoryView<'_>,
) {
    plx_base::testlock::assert_held("the two-library pms home fixture");
    let sections = directory.sections();
    assert!(
        sections.len() >= 2 && sections[..2].iter().all(|section| section.sid == Some(sid)),
        "the two-library Home fixture requires two sections on its server"
    );
    let item = |section: &crate::stores::browse::SectionView, rk: &str| Arc::new(PmsMovie {
        sid,
        sec: section.key,
        rk: rk.into(),
        title: rk.into(),
        thumb: "/t.jpg".into(),
        art: "/a.jpg".into(),
        ..Default::default()
    });
    let keys = registry_keys_for_test();
    let scope = BrowseScope::retained(directory, &keys);
    let mut source = Src::new(sid, String::new(), &scope.clients);
    source.state = HubState::Ready;
    source.last = Some(SourceBuild {
        cw: Vec::new(),
        lane: Default::default(),
        shelves: vec![Shelf { epoch: 0,
            title: "Recent".into(),
            hub_id: "home.movies.recent".into(),
            key: String::new(),
            items: vec![item(&sections[0], "alpha"), item(&sections[1], "beta")],
            total: 0, offset: 0, end: 0, more: false, shown: 0, positions: Vec::new(), row: Default::default(),
        }],
    });
    let sources = vec![source];
    let build = merge_with_scope(&sources, &scope);
    state.srcs = sources;
    remember_roster(state, &scope);
    state.seen_facts = facts_key();
    adopt_browse_scope(state, &scope);
    commit(state, build);
}

#[cfg(any(test, feature = "test-support"))]
fn seed_with_scope_for_test(state: &mut PmsState, adapter: &Arc<PmsAdapter>, sid: ServerId, items: usize, hub_state: HubState, scope: &BrowseScope) {
    reset_with_scope(state, adapter, scope);
    let handle = plx_plex::plex::server_facts(sid)
        .map(|facts| facts.handle.clone())
        .unwrap_or_default();
    let mut s = Src::new(sid, handle, &scope.clients);
    s.ledger = state.ledger.clone();
    s.state = hub_state;
    if items > 0 {
        let mut build = build_test(items);
        for shelf in &mut build.shelves {
            for item in &mut shelf.items {
                Arc::make_mut(item).sid = sid;
            }
        }
        s.last = Some(build);
    }
    let srcs = vec![s];
    let build = merge_with_scope(&srcs, scope);
    state.srcs = srcs;
    // Leave `sync_roster` believing this exact scope is up to date. For a standalone fixture that
    // preserves the synthetic source against an empty registry; for an owner-bound fixture it
    // preserves the source whose sid and retained directory were supplied together. BOTH halves
    // of the fingerprint matter, or the facts epoch alone reads as a change and rebuilds anyway.
    remember_roster(state, scope);
    state.seen_facts = facts_key();
    commit(state, build);
}

/// Drop everything and re-arm the fetch — the identity-change twin of `BrowseCmd::Reset`, called
/// from the same identity boundary. Now that a failed fetch KEEPS the previous build,
/// a profile switch whose fetch fails would otherwise leave the previous user's shelves on screen;
/// this is the one place that must still wipe them.
///
/// Private since D3's follow-up: `app/bridge.rs` and `app/recorder.rs` (nine `#[cfg(test)] mod
/// tests` call sites between them) were the last two direct callers, both now routed through
/// `HubsStore::run`/`run_with_directory` (`HubsCmd::Reset`) — `HubsCmd` already had the variant
/// and `pms::run` already matched it, so closing this was a caller-site swap alone, no new enum
/// surface.
#[cfg(test)]
fn reset(state: &mut PmsState, adapter: &Arc<PmsAdapter>) {
    reset_with_scope(state, adapter, &BrowseScope::standalone());
}

fn reset_with_scope(state: &mut PmsState, adapter: &Arc<PmsAdapter>, scope: &BrowseScope) {
    // Same test-only catalog guard as `request_refetch_hubs` — the catalog is `PmsState`, a
    // field of the per-`Bridge` `HubsStore`, not a crate-global; see `lib.rs::testlock` and D5.
    #[cfg(any(test, feature = "test-support"))]
    plx_base::testlock::assert_held("the pms hub catalog (reset)");
    let _ = adapter; // every HubsStore command path rotates before applying `HubsCmd::Reset`
    state.hub_gen = state.hub_gen.wrapping_add(1); // a worker still running belongs to the old identity
    state.srcs = Vec::new();
    // The sources those fetches were asked for are gone: a fetch not yet started would answer into
    // a store that no longer waits for it.
    state.jobs.clear();
    state.deck_ask = None;
    forget_roster(state);
    state.seen_facts = u32::MAX;
    // Adopt the retained pin semantics with the empty commit below: a change from BEFORE this reset
    // is already reflected in "nothing", so it is not owed a re-merge. Left unadopted, the next
    // `pump` "caught up" on a scope some other era had moved and re-committed — freeing the HUBS
    // strings out from under a `hub_title` borrow held across that pump, which is how the test suite
    // read freed memory whenever another module's `browse::reset` ran in between.
    adopt_browse_scope(state, scope);
    commit(state, (Vec::new(), Vec::new(), Vec::new()));
}
// ---------------------------------------------------------------------------------------
#[cfg(any(test, feature = "test-support"))]
pub fn queue_test_landing(state: &PmsState, adapter: &PmsAdapter, items: Option<usize>) -> u32 {
    plx_base::testlock::assert_held("the pms hub catalog (queue_test_landing)");
    let source = &state.srcs[0];
    let seq = source.seq;
    let landing = Landing {
        gen: state.hub_gen, seq, sid: source.sid,
        client: None, token_gen: 0, build: items.map(build_test),
    };
    adapter.results.lock().unwrap_or_else(|e| e.into_inner()).push(landing);
    seq
}

#[cfg(any(test, feature = "test-support"))]
pub fn reverse_test_shelves(state: &mut PmsState) {
    plx_base::testlock::assert_held("the pms hub catalog (reverse_test_shelves)");
    for source in state.srcs.iter_mut() {
        if let Some(build) = source.last.as_mut() {
            for shelf in &mut build.shelves { shelf.items.reverse(); }
        }
    }
    let build = merge(&state.srcs);
    commit(state, build);
}

/// Test hook: every shelf's last card moves to its head, so each other card sits one place later
/// (an item landing above the focus, or a reorder that moves a card by one place).
#[cfg(any(test, feature = "test-support"))]
pub fn rotate_test_shelves_right(state: &mut PmsState) {
    plx_base::testlock::assert_held("the pms hub catalog (rotate_test_shelves_right)");
    for source in state.srcs.iter_mut() {
        if let Some(build) = source.last.as_mut() {
            for shelf in &mut build.shelves { shelf.items.rotate_right(1); }
        }
    }
    let build = merge(&state.srcs);
    commit(state, build);
}

#[cfg(any(test, feature = "test-support"))]
pub fn seed_grid_for_test(state: &mut PmsState, adapter: &Arc<PmsAdapter>, rows: usize, items: usize) {
    plx_base::testlock::assert_held("the pms hub catalog (seed_grid_for_test)");
    seed_for_test(state, adapter, items, HubState::Ready);
    let source = state.srcs[0].last.as_mut().unwrap();
    source.shelves = (0..rows).map(|row| {
        let mut shelf = build_test(items).shelves.remove(0);
        shelf.hub_id = format!("test.row.{row}");
        shelf
    }).collect();
    let build = merge(&state.srcs);
    commit(state, build);
}

/// [`seed_grid_for_test`] as the ring publishes it: `rows` pageable rows of `items` cards, the rows
/// outside `hold` given up to descriptors (placeholders in the catalog). `salt(row)` is part of each
/// card's rating key, so a later call with another salt is a refresh that brought new cards and a
/// call with the same salt is a row coming back with the cards it showed.
#[cfg(any(test, feature = "test-support"))]
pub fn seed_ring_for_test(state: &mut PmsState, adapter: &Arc<PmsAdapter>, rows: usize, items: usize,
    salt: &dyn Fn(usize) -> usize, hold: (usize, usize)) {
    plx_base::testlock::assert_held("the pms hub catalog (seed_ring_for_test)");
    seed_for_test(state, adapter, items, HubState::Ready);
    state.hold = hold;
    let source = state.srcs[0].last.as_mut().unwrap();
    source.shelves = (0..rows).map(|row| {
        let mut shelf = build_test(items).shelves.remove(0);
        shelf.hub_id = format!("test.row.{row}");
        shelf.key = format!("/hubs/sections/1/{row}");
        for (c, item) in shelf.items.iter_mut().enumerate() {
            Arc::make_mut(item).rk = format!("{}-{row}-{c}", salt(row));
        }
        shelf.end = items;
        shelf.more = true;
        shelf
    }).collect();
    let scope = BrowseScope::standalone();
    let mut srcs = std::mem::take(&mut state.srcs);
    let (build, _) = merge_held(&mut srcs, &scope, hold);
    state.srcs = srcs;
    commit(state, build);
}

/// Test hook: the store's half of `HubsCmd::Hold` (the band moves, the rows that left it give their
/// cards up, the catalog is published), without the reads for the rows that entered it; land those
/// with [`land_kept_rows_for_test`]. Returns whether the band moved.
#[cfg(any(test, feature = "test-support"))]
pub fn hold_for_test(state: &mut PmsState, lo: usize, hi: usize) -> bool {
    plx_base::testlock::assert_held("the pms hub catalog (hold_for_test)");
    apply_hold(state, &BrowseScope::standalone(), lo, hi)
}

/// Test hook: the answer a server would give every held descriptor of a [`seed_ring_for_test`]
/// fixture (its rows page, so they come back from their listing, not through `RowState::kept`):
/// `items` cards, keyed as the fixture keys them, landed as a reload lands. Returns whether a row
/// landed.
#[cfg(any(test, feature = "test-support"))]
pub fn land_ring_rows_for_test(state: &mut PmsState, items: usize, salt: &dyn Fn(usize) -> usize) -> bool {
    plx_base::testlock::assert_held("the pms hub catalog (land_ring_rows_for_test)");
    let scope = BrowseScope::standalone();
    let wanted = wanted_reloads(&state.srcs, &scope.pins, published_home(state).hubs.len(), state.hold);
    let mut srcs = std::mem::take(&mut state.srcs);
    let mut landed = false;
    for (source, want) in srcs.iter_mut().zip(wanted) {
        for (id, key) in want {
            let Some(row) = key.rsplit('/').next().and_then(|row| row.parse::<usize>().ok()) else { continue };
            let Some(shelf) = source.last.as_ref().and_then(|build| find_shelf(&build.shelves, &id, &key)).cloned() else { continue };
            let cards = (0..items).map(|c| Arc::new(PmsMovie {
                sid: source.sid,
                rk: format!("{}-{row}-{c}", salt(row)),
                art: format!("/art/{}", c + 1),
                ..PmsMovie::default()
            })).collect();
            let page = PageQuery { id, key, start: shelf.offset, before: false, hidden: Vec::new(), reload: true };
            let build = SourceBuild { cw: Vec::new(), lane: Default::default(), shelves: vec![Shelf {
                items: cards, positions: (0..items).collect(), shown: 0, ..shelf }] };
            landed |= land_reload(source, &page, build);
        }
    }
    if landed {
        let (mut build, _) = merge_held(&mut srcs, &scope, state.hold);
        state.srcs = srcs;
        preserve_heroes(state, &mut build);
        commit(state, build);
    } else {
        state.srcs = srcs;
    }
    landed
}

/// Test hook: the answer a server would give every held row that waits for the cards it showed
/// (`RowState::kept`), landed as a reload lands. A fixture seeded without a server has no one to
/// answer those reads, so a row the ring released would stay placeholders for good; a real Home
/// has its cards back within a few frames of holding the row again. Returns whether a row landed.
#[cfg(any(test, feature = "test-support"))]
pub fn land_kept_rows_for_test(state: &mut PmsState) -> bool {
    plx_base::testlock::assert_held("the pms hub catalog (land_kept_rows_for_test)");
    let scope = BrowseScope::standalone();
    let wanted = wanted_reloads(&state.srcs, &scope.pins, published_home(state).hubs.len(), state.hold);
    let mut srcs = std::mem::take(&mut state.srcs);
    let mut landed = false;
    for (source, want) in srcs.iter_mut().zip(wanted) {
        for (id, key) in want {
            let Some(shelf) = source.last.as_ref().and_then(|build| find_shelf(&build.shelves, &id, &key))
                .filter(|shelf| !shelf.row.kept.is_empty()).cloned() else { continue };
            let page = PageQuery { id, key, start: shelf.offset, before: false, hidden: Vec::new(), reload: true };
            let mut many = |keys: &[String]| {
                let items: Vec<serde_json::Value> = keys.iter().map(|key| serde_json::json!({
                    "ratingKey": key, "type": "movie", "title": "Movie", "thumb": "/poster"})).collect();
                serde_json::from_value(serde_json::json!({"Metadata": items})).ok()
            };
            let Some(build) = fetch_kept(source.sid, &page, &shelf, &mut many) else { continue };
            landed |= land_reload(source, &page, build);
        }
    }
    if landed {
        let (mut build, _) = merge_held(&mut srcs, &scope, state.hold);
        state.srcs = srcs;
        preserve_heroes(state, &mut build);
        commit(state, build);
    } else {
        state.srcs = srcs;
    }
    landed
}

/// [`seed_grid_for_test`] with each row's provider identity named: `(hubIdentifier, key, title)`.
/// A linked collection shelf is a `custom.collection.*` row, so its fixtures need both halves.
#[cfg(any(test, feature = "test-support"))]
pub fn seed_named_hubs_for_test(
    state: &mut PmsState,
    adapter: &Arc<PmsAdapter>,
    items: usize,
    rows: &[(&str, &str, &str)],
) {
    plx_base::testlock::assert_held("the pms hub catalog (seed_named_hubs_for_test)");
    seed_for_test(state, adapter, items, HubState::Ready);
    let source = state.srcs[0].last.as_mut().unwrap();
    source.shelves = rows.iter().map(|(hub_id, key, title)| {
        let mut shelf = build_test(items).shelves.remove(0);
        shelf.hub_id = (*hub_id).into();
        shelf.key = (*key).into();
        shelf.title = (*title).into();
        shelf
    }).collect();
    let build = merge(&state.srcs);
    commit(state, build);
}

#[cfg(any(test, feature = "test-support"))]
pub fn seed_recent_window_for_test(state: &mut PmsState, adapter: &Arc<PmsAdapter>, offset: usize, items: usize, more: bool) {
    seed_named_hubs_for_test(state, adapter, items,
        &[("home.movies.recent", "/hubs/home/recentlyAdded?type=1", "Recent")]);
    let shelf = &mut state.srcs[0].last.as_mut().unwrap().shelves[0];
    shelf.offset = offset;
    shelf.end = offset + items;
    shelf.positions = (offset..offset + items).collect();
    shelf.more = more;
    for (i, item) in shelf.items.iter_mut().enumerate() { Arc::make_mut(item).rk = (offset + i + 1).to_string(); }
    let build = merge(&state.srcs);
    commit(state, build);
}

/// Give every card of the first shelf its own `UltraBlurColors` envelope (`blur_of(index)`), so a
/// test can tell WHICH card a colour-keyed ground was keyed to. Call after a `seed_*_for_test`.
#[cfg(any(test, feature = "test-support"))]
pub fn blur_first_shelf_for_test(state: &mut PmsState, blur_of: impl Fn(usize) -> [[f32; 3]; 4]) {
    let shelf = &mut state.srcs[0].last.as_mut().unwrap().shelves[0];
    for (i, item) in shelf.items.iter_mut().enumerate() {
        let m = Arc::make_mut(item);
        m.blur = blur_of(i);
        m.has_blur = true;
    }
    let build = merge(&state.srcs);
    commit(state, build);
}

/// Rewrite the first shelf's cards with `edit` and publish the result the way a REFRESHED landing
/// does: a fresh merge, so the hero pool is rebuilt from the new rows rather than preserved (the
/// post-playback refetch — a finished film leaving Continue Watching, the next episode replacing the
/// one that was watched). Call after a `seed_*_for_test`.
#[cfg(any(test, feature = "test-support"))]
pub fn edit_first_shelf_for_test(state: &mut PmsState, edit: impl FnOnce(&mut Vec<Arc<PmsMovie>>)) {
    let shelf = &mut state.srcs[0].last.as_mut().unwrap().shelves[0];
    edit(&mut shelf.items);
    shelf.positions = (0..shelf.items.len()).collect();
    let build = merge(&state.srcs);
    commit(state, build);
}

/// A Home whose only row is a Continue Watching deck of `items` cards with `more` behind them.
#[cfg(any(test, feature = "test-support"))]
pub fn seed_continue_window_for_test(state: &mut PmsState, adapter: &Arc<PmsAdapter>, items: usize, more: bool) {
    let _ = adapter;
    let sid = ServerId::from_raw(0);
    let cw: Vec<CwItem> = (0..items).map(|i| CwItem {
        last_viewed_at: 10_000 - i as i64,
        m: Arc::new(PmsMovie { sid, rk: i.to_string(), title: i.to_string(), thumb: "/t".into(), art: "/a".into(),
            ..Default::default() }),
        position: i,
    }).collect();
    let mut source = Src::new(sid, String::new(), &registry_keys_for_test());
    source.state = HubState::Ready;
    let lane = deck::Lane::preview("0", &items.saturating_sub(1).to_string(), items, items + usize::from(more), !more);
    source.last = Some(SourceBuild { cw, lane, shelves: Vec::new() });
    let mut srcs = vec![source];
    settle_deck(&mut srcs, false);
    let build = merge(&srcs);
    state.srcs = srcs;
    remember_roster(state, &BrowseScope::standalone());
    state.seen_facts = facts_key();
    commit(state, build);
}

#[cfg(any(test, feature = "test-support"))]
pub fn reverse_test_hubs(state: &mut PmsState) {
    plx_base::testlock::assert_held("the pms hub catalog (reverse_test_hubs)");
    for source in state.srcs.iter_mut() {
        if let Some(build) = source.last.as_mut() { build.shelves.reverse(); }
    }
    let build = merge(&state.srcs);
    commit(state, build);
}

#[cfg(any(test, feature = "test-support"))]
pub fn remove_test_item(state: &mut PmsState, rk: &str) {
    plx_base::testlock::assert_held("the pms hub catalog (remove_test_item)");
    for source in state.srcs.iter_mut() {
        if let Some(build) = source.last.as_mut() {
            for shelf in &mut build.shelves {
                let mut index = 0;
                shelf.positions.retain(|_| {
                    let keep = shelf.items[index].rk != rk;
                    index += 1;
                    keep
                });
                shelf.items.retain(|item| item.rk != rk);
            }
        }
    }
    let build = merge(&state.srcs);
    commit(state, build);
}

/// Test hook: turn the shelf item `rk` into a collection, which the item menu has nothing to offer
/// (`has_actions`) — the one Home card whose hold the app DECLINES.
#[cfg(any(test, feature = "test-support"))]
pub fn retag_test_item_as_collection(state: &mut PmsState, rk: &str) {
    plx_base::testlock::assert_held("the pms hub catalog (retag_test_item_as_collection)");
    for source in state.srcs.iter_mut() {
        if let Some(build) = source.last.as_mut() {
            for shelf in &mut build.shelves {
                for item in shelf.items.iter_mut().filter(|item| item.rk == rk) {
                    Arc::make_mut(item).kind = KIND_COLLECTION;
                }
            }
        }
    }
    let build = merge(&state.srcs);
    commit(state, build);
}

/// Test hook: rewrite every shelf item `rk` in place (it may change its own rating key, kind, show
/// or server) and republish: the server answering a finished episode with the show's next one.
#[cfg(any(test, feature = "test-support"))]
pub fn edit_test_item(state: &mut PmsState, rk: &str, edit: &dyn Fn(&mut PmsMovie)) {
    plx_base::testlock::assert_held("the pms hub catalog (edit_test_item)");
    for source in state.srcs.iter_mut() {
        if let Some(build) = source.last.as_mut() {
            for shelf in &mut build.shelves {
                for item in shelf.items.iter_mut().filter(|item| item.rk == rk) {
                    edit(Arc::make_mut(item));
                }
            }
        }
    }
    let build = merge(&state.srcs);
    commit(state, build);
}

/// Test hook: the Home a real library has: `items` Continue Watching EPISODES, each of its own show
/// (`show<rk>`), as the source's preview with the deck PLACED (so Continue Watching is the deck row,
/// row 0), above one Recently Added shelf (row 1). Settled and published.
#[cfg(any(test, feature = "test-support"))]
pub fn seed_deck_home_for_test(state: &mut PmsState, adapter: &Arc<PmsAdapter>, items: usize) {
    plx_base::testlock::assert_held("the pms hub catalog (seed_deck_home_for_test)");
    seed_with_scope_for_test(state, adapter, ServerId::UNSET, 0, HubState::Ready, &BrowseScope::standalone());
    let card = |rk: String| Arc::new(PmsMovie {
        art: format!("/art/{rk}"), thumb: format!("/t/{rk}"), title: rk.clone(), kind: 3,
        show_rk: format!("show{rk}"), rk, ..PmsMovie::default()
    });
    let mut recent = build_test(3).shelves.remove(0);
    recent.hub_id = "home.movies.recent".into();
    recent.title = "Recently Added".into();
    for (i, item) in recent.items.iter_mut().enumerate() { Arc::make_mut(item).rk = format!("r{}", i + 1); }
    let mut source = Src::new(ServerId::UNSET, String::new(), &registry_keys_for_test());
    source.state = HubState::Ready;
    source.ledger = state.ledger.clone();
    source.last = Some(SourceBuild {
        cw: (1..=items).map(|i| CwItem { last_viewed_at: (1000 - i) as i64, m: card(i.to_string()), position: 0 }).collect(),
        shelves: vec![recent],
        lane: Default::default(),
    });
    let mut srcs = vec![source];
    settle_deck(&mut srcs, true);
    let build = merge_with_scope(&srcs, &BrowseScope::standalone());
    state.srcs = srcs;
    commit(state, build);
}

/// Test hook: Home as ONE server's `/hubs` answer makes it, projected the way the refresh projects it
/// (`Projection`: `listable`, `shelf_card`, the hub order) and then committed, so a test sees what a
/// household would, and not what a hand-built row would. `seed_deck_home_for_test` and the rest build
/// their rows by hand and so can never observe a row the projection drops.
#[cfg(any(test, feature = "test-support"))]
pub fn seed_hubs_response_for_test(state: &mut PmsState, adapter: &Arc<PmsAdapter>, mc: &plx_plex::plex::MediaContainer) {
    plx_base::testlock::assert_held("the pms hub catalog (seed_hubs_response_for_test)");
    seed_with_scope_for_test(state, adapter, ServerId::UNSET, 0, HubState::Ready, &BrowseScope::standalone());
    let mut projection = Projection::new(ServerId::UNSET, None);
    projection.take(&mc.hub);
    let mut source = Src::new(ServerId::UNSET, String::new(), &registry_keys_for_test());
    source.state = HubState::Ready;
    source.ledger = state.ledger.clone();
    source.last = Some(projection.finish());
    let srcs = vec![source];
    let build = merge_with_scope(&srcs, &BrowseScope::standalone());
    state.srcs = srcs;
    commit(state, build);
}

/// Test hook: what a quiet read of the first source brought, without the read: its answer is its
/// last one with every shelf item `rk` rewritten by `edit`, held in `staged` for
/// [`HubsCmd::Publish`](crate::stores::hubs::HubsCmd) to land. Publishes nothing.
#[cfg(any(test, feature = "test-support"))]
pub fn stage_test_item(state: &mut PmsState, rk: &str, edit: &dyn Fn(&mut PmsMovie)) {
    plx_base::testlock::assert_held("the pms hub catalog (stage_test_item)");
    let source = state.srcs.first_mut().expect("a seeded source");
    let mut staged = source.last.clone().expect("a source that has answered");
    staged.lane = Default::default(); // a fresh answer has not had the deck placed on it
    for shelf in &mut staged.shelves {
        for item in shelf.items.iter_mut().filter(|item| item.rk == rk) { edit(Arc::make_mut(item)); }
    }
    for card in staged.cw.iter_mut().filter(|card| card.m.rk == rk) { edit(Arc::make_mut(&mut card.m)); }
    source.staged = Some(staged);
    state.staged_gen = state.staged_gen.wrapping_add(1);
}

/// Test hook: move the shelf item `rk` to the front of its shelf and republish: Continue Watching
/// is ordered by last viewed, so the next episode of a show just watched arrives in column 0.
#[cfg(any(test, feature = "test-support"))]
pub fn promote_test_item(state: &mut PmsState, rk: &str) {
    plx_base::testlock::assert_held("the pms hub catalog (promote_test_item)");
    for source in state.srcs.iter_mut() {
        if let Some(build) = source.last.as_mut() {
            for shelf in &mut build.shelves {
                if let Some(at) = shelf.items.iter().position(|item| item.rk == rk) {
                    let item = shelf.items.remove(at);
                    shelf.items.insert(0, item);
                }
            }
        }
    }
    let build = merge(&state.srcs);
    commit(state, build);
}

/// Test hook: the server offset every shelf's window now starts at (a window that slid under the
/// cards it held), republished.
#[cfg(any(test, feature = "test-support"))]
pub fn slide_test_window(state: &mut PmsState, offset: usize) {
    plx_base::testlock::assert_held("the pms hub catalog (slide_test_window)");
    for source in state.srcs.iter_mut() {
        if let Some(build) = source.last.as_mut() {
            for shelf in &mut build.shelves {
                shelf.end = offset + shelf.items.len();
                shelf.offset = offset;
            }
        }
    }
    let build = merge(&state.srcs);
    commit(state, build);
}

#[cfg(test)]
#[path = "pms_test_support.rs"]
mod test_support;

#[cfg(test)]
#[path = "pms_catalog_commit_tests.rs"]
mod catalog_commit_tests;

#[cfg(test)]
#[path = "pms_fetch_retry_tests.rs"]
mod fetch_retry_tests;

#[cfg(test)]
#[path = "pms_local_edit_tests.rs"]
mod local_edit_tests;

#[cfg(test)]
#[path = "pms_hero_pool_tests.rs"]
mod hero_pool_tests;

#[cfg(test)]
#[path = "pms_multi_source_merge_tests.rs"]
mod multi_source_merge_tests;

#[cfg(test)]
#[path = "pms_recent_paging_tests.rs"]
mod recent_paging_tests;

#[cfg(test)]
#[path = "pms_owed_tests.rs"]
mod owed_tests;

#[cfg(test)]
#[path = "pms_staged_tests.rs"]
mod staged_tests;

#[cfg(test)]
#[path = "pms_held_keys_tests.rs"]
mod held_keys_tests;

/// The library's tile abstraction (restructure spec §10) over a catalog row: the one place a
/// `PmsMovie` becomes a `Tile`, so a widget that draws a tile asks the trait and never this type.
impl plx_base::tile::Tile for PmsMovie {
    fn title(&self) -> &str {
        &self.title
    }
    fn poster(&self) -> Option<(u16, &str)> {
        (!self.thumb.is_empty()).then_some((self.sid.raw(), self.thumb.as_str()))
    }
    fn progress(&self) -> Option<f32> {
        self.resume_frac()
    }
    fn watched(&self) -> bool {
        self.watched
    }
    fn unwatched(&self) -> bool {
        self.unwatched
    }
}

#[cfg(test)]
#[path = "pms_hub_row_paging_tests.rs"]
mod hub_row_paging_tests;

#[cfg(test)]
#[path = "pms_deck_tests.rs"]
mod deck_tests;

#[cfg(test)]
#[path = "pms_home_row_ring_tests.rs"]
mod home_row_ring_tests;

#[cfg(test)]
#[path = "pms_home_hub_windows_tests.rs"]
mod home_hub_windows_tests;

#[cfg(test)]
mod caption_fact_tests {
    use super::*;

    fn row(json: &str) -> PmsMovie {
        let it: plx_plex::plex::Metadata = serde_json::from_str(json).expect("fixture parses");
        parse_item(&it, ServerId::UNSET)
    }

    #[test]
    fn a_movie_carries_genre_resolution_hdr_score_and_added_at() {
        let m = row(r#"{"type":"movie","title":"M","ratingKey":"1","audienceRating":7.8,"addedAt":1700000000,
            "Genre":[{"tag":"Drama"},{"tag":"Crime"}],
            "Media":[{"videoResolution":"4k","Part":[{"Stream":[
                {"streamType":1,"colorTrc":"smpte2084"},{"streamType":2}]}]}]}"#);
        assert_eq!(m.genre.as_deref(), Some("Drama"), "the FIRST genre");
        assert_eq!(m.video_resolution.as_deref(), Some("4k"));
        assert!(m.hdr);
        assert_eq!(m.audience_rating, Some(7.8));
        assert_eq!(m.added_at, Some(1_700_000_000));
        assert!(!m.smart && m.leaf_count.is_none() && m.child_count == 0);
    }

    #[test]
    fn an_sdr_movie_is_not_hdr_and_unknown_stays_unknown() {
        let m = row(r#"{"type":"movie","title":"M","Media":[{"videoResolution":"1080",
            "Part":[{"Stream":[{"streamType":1,"colorTrc":"bt709"}]}]}]}"#);
        assert!(!m.hdr);
        assert_eq!((m.genre, m.audience_rating, m.added_at, m.subtype), (None, None, None, None));
    }

    #[test]
    fn dolby_vision_without_a_transfer_characteristic_is_hdr() {
        let m = row(r#"{"type":"movie","title":"M","Media":[{"Part":[{"Stream":[
            {"streamType":1,"DOVIPresent":true}]}]}]}"#);
        assert!(m.hdr);
    }

    #[test]
    fn a_show_carries_its_season_and_episode_counts() {
        let m = row(r#"{"type":"show","title":"S","childCount":3,"leafCount":30,"viewedLeafCount":12}"#);
        assert_eq!((m.child_count, m.leaf_count, m.viewed_leaf_count), (3, Some(30), Some(12)));
    }

    #[test]
    fn a_season_has_no_member_count() {
        let m = row(r#"{"type":"season","title":"S1","childCount":9,"leafCount":9}"#);
        assert_eq!(m.child_count, 0);
    }

    #[test]
    fn a_smart_collection_is_told_from_a_normal_one() {
        let smart = row(r#"{"type":"collection","title":"C","subtype":"movie","smart":"1","childCount":14}"#);
        assert!(smart.smart);
        assert_eq!((smart.child_count, smart.subtype.as_deref()), (14, Some("movie")));
        let smart_bool = row(r#"{"type":"collection","title":"C","smart":true}"#);
        assert!(smart_bool.smart);
        let normal = row(r#"{"type":"collection","title":"C","subtype":"movie","smart":"0","childCount":5}"#);
        assert!(!normal.smart && normal.child_count == 5);
        let absent = row(r#"{"type":"collection","title":"C"}"#);
        assert!(!absent.smart);
    }
}

#[cfg(test)]
mod sort_info_tests {
    use super::*;
    #[test]
    fn listing_keeps_distinct_scores_and_default_version_without_a_detail_request() {
        let it: plx_plex::plex::Metadata = serde_json::from_value(serde_json::json!({
            "type":"movie","ratingKey":"7","title":"Film","contentRating":"PG-13",
            "rating":10.0,"ratingImage":"rottentomatoes://image.rating.ripe",
            "audienceRating":8.5,"audienceRatingImage":"rottentomatoes://image.rating.spilled",
            "userRating":6.0,"viewCount":3,"addedAt":1700000000,"lastViewedAt":1700000010,
            "Media":[{"id":1,"videoResolution":"1080","width":1920,"height":802,"bitrate":8000},
                     {"id":2,"videoResolution":"4k","width":3840,"height":2160,"bitrate":40000}]
        })).unwrap();
        let row = parse_item(&it, ServerId::UNSET);
        let facts = row.sort_info.as_ref().unwrap();
        assert_eq!(row.rating, "PG-13");
        assert_eq!(facts.rating, Some(10.0));
        assert_eq!(row.audience_rating, Some(8.5));
        assert_eq!(facts.user_rating, Some(6.0));
        assert_eq!((facts.view_count, facts.last_viewed_at), (Some(3), Some(1700000010)));
        assert_eq!((facts.bitrate_kbps, facts.width, facts.height, facts.media_count), (Some(8000),1920,802,2));
        assert_eq!(row.video_resolution.as_deref(), Some("1080"));
        let roundtrip: PmsMovie = serde_json::from_str(&serde_json::to_string(&row).unwrap()).unwrap();
        assert!(roundtrip == row);
    }

    /// A show's listing row (a PMS 1.43.5 `type=2` row, trimmed) carries the same sort facts a movie's
    /// does, and its episode counts: a TV Shows grid is captioned from the listing, with no detail request.
    #[test]
    fn a_show_row_keeps_its_sort_facts_and_episode_counts() {
        let it: plx_plex::plex::Metadata = serde_json::from_value(serde_json::json!({
            "type":"show","ratingKey":"2175","title":"Series","year":2013,"viewCount":5,"lastViewedAt":1791619274,
            "originallyAvailableAt":"2013-09-29","leafCount":37,"viewedLeafCount":28,"childCount":2,"addedAt":1788473416,
            "updatedAt":1791694124,"audienceRating":8.3,"audienceRatingImage":"themoviedb://image.rating"
        })).unwrap();
        let row = parse_item(&it, ServerId::UNSET);
        assert_eq!(row.kind, KIND_SHOW);
        let facts = row.sort_info.as_ref().expect("a show row carries sort facts");
        assert_eq!((facts.view_count, facts.last_viewed_at), (Some(5), Some(1791619274)));
        assert_eq!(facts.audience_image, "themoviedb://image.rating");
        assert_eq!((row.leaf_count, row.viewed_leaf_count, row.added_at, row.aired.as_str()), (Some(37), Some(28), Some(1788473416), "2013-09-29"));
        // A collection keeps none: its grid words no sort value.
        let other: plx_plex::plex::Metadata = serde_json::from_value(serde_json::json!({"type":"collection","ratingKey":"9","title":"x"})).unwrap();
        assert!(parse_item(&other, ServerId::UNSET).sort_info.is_none());
    }

    /// An episode's listing row (a `type=4` row, trimmed) carries the facts its sorts read: the scores and
    /// their providers, its own air date, the dates the library recorded, its play state and its media.
    /// A season's row carries the dates and play state it has and its episode counts. Soft: a row that
    /// sends none of them still parses, every fact unknown rather than a zero a reader could mistake
    /// for a value.
    #[test]
    fn episode_and_season_rows_keep_their_sort_facts() {
        let ep: plx_plex::plex::Metadata = serde_json::from_value(serde_json::json!({
            "type":"episode","ratingKey":"31","title":"Pilot","grandparentTitle":"Series","parentIndex":1,"index":22,
            "rating":7.5,"ratingImage":"rottentomatoes://image.rating.ripe",
            "audienceRating":8.4,"audienceRatingImage":"themoviedb://image.rating","userRating":9.0,
            "originallyAvailableAt":"2014-05-12","year":2014,"duration":1320000,"viewCount":2,
            "addedAt":1700000000,"lastViewedAt":1700000010,"contentRating":"TV-14",
            "Media":[{"id":1,"videoResolution":"1080","width":1920,"height":1080,"bitrate":6500}]
        })).unwrap();
        let row = parse_item(&ep, ServerId::UNSET);
        assert_eq!(row.kind, KIND_EPISODE);
        let facts = row.sort_info.as_ref().expect("an episode row carries sort facts");
        assert_eq!((facts.rating, facts.user_rating), (Some(7.5), Some(9.0)));
        assert_eq!((facts.rating_image.as_str(), facts.audience_image.as_str()), ("rottentomatoes://image.rating.ripe", "themoviedb://image.rating"));
        assert_eq!((facts.view_count, facts.last_viewed_at), (Some(2), Some(1700000010)));
        assert_eq!((facts.bitrate_kbps, facts.width, facts.height, facts.media_count), (Some(6500), 1920, 1080, 1));
        assert_eq!((row.audience_rating, row.aired.as_str(), row.added_at, row.rating.as_str()), (Some(8.4), "2014-05-12", Some(1700000000), "TV-14"));
        let season: plx_plex::plex::Metadata = serde_json::from_value(serde_json::json!({
            "type":"season","ratingKey":"32","title":"Season 1","index":1,"leafCount":22,"viewedLeafCount":22,
            "viewCount":9,"addedAt":1700000000,"lastViewedAt":1700000020
        })).unwrap();
        let row = parse_item(&season, ServerId::UNSET);
        assert_eq!(row.kind, KIND_SEASON);
        let facts = row.sort_info.as_ref().expect("a season row carries sort facts");
        assert_eq!((facts.view_count, facts.last_viewed_at), (Some(9), Some(1700000020)));
        assert_eq!((row.leaf_count, row.viewed_leaf_count, row.added_at), (Some(22), Some(22), Some(1700000000)));
        // A row that sends none of it: nothing invented.
        for kind in ["season", "episode"] {
            let bare: plx_plex::plex::Metadata = serde_json::from_value(serde_json::json!({"type":kind,"ratingKey":"9","title":"x"})).unwrap();
            let row = parse_item(&bare, ServerId::UNSET);
            let facts = row.sort_info.as_ref().expect(kind);
            assert_eq!((facts.rating, facts.user_rating, facts.last_viewed_at, facts.bitrate_kbps), (None, None, None, None), "{kind}");
            assert_eq!((row.audience_rating, row.added_at), (None, None), "{kind}");
        }
        // An older recorded row, which has no `sort_info` key at all, still parses: the field is soft.
        let mut old = serde_json::to_value(&parse_item(&ep, ServerId::UNSET)).unwrap();
        old.as_object_mut().unwrap().remove("sort_info");
        let old: PmsMovie = serde_json::from_value(old).unwrap();
        assert!(old.sort_info.is_none());
    }

    /// Marking a show watched or unwatched moves its episode counts with the mark, so a grid sorted by
    /// Unplayed does not keep wearing "9 episodes" under a watched tick until the listing is read again.
    #[test]
    fn marking_a_container_moves_its_episode_counts() {
        let mut show = PmsMovie { kind: KIND_SHOW, leaf_count: Some(37), viewed_leaf_count: Some(28), ..Default::default() };
        set_watched(&mut show, true);
        assert_eq!((show.viewed_leaf_count, show.watched, show.unwatched), (Some(37), true, false));
        set_watched(&mut show, false);
        assert_eq!((show.viewed_leaf_count, show.watched, show.unwatched), (Some(0), false, true));
        // No counts sent: nothing invented. A movie has none to move.
        let mut bare = PmsMovie { kind: KIND_SHOW, ..Default::default() };
        set_watched(&mut bare, true);
        assert_eq!(bare.viewed_leaf_count, None);
        let mut movie = PmsMovie { leaf_count: Some(1), viewed_leaf_count: Some(0), ..Default::default() };
        set_watched(&mut movie, true);
        assert_eq!(movie.viewed_leaf_count, Some(0));
    }
}
