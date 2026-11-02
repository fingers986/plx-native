Warning: truncated output (original token count: 138812)
Total output lines: 9668

//! The frame loop — phase 1b-ii's THIN COORDINATOR (restructure spec §13).
//!
//! `run` is the one `while app.running` loop; each phase of an iteration is a function below,
//! cut VERBATIM out of `plex_run`'s former body at the eight `FRAMEDROP` stamps (spec §8.4) so
//! that a diff of this commit is moves and renames only — `app.<field>` for what used to be a
//! loop-local, `fr.<field>` ([`Frame`]) for the handful of per-iteration values that cross a
//! phase boundary. Nothing is reordered;
//! the positional frame points the restructure names (NAV COMMIT, `opaque_route`, the present
//! decision, `clear_opaque_region` inside [`draw`], the swap) are the labelled lines of `run`
//! itself, which is why they are not folded into a phase function.
//!
//! Phase 2 replaces this loop with `ui::dispatch`'s ten-step algorithm (spec §3.3); the phase
//! names here are that algorithm's, mapped onto today's order (`navcommit` still runs BEFORE
//! `tick_drain` on this loop, and the `FRAMEDROP` line prints them in the spec's order).
//!
//! **Phase 5b is where the replacement stopped being a shadow.** The Settings family — the root,
//! Legal and its documents, Privacy & data, the first-run consent question and Favourite libraries
//! in both its modes — is now OWNED by the dispatcher, and this loop asks it four questions and
//! obeys one queue:
//!
//! * `Dispatcher::owns_input` — asked by the key, pointer, click and wheel arms alike. When it is
//!   true the arm pushes an `InputEvent` onto `app.inputs` and `continue`s; the ladders see
//!   nothing. Four hand-written modal arms and their ordering left this file with it.
//! * `bridge::host_frozen` / `host_replaced` — the container's fold, replacing the pairs of
//!   `is_open()` / `host_ground_ready()` reads the update and draw phases each had to keep in step.
//! * `bridge::page_owned` — whether the dispatcher draws the top PAGE too, folded with
//!   `host_replaced` into `bridge::page_plan` (`Owned` / `LegacyThenSurfaces` / `SurfacesOnly`),
//!   which is what the page closure and `Dispatcher::draw`'s single call per frame both read.
//! * `Bridge::take_reqs` — what an owned screen asked this loop to do (`loop_requests`), because
//!   the machines that own sign-in and the app's real stack are not on the dispatcher yet.
//!
//! **Phase 10 moved the boot-trigger scripts out of this file.** Where this loop used to call a
//! local `dev_scripts` and hold each arm's own retry counters and oscillator phases as fields on
//! `App` directly (`app.dev.<flag>`), it now calls [`crate::dev::scenarios::each_frame`] — one
//! call at the same position `dev_scripts` occupied — and the per-arm state lives on
//! `app.scenarios`. The oscillator continuations invoked from `update()` (`nav_osc_tick`,
//! `hero_osc_tick`, `search_osc_tick`, …) and the replay-after-EOS check in `playback_tick` moved
//! the same way; see `dev::scenarios`'s own module doc for the full inventory.
use super::*;
use plx_appkit::player_hud::geometry::{GROUP_ROW, GROUP_TABS};

/// One lazy product frame-tail hash path for recording and replay grading alike.
pub(super) fn recorder_end_frame(
    rec: &mut super::recorder::Recplay,
    bridge: &super::bridge::Bridge,
    press: &plx_ui::press::Press,
    route: &str,
    overlay: &str,
    focus: &str,
    tree: u64,
) -> bool {
    rec.measurements(bridge);
    rec.content_end();
    rec.end_frame_with_gate(&|| super::recorder::state_hash(
        press, route, overlay, focus, tree, bridge.session_subhash(), bridge.consent_subhash(),
        bridge.initial_subhash(),
    ), &|id| bridge.store_gen(id), bridge.landgate())
}

/// The per-iteration values that cross a phase boundary. Reset at the top of every iteration
/// (`Frame::begin`); every field is written by one phase and read by the ones after it, except
/// `ctrl`, which `content_requests` may re-resolve once after a player request changes the
/// metadata store's skipped set.
pub(crate) struct Frame {
    /// The HUD's control slot for this frame, sampled by `Frame::begin` for input and the pump.
    /// `content_requests` re-resolves it once, and pushes it to the page, on a frame where BACK
    /// dismissed an offer; `update` and `report` read the refreshed value.
    pub(crate) ctrl: plx_appkit::player_hud::ControlSlot,
    /// `clock::now()` at the ingest boundary — THE frame time every phase after it uses.
    pub(crate) now: u32,
    /// Seconds since the previous frame's `now`, clamped to 50 ms (the animation timestep).
    pub(crate) dt: f32,
    /// Something under a modal surface is still moving (its host must not freeze yet).
    pub(crate) underlay_moving: bool,
    /// The route is the player. NOT the present-gate question, which is
    /// `Player::video_plane_bound` (phase 9) — this is only "which screen is up", and the one
    /// thing still keyed on it is the draw's own `clear_opaque_region` + transparent clear.
    pub(crate) player: bool,
    /// This iteration draws and swaps (the present decision, spec §3.3 step 8).
    pub(crate) present: bool,
    /// This iteration did NOT present only because a page capture is still in flight on the GPU
    /// (`gfx::snapshot_pending`): the window was active, so the fence was the one reason. Pure
    /// bookkeeping for the present-interval stamps and the bench; it never feeds a decision.
    pub(crate) deferred: bool,
    /// The heartbeat's route word for this frame (`route_word`).
    pub(crate) rn: &'static str,
}

impl Frame {
    fn begin(ps: &plx_media::route::PlaybackSession, meta: plx_data::metadata::MetadataView<'_>) -> Frame {
        Frame {
            ctrl: plx_appkit::player_hud::slot(ps, meta),
            now: 0,
            dt: 0.0,
            underlay_moving: false,
            player: false,
            present: false,
            deferred: false,
            rn: "",
        }
    }
}

/// The loop. `plex_run` calls this once, after `boot`, and `shutdown` after it returns.
///
/// **No `mt: &MainThread` parameter since phase 9.** The token is a FIELD now
/// (`app.adapters.player`, see `player::adapter`), so the proof travels with `&mut App` — every
/// phase function below reaches the native session as `&mut app.adapters.player` and the seam as
/// `app.adapters.player.mt()`. Threading a second `&MainThread` beside `&mut App` would have
/// needed a second token, and minting one is the hole `MainThread::assume` documents.
pub(crate) unsafe fn run(app: &mut App) {
    let watchdog = plx_base::task::watchdog::LoopWatch::start();
    let mut clock_notice = super::clock_notice::ClockNotice::new();
    let mut hint_taught = super::hint_taught::HintTaught::new();
    while app.running {
        watchdog.advance();
        plx_telemetry::telemetry::session::tick(matches!(app.route(), AppArg::Player));
        let _frame_scope = plx_base::task::FrameScope::enter();
        #[cfg(all(feature = "hostsim", target_os = "linux"))]
        let wslg_frame_budget = app.wslg_frame_pacing.then(super::window_activity::WslgFrameBudget::begin);
        // Resolve the control row ONCE per iteration, before the event pump, and pass this
        // value to input, update and draw alike. `player_hud::slot()` reads `playpos_ns`, which
        // LG's media thread writes and `player::pump` advances mid-iteration — deriving it per
        // call site let a keypress activate a control this same frame then declined to draw.
        // Frame-drop detector, stamp 0 of 9: the TOP of the iteration, before the ls2 pump
        // and the event poll — `worstframe=` covers the whole iteration, not the half after
        // the input. Eight phases between the nine stamps, named after the frame algorithm
        // the restructure moves this loop onto (spec §3.3/§8.4); on THIS loop navcommit
        // precedes tick_drain, and the FRAMEDROP line prints them in the algorithm's order.
        let mut fr = Frame::begin(&app.player.session, app.bridge.metadata_view());
        let first_controlled_frame = app.boot_initial.is_some() && app.prev == 0;
        let fr = &mut fr;
        app.instr.mark(plx_base::diag::heartbeat::Phase::Top); // also starts the visit's hang interval
        // The frame index the LANDING SCHEDULE stamps against (§3.3 step 3, `plx_machine::landgate`),
        // published at the TOP because a landing site is reachable from the dev scenarios below
        // as well as from `land_results` and the dispatcher's own frame. One relaxed atomic load
        // unless `plxnative-rec` or `plxnative-recplay` is armed.
        app.rec.begin_frame(app.bridge.landgate());
        // No modal dim has been drawn yet this frame, on any route (the player and a bound preview never
        // reach `popover::host::begin_frame`, which used to be the only place this was cleared).
        plx_ui::popover::host::clear_dims();
        // REPLAY (`plxnative-recplay`): this frame runs on the recorded tick — set BEFORE
        // ingest, whose key arms stamp `last_input` from the clock — and the frame's recorded
        // inputs are re-injected through the same synthesis the remote FIFO uses, so the poll
        // below consumes them exactly as it consumed the originals.
        if let Some(t) = app.rec.replay_tick() {
            clock::set_replay(t.ms);
            for v in app.rec.replay_inputs() {
                replay_inject(app, fr, &v);
            }
        }
        // The frame dump (`dev::framedump`) owns the clock and the placeholder count from here:
        // it must run before anything reads `clock::now()`.
        #[cfg(all(feature = "hostsim", feature = "devtriggers"))]
        if crate::dev::framedump::requested() {
            crate::dev::framedump::iteration_begin(app);
        }
        plx_platform::tv::window::pump_bus();
        plx_platform::tv::home::poll();
        // The one toast key mode owes the viewer (`net::keypin`'s facts), on every route.
        clock_notice.poll();
        // What the hold hint has taught since last frame, written once to the install's session.
        hint_taught.poll(app.boot_initial.is_some());
        ingest(app, fr);
        app.instr.mark(plx_base::diag::heartbeat::Phase::Ingest); // ingest

        fr.now = clock::now();
        // Share the actual/replay frame timestamp, before spring dt is clamped.
        plx_ui::card_motion::begin_frame(fr.now);
        // **The Player machine's tick** (spec §4.1): set ONCE per iteration, from the same
        // `fr.now` every other phase of this frame reads. `PlaybackSession::auto_last_switch` — the
        // adaptive controller's "how long since the last visible rung change" — is stamped from
        // it, so that stamp cannot disagree with the frame it belongs to. It used to come from
        // `player::vclock_ms()`, read at whatever depth of the call stack happened to need it.
        app.player.set_now(fr.now);
        // dev: /tmp/plxnative-autoplay auto-presses OK once
        //
        // **Never from the sign-in or the picker.** The auth flow hands its credentials to
        // the main thread through `take_ready`, which is polled only on those two routes; a
        // trigger that jumped to the player from the picker left a HALF-seated profile —
        // the worker had re-keyed the registry, but the session was never persisted and
        // `apply_pending` stayed set — so the next boot came up as the previous profile
        // and the offline-pick harness case found no cache record (device, 2026-09-06).
        // Waiting for the handoff costs a headless run the seconds the seating takes.
        app.rec.content_begin();
        // The `results` phase is four steps (scenarios, playback pump, clock/press, result
        // landing); three of them carry their own FRAMEDROP spans (`scen`, `pump`, `land`), so a
        // slow `results=` names which held the frame.
        let scenarios_ok = plx_base::diag::spans::span("scen", || {
            if app.boot_initial.is_some() {
                crate::dev::scenarios::controlled_each_frame(app, fr)
            } else {
                unsafe { crate::dev::scenarios::each_frame(app, fr) }
            }
        });
        if !scenarios_ok {
            // This iteration presents nothing, so its spans belong to no FRAMEDROP line.
            let _ = plx_base::diag::spans::take();
            continue;
        }
        plx_base::diag::spans::span("pump", || unsafe { playback_tick(app, fr) });
        clock_and_press(app, fr);
        plx_base::diag::spans::span("land", || unsafe { land_results(app, fr) });
        app.instr.mark(plx_base::diag::heartbeat::Phase::Results); // results
        // **Was the player the page on top going INTO this frame?** The container's own commit
        // may take it off during `bridge::frame_with_tap` below, and the detail page it uncovers
        // has to be told to reveal the episode that played. This used to be a disagreement between
        // the tree and the loop's route mirror ("the tree still says player, the route already
        // says home"); with one authority it is an EDGE, read either side of the frame.
        let was_player = super::bridge::player(&app.pages).is_some();
        // **Publish the frame's inputs into the tree** (spec §2.3) before the dispatcher's frame and
        // the draw, so every owned screen in one frame reads one consistent picture: the playback
        // session and the calendar date, see `publish_frame`.
        if app.adapters.player.poll_repair(&mut app.player.repair) {
            plx_machine::idle::invalidate();
        }
        // A timed-out native Load parked by teardown is released here, on the main thread, the
        // first frame after its media thread returns (`player::engine::AbandonedLoad`).
        plx_media::player::engine::reap_abandoned_load(&mut app.adapters.player);
        // …and a real stop's off-frame `Unload` gets its main-thread tail (gate, epoch, ACB,
        // D1) the first frame after it returns (`player::engine::PendingRelease`).
        plx_media::player::engine::poll_release_frame(&mut app.player.session, &mut app.adapters.player);
        app.player.session.repair_status = app.player.repair.state();
        publish_frame(&mut app.bridge, &app.player.session, was_player);
        // The container runs its frame: the pending navigation's commit (at `PageDip`'s floor),
        // then the owned screens' inputs, ticks, timers and effects. `app/bridge.rs` is the seam.
        let supplied = match app.rec.replay_results(|id| app.bridge.recorded_client(id)) {
            Ok(results) => results,
            Err(reason) => {
                plx_base::eventlog::log(&format!("replay: REFUSED — {reason}"));
                app.running = false;
                break;
            }
        };
        let tick = plx_machine::machine::Tick { ms: fr.now, dt_us: (fr.dt * 1_000_000.0) as u32 };
        if first_controlled_frame { plx_base::eventlog::log(&format!("bootstrap: pre-dispatch dt={} transition={:?} alpha={} flight={}",
            tick.dt_us, app.pages.nav.tabs.stack.transition.commit_point(),
            app.pages.nav.tabs.stack.transition.page_alpha(), app.pages.nav.tabs.stack.transition.in_flight())); }
        app.rec.prepare_resources(&mut app.bridge);
        let (_word, tree_report) = if let Some(results) = supplied {
            let mut stores = std::collections::BTreeSet::new();
            for (address, _) in &results {
                if let plx_machine::machine::MachineId::Store(ord) = address.to {
                    stores.insert(ord.0);
                }
            }
            for ord in stores {
                app.bridge.landgate().landed(plx_machine::machine::StoreOrd(ord));
            }
            super::bridge::frame_with_results(&mut app.pages, &mut app.bridge, tick,
                std::mem::take(&mut app.inputs), || {
                    // One consumed batch per supported store, exactly as the live suppliers
                    // stamp it. No mailbox is polled and no worker is needed for these facts.
                    results
                }, &mut app.rec)
        } else { super::bridge::frame_with_tap(
            &mut app.pages,
            &mut app.bridge,
            plx_machine::machine::Tick {
                ms: fr.now,
                dt_us: (fr.dt * 1_000_000.0) as u32,
            },
            std::mem::take(&mut app.inputs),
            &mut app.rec,
        ) };
        if first_controlled_frame { plx_base::eventlog::log(&format!("bootstrap: post-dispatch alpha={} flight={}",
            app.pages.nav.tabs.stack.transition.page_alpha(), app.pages.nav.tabs.stack.transition.in_flight())); }
        app.rec.resource_requests(app.bridge.take_resource_requests());
        if let Some(reason) = app.rec.failure().or_else(|| app.bridge.controlled_failure()) {
            plx_base::eventlog::log(&format!("replay: REFUSED — {reason}"));
            app.running = false;
            break;
        }
        // **The OWNED pages' share of the underlay verdict.** The three legacy `|=` sites below
        // report the motion of the screens this loop still steps itself; this is the same term
        // for the screens the dispatcher steps, and it was DROPPED for the whole of phase 8 —
        // `frame_with_tap`'s report was bound to `_report` and thrown away. The dispatcher keeps
        // the verdict on its own ledger (`Present::page_moving`, scoped `Page` by default and
        // `Surface` around every modal tick and surface step — §4.4), so it is exactly the term
        // the legacy loop got from stepping `library::update` inside `idle::scoped_motion`.
        //
        // Its host-cache reader is `popover::host::begin_frame` below — `host_refresh`'s `page_moving`
        // term, which decides whether the frozen snapshot under a FADING panel is taken again,
        // and which could not see an owned page move. (It fed `gfx::page_wash_dither` too until
        // 2026-09-19; a page-wide verdict turned out to be the wrong question for a wash, since
        // the wash's own dissolve is in it, and the function itself is gone now — every gradient
        // wash dithers on every frame.)
        fr.underlay_moving |= tree_report.underlay_moving;
        if was_player && super::bridge::player(&app.pages).is_none() { restore_played_entry(app); }
        content_requests(app, fr);
        crate::dev::scenarios::advance_content_boot(app, fr);
        loop_requests(app);
        app.instr.mark(plx_base::diag::heartbeat::Phase::NavCommit); // navcommit
        update(app, fr);
        app.rec.content_results();
        app.instr.mark(plx_base::diag::heartbeat::Phase::TickDrain); // tick_drain
        plx_platform::tv::window::frame_probe_prepare_begin();
        prepare_window(app, fr);
        plx_platform::tv::window::frame_probe_prepare_end();
        present_and_swap(
            app,
            fr,
            #[cfg(all(feature = "hostsim", target_os = "linux"))]
            wslg_frame_budget,
        );
        if fr.present { watchdog.presented(); }
        report(app, fr);
        heartbeat(app, fr);
    }
}

/// **What the loop publishes into the Bridge once per iteration**, ahead of the dispatcher frame and
/// the draw: the playback session ([`super::bridge::Bridge::publish_playback`]) and today's
/// calendar date ([`super::bridge::Bridge::publish_today`]). The date comes from [`clock::today`],
/// which this call is the one caller of: it answers [`clock::REPLAY_DATE`] under a replay (the
/// recorded tick is set at the top of the iteration) and the platform's date, read at most once a
/// minute, otherwise. A screen then receives the date through `AppViews::today` and reads no clock.
fn publish_frame(bridge: &mut super::bridge::Bridge, session: &plx_media::route::PlaybackSession, was_player: bool) {
    bridge.publish_playback(session, was_player);
    bridge.publish_today(clock::today());
}

/// **The frame's clock and the click's spring** (spec §3.3 step 3's head).
///
/// `dt` is clamped at 50 ms: a frame that took longer than that is a stall, and integrating one
/// as if it were real time teleports every spring. It is stamped into `plx_machine::idle::frame_begin`
/// BEFORE the update phase re-steps anything, so the motion flag the gate reads at the bottom
/// describes THIS frame and a spring's velocity can be judged as travel-this-frame rather than as
/// a bare units-per-second.
fn clock_and_press(app: &mut App, fr: &mut Frame) {
    if app.boot_initial.is_some() && app.prev == 0 {
        plx_base::eventlog::log(&format!("bootstrap: first-loop tree={:016x} now={}", app.pages.state_hash(), fr.now));
    }
    fr.dt = {
        let mut d = if app.prev != 0 {
            fr.now.wrapping_sub(app.prev) as f32 / 1000.0
        } else {
            0.016
        };
        if d > 0.05 {
            d = 0.05;
        }
        d
    };
    app.prev = fr.now;
    app.rec.tick(fr.now, fr.dt);
    // Whole-frame present gate (`plx_machine::idle`): forget last frame's motion BEFORE the update
    // phase below re-steps every spring, so the flag it leaves describes THIS frame, and
    // stamp `dt` so a spring's velocity can be judged as travel-this-frame rather than as
    // a bare units-per-second. The decision itself is taken just above `glViewport`.
    // A dump's held repeat judges rest with its first pass's `dt` (`framedump::idle_dt`); the
    // clock `now_ms` reads still advances by `fr.dt` alone.
    #[cfg(all(feature = "hostsim", feature = "devtriggers"))]
    let rest_dt = if crate::dev::framedump::active() { crate::dev::framedump::idle_dt(fr.dt) } else { fr.dt };
    #[cfg(not(all(feature = "hostsim", feature = "devtriggers")))]
    let rest_dt = fr.dt;
    plx_machine::idle::frame_begin_judging_rest_with(fr.dt, rest_dt);
    // Is a page capture still in flight on the GPU, and is recorded text still warming? Latched
    // once, before the springs step, so the held appear spring and the present gate below read
    // the same answer — and, being the machine's speed rather than the inputs, recorded or
    // supplied by the recorder like any other environmental observation.
    let text = plx_gfx::text::prewarm_pending();
    plx_gfx::gfx::snapshot_frame_begin(|snapshot| {
        let seen = app.rec.capture_readiness(plx_ui::rec::Readiness { snapshot, text });
        plx_gfx::text::latch_surface_text_pending(seen.text);
        seen.snapshot
    });
    // ui::press (tvOS click) — advance the dip/spring every frame; when a deferred activation
    // commits (the spring-back bounce has played), run it for whichever CARD view armed the
    // press. A long-press does NOT commit (`press::tick` clears `want_commit` at `LONG_MS`):
    // on Home it opens the item menu below, and anywhere else it just springs back.
    let (_, press_moving) = plx_machine::idle::scoped_motion(|| {
        app.input.press.tick(fr.now, fr.dt);
    });
    // The motion of whatever page is UNDER a popover — Home, the Library or Search, since
    // the profile chip is a stop on all three. It was `home_underlay_moving` while only Home
    // could be underneath. The account popover's glass re-snapshots off this.
    fr.underlay_moving = press_moving;
}

/// **Make sure the player has a compiled HUD**, on each player frame in `prepare_window` (inside the frame
/// iteration, before the draw): the app's layer is where the current locale is read
/// (`plx_platform::i18n::current()`), and the compile gets it as an argument. The compile runs once per
/// process: [`boot_hud`] does it at boot, and this does it on the first player frame when boot did not (a
/// controlled replay or record run) and for the `plxnative-hud=<path>` package, which is read and compiled
/// on that frame too, so THAT frame pays the compile. A language change does not recompile: the language is
/// fixed for the process (`i18n`'s `CURRENT` is a `OnceLock`; a changed setting is the "confirmed next-launch"
/// one) and takes effect at the next launch. Every compile is handed to the mounter ([`adopt_hud`]), so the next
/// player instance is constructed with the current one. `builtin:classic` always loads, so after this
/// `app.hud` is `Some`.
fn refresh_hud(app: &mut App) {
    use plx_appkit::player_hud::loaded::{DevHud, DevStep};
    let cx = plx_platform::i18n::current();
    // `plxnative-hud=<path>`: a package from a file replaces classic while it loads. Read once; a file that
    // does not load reports one line and leaves classic to draw.
    if matches!(app.dev_hud, DevHud::Unread) {
        app.dev_hud = crate::dev::scenarios::hud_trigger();
    }
    if let DevHud::Package(dev) = &mut app.dev_hud {
        match dev.refresh(plx_plex::plex::identity::version(), cx, app.bridge.measure()) {
            DevStep::Report(_line) => {
                #[cfg(feature = "devtriggers")]
                plx_base::eventlog::log(&format!("plxnative-hud: {_line}"));
            }
            DevStep::Compiled => {
                #[cfg(feature = "devtriggers")]
                plx_base::eventlog::log(&format!("plxnative-hud: compiled for {}", cx.language().tag()));
            }
            DevStep::Ready | DevStep::Quiet => {}
        }
        if let Some(hud) = dev.hud() {
            if app.hud.as_ref().is_none_or(|h| !std::sync::Arc::ptr_eq(&h.compiled, &hud.compiled)) {
                let hud = hud.clone();
                adopt_hud(&mut app.bridge, &mut app.hud, hud);
            }
            return;
        }
    }
    if app.hud.is_none() {
        let hud = plx_appkit::player_hud::loaded::load_classic(plx_plex::plex::identity::version(), cx, app.bridge.measure());
        adopt_hud(&mut app.bridge, &mut app.hud, hud);
    }
}

/// **The app's HUD at construction, compiled outside any frame**: bridge's measurer and the viewer's
/// language, taken into use ([`adopt_hud`]) so the first player page is constructed with it. Boot and the
/// host test rig both build their `App` with it. A controlled replay or record run compiles nothing here:
/// its measurer answers only the keys the recording holds, and measuring before the recorder's resources
/// exist latches a failure; no fixture reaches the player, and [`refresh_hud`] compiles on the first player
/// frame there.
pub(super) fn boot_hud(bridge: &mut super::bridge::Bridge, controlled: bool) -> Option<plx_appkit::player_hud::loaded::LoadedHud> {
    if controlled {
        return None;
    }
    let loaded = plx_appkit::player_hud::loaded::load_classic(plx_plex::plex::identity::version(), plx_platform::i18n::current(), bridge.measure());
    let mut hud = None;
    adopt_hud(bridge, &mut hud, loaded);
    hud
}

/// **The one place the app takes a compile into use**: keeps it as the app's current HUD and hands it to the
/// mounter (`Bridge::seed_player_compiled`), so the next player page is constructed with it. Boot and
/// [`refresh_hud`] (a controlled run's first player frame, the dev package) both come through here.
pub(super) fn adopt_hud(
    bridge: &mut super::bridge::Bridge,
    slot: &mut Option<plx_appkit::player_hud::loaded::LoadedHud>,
    hud: plx_appkit::player_hud::loaded::LoadedHud,
) {
    bridge.seed_player_compiled(std::sync::Arc::clone(&hud.compiled));
    *slot = Some(hud);
}

/// **The prepare window** (spec §3.3 steps 8-9, §8.1): the frame budget's own frame, the poster
/// adapter's, the player page's clock-motion report, the whole-frame present DECISION, and — on
/// the presenting side of it — the render cache's upload.
unsafe fn prepare_window(app: &mut App, fr: &mut Frame) {
    // ---- the PREPARE WINDOW opens here (spec §3.3 steps 8-9, §8.1) -------------------
    //
    // **The budget's frame opens at the start of this window, not at the top of the
    // iteration**, and that placement is the whole of its ceiling's meaning. `take` admits
    // while `now - frame_start + worst_us <= PREPARE_MAX_US`; opened at the loop top, the
    // elapsed term already contained ingest, the result landings, the nav commit and the
    // tick drain, so on a cold-open frame (~62 ms) every upload was over the ceiling and
    // only the forward-progress escape let ONE through. A quota of three that was really a
    // quota of one, on exactly the frames with the most textures waiting.
    app.pages.budget.begin_frame(plx_base::diag::heartbeat::now_us());
    // The poster adapter's frame (spec §3.3 step 3's tail): a new frame for the slot LRU and
    // every decoded image handed to the render cache as owned pixels. No GL here — the
    // upload is below, on the presenting side of the decision.
    super::adapters::poster::begin_frame();
    super::adapters::poster::drain_decoded();

    fr.player = matches!(app.route(), AppArg::Player);
    // ---- the player page's per-frame HUD SNAPSHOT (`HudModel`) ----
    //
    // Sampled ONCE here and pushed into the screen as a value, so nothing the page prepares or
    // draws reads a process global (`docs/sans-io.md`). Here, and not at the top of the iteration,
    // because it must see everything the frame has changed: the pump (`playback_tick`) has
    // republished the playhead, the seek target and the pipeline state, and the dispatcher's frame
    // and `update` have run the screen's own `publish` — `TX.scrub_ns`, which the scrub preview
    // draws, is written by the page's `Tick` and its keys, and a snapshot taken before them would
    // draw it a frame late. Ahead of everything that reads the model: the clock fingerprint just
    // below (the present gate's input) and, on a presenting frame, the draw (`draw`). The
    // dispatcher's own `prepare` pass runs earlier, inside `bridge::frame_with_tap`, and so reads
    // the PREVIOUS frame's snapshot exactly as it reads the previous frame's `busy` and `transport` —
    // except `scrub_ns`, which the page's `prepare` takes from its own scrub (a held scrub moves the
    // target by seconds a frame, and the clock prewarm warms only the displayed second and the next).
    if fr.player {
        let model = hud_model(&app.player.session, app.bridge.metadata_view());
        refresh_hud(app);
        if let Some(player) = super::bridge::player_mut(&mut app.pages) {
            player.model = model;
            if let Some(hud) = app.hud.as_ref() {
                if !std::sync::Arc::ptr_eq(&player.compiled, &hud.compiled) {
                    player.compiled = std::sync::Arc::clone(&hud.compiled);
                }
            }
        }
    }
    // ---- the player page's CLOCK-DRIVEN motion report (spec §8.3, §9) ----
    //
    // After the update phase, so this frame's springs and this frame's pump have been seen,
    // and before the gate below reads the verdict. Everything the player page draws from a
    // clock rather than from a spring is folded into ONE value and compared with last frame's;
    // `PlayerScreen::clock_fingerprint` is the inventory and the argument. Nothing to do off
    // the route — the page is not mounted and nothing it owns is on screen.
    if fr.player {
        let fp = super::bridge::player(&app.pages)
            .map(|p| p.clock_fingerprint(&app.player.session, fr.now));
        if let Some(fp) = fp {
            if app.player.note_clock(fp) {
                plx_machine::idle::invalidate();
            }
        }
    }
    // ---- whole-frame present gate (`plx_machine::idle`) --------------------------------------
    // A screen with nothing moving on it does not need to be re-sent to the panel. This
    // skips `glViewport`…`SDL_GL_SwapWindow` WHOLESALE — it is not dirty-RECTANGLE
    // tracking, which `plx_ui`'s `lib.rs` renderer doc rejects: when this says yes, the frame
    // below is byte-for-byte the immediate-mode frame it always was, clear and all.
    //
    // Measured cost of not doing this (2026-07-31): a still Home grid burns 16.0% of one
    // A53 core here plus ~19.4 points inside `surface-manager`, which must blend our
    // 1080p surface on every present — a charge that measured identical on three
    // different screens, so it is per-PRESENT, not per-pixel. ~35 points of a core to
    // re-send an unchanged picture, on a fan-less SoC that sits on Home for hours.
    //
    // **The exclusion is the BOUND PLANE, not the player ROUTE** (spec §9, phase 9).
    // `system.rs::clear_opaque_region` documents the hardware video plane as *slaved* to this
    // wayland surface, and "we stop presenting while a plane is slaved to it" is a claim about
    // this compositor that reading cannot settle — so while the plane is bound the gate
    // answers `true` unconditionally, and that term now lives INSIDE `should_present`, fed by
    // `Player::set_video_plane_bound`'s edges and by nothing else.
    //
    // What that changes is the frames on either side of a playback. Pre-bind (the spinner
    // while the route resolves and the Load is in flight) and post-unbind (the HUD fading out,
    // the failure read-out) are ORDINARY idle frames: nothing is slaved to the surface, so the
    // 35-points-of-a-core argument applies to them exactly as it does to Home — and every
    // player-side animator that advances from a clock has to report, or it freezes. Playback
    // itself still spends ~99% of its time with the HUD auto-hidden, where the frame is
    // already 0 draw calls.
    //
    // **The foreground demand is taken ONCE and has TWO terms** (spec §3.3 step 8): the
    // gate above, and whether the frame budget holds queued prepare work. The second term is what makes
    // the upload step below legal on the presenting side of the decision — a texture waiting
    // to be uploaded is itself a reason to present, so nothing sits in the queue behind a
    // settled screen. It was inert before phase 11: nothing in the product ever published a
    // queue to the budget, so `has_queued_work()` was permanently false and the upload had to
    // run BEFORE the decision (and invalidate) to happen at all.
    plx_ui::tex::note_queued(&mut app.pages.budget);
    // Lifecycle is a hard outer gate: even noidle, a bound plane or queued uploads must
    // not reach EGL/Mali while SDL's window is backgrounded.
    //
    // A third term goes FIRST and short-circuits the other two: a page capture still in flight on
    // the GPU (`gfx::SNAPSHOT_THIS_FRAME`'s doc). Presenting now would only wait a whole vsync for
    // a buffer; not asking `should_present` leaves its damage and motion for the frame that does.
    // The simulator's settled capture (`PLXNATIVE_SHOT_SETTLE`): once nothing has changed for the
    // asked-for quiet, invalidate so the NEXT present is the settled frame and `maybe_capture`
    // takes it. Before the decision below, so that invalidate selects this very frame.
    #[cfg(feature = "hostsim")]
    crate::shot::tick(fr.now, app.pages.budget.has_queued_work() || plx_gfx::gfx::snapshot_pending()
        || app.scenarios.shots.pending());
    fr.present = !plx_gfx::gfx::snapshot_pending()
        && app.window_activity.allow_present(
            plx_machine::idle::should_present(fr.now) || app.pages.budget.has_queued_work(),
        );
    // Read-only: was the fence the only thing standing between this iteration and a present? An
    // inactive window would not have presented either way. (`should_present` is deliberately not
    // asked: it takes-and-clears the damage the deferred frame still needs.)
    fr.deferred = !fr.present && plx_gfx::gfx::snapshot_pending() && app.window_activity.allow_present(true);
    // A dump draws every iteration: a held repeat and a written frame alike are a draw.
    #[cfg(all(feature = "hostsim", feature = "devtriggers"))]
    if crate::dev::framedump::active() {
        fr.present = true;
    }
    app.rec.present(fr.present);
    if fr.present {
        app.window_activity.begin_present(fr.player);
    }
    // ---- step 9's UPLOAD, on the presenting side of the decision ---------------------
    //
    // The render cache's upload step under the frame budget (§3.3 step 9, §10: "a frame that
    // does not present uploads nothing"). Two placement rules, both load-bearing:
    //
    // * **Only on a presenting frame.** The spec's rule: a frame that is skipped wholesale
    //   uploads nothing and the queue waits (`GfxUploader::warm` is `gfx::warm_tex`, which
    //   also draws, so it wants the frame's GL state in place).
    // * **Before the draw.** `warm_tex` draws into its own tiny offscreen target and never
    //   touches framebuffer 0 (the first framebuffer-0 draw of a frame is where the back-buffer
    //   wait lands, and it must stay with the page's `frame_clear`, not with this window), so
    //   there is no pixel to show over the picture — film included — wherever it runs; it stays
    //   here so the upload's cost is bounded by the budget and off the draw.
    if fr.present {
        // A Tracks/More page's text, recorded in `update` or, on the mount frame, in the overlay's
        // `prepare` (`PanelMotion::prewarm_text`), and uploaded here: `fr.present` already carries the window-activity gate, so a frame that
        // does not present, or one while the window is backgrounded, uploads nothing.
        plx_ui::panel_motion::PanelMotion::drain_queued_text(plx_base::diag::heartbeat::now_us);
        let mut ph = plx_machine::machine::PresentHandle::of(&mut app.present);
        super::adapters::poster::prepare(
            &mut app.pages.budget,
            &mut ph,
            plx_base::diag::heartbeat::now_us,
        );
    }
    // EXPERIMENT (`/tmp/plxnative-opaque`): one `static` read and a return when the trigger
    // is absent. Edge-triggered — see `system.rs`.
    //
    // Called on EVERY frame, from the BIT rather than from the route. The false edge after an
    // unbind may land on a frame that would otherwise not present (spec §3.3 step 9), and
    // there is nothing else in the loop that would carry it: a `return` above, or a term that
    // only ran on player frames, would leave the compositor believing our surface is still
    // opaque with an ordinary UI on it.
    plx_platform::tv::window::opaque_route(app.player.video_plane_bound);
    app.instr.mark(plx_base::diag::heartbeat::Phase::Prepare); // prepare
    // `worstprep=`: the prepare phase is timed on EVERY iteration, presented or not — a
    // settled screen must never run untimed work at the loop rate (spec §8.3).
    app.instr.note_prepare();
    // Hoisted: the frame-drop detector reads these after the gate. Seeded to the pump
    // stamp so a skipped frame reports zero draw/cap/swap rather than a stale delta.
    app.instr.seed_present_phases();
}

/// **The player HUD's [`HudModel`] for this frame**: every process global its presentation reads,
/// sampled once. Each field is the expression its own doc in `appkit/player_hud/model.rs` names;
/// `bitmap_by_key` is the media function `draw_subtitle_bitmap` fetches a cue's rects with, injected
/// so appkit never names the player's globals. Allocation-free: a dozen atomic loads, plus the image
/// cue lookup (`active_bitmap_key`, one lock only while an image subtitle track is selected).
///
/// The playhead is read once and the cue lookup takes that same sample, so the two cannot straddle a
/// media-thread publication. App-side by construction: the app crate is where globals are read, the
/// screen and the draw receive the value (`prepare_window` is the one caller).
fn hud_model(
    ps: &plx_media::route::PlaybackSession,
    meta: plx_data::metadata::MetadataView<'_>,
) -> plx_appkit::player_hud::model::HudModel {
    let playpos_ns = plx_media::player::playpos_ns();
    plx_appkit::player_hud::model::HudModel {
        paused: plx_media::player::TX.paused.load(Relaxed),
        scrub_ns: plx_media::player::TX.scrub_ns.load(Relaxed),
        playpos_ns,
        seek_display_ns: plx_media::player::seek_display_ns(),
        duration_ns: plx_media::player::duration_ns(),
        loading: plx_media::player::loading(ps),
        sub_size: plx_media::route::subtitle_size(),
        sub_position: plx_media::route::subtitle_position(),
        sub_tone: plx_media::player::subtitle_tone(),
        sub_sel: plx_media::player::desired_sub_idx(),
        controls: plx_appkit::player_hud::Controls::now(ps),
        chapters: plx_appkit::chapters_panel::has_chapters(meta),
        detail: plx_appkit::info_panel::detail_rk(meta).is_some(),
        skip_secs: u32::try_from(plx_media::route::skip_interval().ns() / 1_000_000_000).unwrap_or(10),
        stats: crate::app::diagnostics::enabled(),
        bitmap_key: plx_media::player::active_bitmap_key(playpos_ns),
        bitmap_by_key: plx_media::player::bitmap_by_key,
    }
}

/// **The `Rig`-delegated privileged primitives** (spec §15.2, D4). `Bridge`'s `Rig` impl
/// (`app/bridge.rs`) cannot itself move here: a trait's implementation for a type is one
/// syntactic unit, and that `impl Rig<AppHost> for Bridge` block carries two dozen other methods
/// beside these three — so this is the "or just its three privileged methods" half of the D4
/// move. `Bridge::opaque_route`/`Bridge::clear_opaque_region` call these instead of
/// `plx_platform::tv::window::` directly, which is what keeps the OS-facing text — the thing the `frame`
/// gate greps for — in this ONE file rather than split across the loop and the bridge. Each is a
/// pure pass-through: the loop's own per-frame call above (`plx_platform::tv::window::opaque_route`, from
/// `fr`) and the dispatcher's `Rig` hook (from `Bridge`'s own copy of the same bit) are two
/// independent callers of one primitive, not two implementations of it. `ls2_pump` needs no
/// twin here — `Bridge::ls2_pump` is or stays a no-op, since the dispatcher does not yet run a
/// phase this early in the frame.
pub(crate) fn rig_opaque_route(video_plane_bound: bool) {
    plx_platform::tv::window::opaque_route(video_plane_bound);
}

/// See [`rig_opaque_route`]. The `if self.video_plane` guard stays on `Bridge`'s side — this is
/// the OS call alone, exactly what `plx_platform::tv::window::clear_opaque_region` was before the move.
pub(crate) fn rig_clear_opaque_region() {
    plx_platform::tv::window::clear_opaque_region();
}

/// One idle iteration's font-warming slice: a key pressed during it waits at most this long more
/// than the idle poll already makes it wait.
const FONT_WARM_SLICE_US: u64 = 2_000;

/// **Draw, capture, swap — or sleep one frame period** (spec §3.3 step 10). Everything in here is
/// inside the present gate's decision, which `prepare_window` has already taken into `fr.present`.
unsafe fn present_and_swap(
    app: &mut App,
    fr: &mut Frame,
    #[cfg(all(feature = "hostsim", target_os = "linux"))]
    wslg_frame_budget: Option<super::window_activity::WslgFrameBudget>,
) {
    if fr.present {
        // The dump's full prewarm drain and pre-draw debt sample, after the takes and before the draw.
        #[cfg(all(feature = "hostsim", feature = "devtriggers"))]
        if crate::dev::framedump::active() {
            crate::dev::framedump::pre_draw(app);
        }
        // the glyph cache's frame serial (phase 11, text.rs's hot window): a drawn frame
        plx_gfx::text::begin_frame();
        // A finished underlay-field reduction is read HERE, before framebuffer 0 holds anything
        // of this frame — a read between the page and the surfaces splits its render pass.
        plx_gfx::gfx::field_frame_begin();
        let (_vx, _vy, _vw, _vh) = draw(app, fr);
        app.instr.mark(plx_base::diag::heartbeat::Phase::Draw); // draw
        // dev capture stream: grab this finished frame before the swap (after the last draw,
        // so the copy's pass-flush is work the swap would submit anyway). One atomic when idle.
        // Deliberately NOT while the video plane is BOUND (the UI plane is transparent over
        // video, so there is nothing to grab) — capture.rs's 5s keepalive resend covers the
        // host's deadness timer while playback is up. Keyed on the bit rather than the route
        // since phase 9: a pre-bind spinner and a post-unbind read-out ARE ordinary UI-plane
        // pictures, and the operator watching a stream had no reason to lose them.
        if !app.player.video_plane_bound {
            crate::capture::tick(fr.now);
        }
        app.instr.mark(plx_base::diag::heartbeat::Phase::Capture); // capture
        // `plxnative-simvideo`: the decoded picture goes UNDER the finished UI, as the television's
        // compositor puts its video plane under ours — before the capture, so shots include it.
        #[cfg(feature = "hostsim")]
        if fr.player {
            plx_media::player::sim_video::composite_under();
        }
        // Before the swap, never after: the back buffer is undefined once presented.
        // A dump decides here whether this frame is written (`dev::framedump`); otherwise the
        // ordinary screenshot seam.
        #[cfg(all(feature = "hostsim", feature = "devtriggers"))]
        let dumping = crate::dev::framedump::active();
        #[cfg(all(feature = "hostsim", feature = "devtriggers"))]
        if dumping && crate::dev::framedump::after_draw(app, _vx, _vy, _vw, _vh) {
            app.running = false;
        }
        #[cfg(all(feature = "hostsim", feature = "devtriggers"))]
        let screenshot_seam = !dumping;
        #[cfg(all(feature = "hostsim", not(feature = "devtriggers")))]
        let screenshot_seam = true;
        #[cfg(feature = "hostsim")]
        if screenshot_seam && crate::shot::maybe_capture(_vx, _vy, _vw, _vh) {
            // The headless one-shot is done: finish this frame, then leave through the ordinary
            // shutdown rather than exiting from inside the frame (see `shot::maybe_capture`).
            app.running = false;
        }
        {
            #[cfg(feature = "threadcheck")]
            let _present_scope = plx_base::task::watchdog::present_scope();
            #[cfg(feature = "hostsim")]
            {
                // `present_supersampled` disables the scissor test itself (a clip left armed would
                // crop the window copy) - below gfx's `LIVE_SCISSOR`, which it cannot name. Clearing
                // the clip through gfx first leaves the two in step for the next frame.
                plx_gfx::gfx::clip_clear();
                plx_base::surface::present_supersampled();
            }
            // dev (`/tmp/plxnative-framecb`): this frame's compositor callback, requested before
            // the swap that commits it. One latched bool unarmed.
            plx_platform::tv::window::frame_probe_request();
            SDL_GL_SwapWindow(app.win);
        }
        app.window_activity.presented(fr.player);
        // dev diagnostic, OFF unless `/tmp/plxnative-eglage` was present at boot: re-ask EGL for
        // the back buffer's AGE after real presents have happened (the boot reading is 0 by
        // construction). One null-handle branch per present when unarmed; armed, its one query
        // blocks that present's swap 13-16 ms on the T820 (2026-10-24). See `egl.rs`.
        plx_gfx::egl::late_probe();
        #[cfg(feature = "devtools")]
        {
            app.buffer_flip_count = (app.buffer_flip_count + 1) % 60;
        }
        app.instr.mark(plx_base::diag::heartbeat::Phase::Swap); // swap
        // Inside the gate: `frame_end` is the end of a DRAWN frame. Counting frames the
        // idle gate skipped would pace the profiler's once-per-N-frames log off frames
        // that ran no phases at all.
        plx_ui::profile::frame_end();
        plx_gfx::overdraw::frame_end();
        // Same reason, same gate: the blur's region accounting is per DRAWN frame. It rolls
        // "what every glass surface asked for this frame" into the region the next frame's
        // first snapshot is taken at. Once that union is known, several surfaces share one
        // capture; a first discovery frame may still need a second non-contained grab.
        plx_gfx::gfx::blur_frame_end();
        app.glass.sources.borrow_mut().finish();
        // …and a queued underlay-field reduction has had one more drawn frame to finish in.
        plx_gfx::gfx::field_frame_end();
        // …and a region texture no page asked for this frame goes back to the driver.
        plx_gfx::gfx::region_frame_end();
        // …and a frame that captured the page leaves a fence the next frames wait on.
        plx_gfx::gfx::snapshot_frame_end();
        // …and a ground probe's queued copy is read back here, between frames, if it is done.
        plx_gfx::gfx::ground_probes_frame_end();
        plx_machine::idle::note_present(fr.now);
        // The swap has returned: this frame's interval, for the visit's slow and frozen counts.
        app.instr.frame_presented();
        // The poster-gate scenes' frame counter, at the same post-swap seam as the present count
        // above. It was called from inside `note_present`; `plx_machine::idle` is the machine layer and
        // no longer names `ui`'s metrics.
        #[cfg(feature = "devtriggers")]
        plx_ui::card_motion_metrics::presented(fr.now);
        #[cfg(all(feature = "hostsim", target_os = "linux"))]
        if let Some(budget) = wslg_frame_budget {
            budget.finish();
        }
    } else {
        if fr.deferred {
            // The fence held a present back: the panel kept the old picture for this iteration, so
            // the next presented frame's interval must span it. An idle gap breaks the cadence; a
            // deferral is part of it.
            app.instr.defer_present_phases();
        } else {
            app.instr.skip_present_phases();
        }
        // The one stretch of main-thread time nothing is waiting on: spend a bounded slice of it
        // opening the theme faces and loading their glyph metrics (`text::warm_fonts_idle`), so
        // a page's first layout does not pay for them inside a transition. A no-op once warm.
        plx_gfx::text::warm_fonts_idle(FONT_WARM_SLICE_US, plx_base::diag::heartbeat::now_us);
        // Device and macOS presented frames block in swap; WSLg/X11 presented frames use the
        // software budget above. A skipped frame reaches neither path, so sleep here to keep a
        // settled screen from becoming a CPU spinner.
        SDL_Delay(plx_machine::idle::IDLE_POLL_MS);
    }
}

/// Ingest: the lab command channel, the remote FIFO and the SDL event queue (spec §3.3 step 2).
/// Every key, pointer, text and lifecycle event the frame acts on enters here.
pub(crate) unsafe fn ingest(app: &mut App, fr: &mut Frame) {
        let supplied = controlled_replay(app);
        // Cloud Test Lab has no SSH/FIFO. Its LAB build long-polls outward, then leaves each
        // command here for the SDL thread so the same dispatcher and event queue remain the
        // only input path. Acknowledge acceptance after dispatch, before polling SDL below.
        for command in crate::lab::take_commands() {
            let ok = !supplied && ingress_token(app, fr, &command.token);
            crate::lab::command_done(command.id, ok);
        }
        // The FIFO's tokens are collected first so the recorder (a field beside `remote`) can
        // be written per token; a token that pushes SDL events is recorded where they are
        // polled, a DIRECT one (`pat:`, `shot`, `txt:`) here — `token_is_direct` is the split.
        let mut toks: Vec<String> = Vec::new();
        if let Some(r) = app.remote.as_mut() {
            r.drain(|tok| toks.push(tok.to_string()));
        }
        for tok in toks {
            if !supplied { let _ = ingress_token(app, fr, &tok); }
        }
        drain_sdl(app, fr);
}

unsafe fn drain_sdl(app: &mut App, fr: &mut Frame) {
    while SDL_PollEvent(app.ev.as_mut_ptr() as *mut c_void) != 0 {
        ingest_sdl_event(app, fr);
    }
}

unsafe fn ingress_token(app: &mut App, fr: &mut Frame, token: &str) -> bool {
    #[cfg(feature = "devtriggers")]
    if let Some(probe) = crate::remote::HangProbe::parse(token) {
        // FIFO, LAB and replay all enter here on the frame thread inside FrameScope.
        // Earlier keys/clicks may still be in SDL: handle and record them before the stall.
        drain_sdl(app, fr);
        app.rec.input(super::recorder::enc_token(token));
        probe.run();
        return true;
    }
    // Keys and clicks use SDL's existing synthesis while coexistence lasts. Consume those
    // before a following direct text token, rather than moving all text ahead of all keys.
    if super::bridge::search_owns_input(&app.pages) { drain_sdl(app, fr); }
    if super::bridge::search_owns_input(&app.pages) {
        if let Some(text) = token.strip_prefix("txt:") {
            ingest_text(app, &text.replace('+', " "), crate::textinput::available(),
                plx_machine::machine::Source::RemoteFifo);
            return true;
        }
    }
    let accepted = dispatch_remote_token(token, &app.player.session);
    if accepted && token_is_direct(token) { app.rec.input(super::recorder::enc_token(token)); }
    accepted
}

/// **Pin the transport for a headless run, and park its ring** — the dev triggers' half of what
/// `start_playback` does for a real one.
///
/// One helper rather than three copies of `set_hud(now + HUD_HEADLESS_MS)`: the deadline and the
/// cursor are the player INSTANCE's since restructure phase 9, so every one of those copies would
/// have had to learn the same `player_mut` lookup and the same `publish`. `tab` parks the bottom
/// tab row (the Info card is tab 0, the Chapters strip tab 1) and moves the ring onto it; `None`
/// leaves the cursor alone, which is what the track menu and the auto-pause script want.
pub(crate) fn pin_headless_hud(app: &mut App, now: u32, tab: Option<i32>) {
    if let Some(player) = super::bridge::player_mut(&mut app.pages) {
        player.hud.extend(now, HUD_HEADLESS_MS);
        if let Some(tab) = tab {
            player.hud.nav.focus = GROUP_TABS;
            player.hud.nav.tab = tab;
        }
        player.publish();
    }
    // The cursor moved by the app's own hand; the engine follows it.
    if tab.is_some() {
        super::bridge::reseat_player_focus(&mut app.pages);
    }
}

fn ingest_text(app: &mut App, text: &str, panel: bool, source: plx_machine::machine::Source) {
    if text.is_empty() { return; }
    let at = plx_machine::machine::Tick { ms: clock::now(), dt_us: 0 };
    app.rec.input(super::recorder::enc_text(text, panel, at, source));
    app.inputs.extend(text_inputs(text, panel, at, source));
    app.last_input = at.ms;
    plx_machine::idle::invalidate();
}

/// Revoke the borrowed SDL window handles on background. `release` answers whether a surface was
/// held; only a real release is a crash-report step, since WILL and DID background both arrive.
fn release_borrowed_window(release: impl FnOnce() -> bool) {
    if release() {
        window_step(plx_telemetry::telemetry::window::Stage::WaylandReleased, None);
    }
}

/// One closed lifecycle step on a native crash event (`window::Stage`): consent-gated and a no-op
/// while the crash backend is off. `playing` is the same flag the WILL/DID edges carry.
fn window_step(stage: plx_telemetry::telemetry::window::Stage, playing: Option<bool>) {
    plx_telemetry::telemetry::window::record(
        plx_telemetry::telemetry::window::Observation::step(stage, playing),
    );
}

/// One polled event. Shared by ordinary polling and ordered FIFO/replay ingestion.
unsafe fn ingest_sdl_event(app: &mut App, fr: &mut Frame) {
    ingest_sdl_event_with_window(app, fr, plx_platform::tv::window::grab);
}

/// The window reacquisition is a platform operation, supplied separately so the
/// recorded/live ingress boundary can be exercised without an SDL/GL context.
unsafe fn ingest_sdl_event_with_window(app: &mut App, fr: &mut Frame,
    restore_window: impl FnOnce(*mut c_void)) {
    let et = rd_u32(&app.ev, 0);
    app.window_activity.event(et);
    let playing = matches!(app.route(), AppArg::Player);
    plx_telemetry::telemetry::window::lifecycle(et, playing);
    // Release health: the foreground edge begins a session (the background edge ends it further
    // down, after the player's park), and a key or pointer event is the activity that keeps one
    // open (`telemetry::session`; consent-gated inside, a no-op while crash reports are off).
    if et == 0x106 { plx_telemetry::telemetry::session::lifecycle(et); }
    if is_input_event(et) { plx_telemetry::telemetry::session::input(); }
    if controlled_replay(app) {
        // The tape owns logical ingress; real SDL startup notifications must not be
        // counted a second time. The real compositor still owns window safety.
        // Only its DID foreground reacquires native handles. Recorded notifications
        // below never reacquire them, revoke grants or resume a native player.
        if et == 0x106 { restore_window(app.win); }
        if matches!(et, 0x103 | 0x104 | SDL_QUIT) {
            app.rec.refuse("live window exit interrupted controlled replay");
            app.running = false;
        }
        return;
    }
    // INPUT while a popover holds the page frozen is the POPOVER's: every invalidate
    // such an event raises — the one below, and whatever its handler adds — is
    // attributed to it, so the frozen host is not re-rendered on every key-up
    // (`popover::host::input_scope`). Only input: a lifecycle or window event is the
    // APP's and may change the page under the panel (backgrounding drops Search's
    // editing layout), so its damage stays the page's and the snapshot is retaken.
    let _own_input = if is_input_event(et) {
        plx_ui::popover::host::input_scope()
    } else {
        None
    };
    // ANY event is a reason to repaint (`plx_machine::idle`): a key changes focus or a label,
    // a lifecycle event changes the whole screen. Marked here — once, for every event
    // kind — rather than in each of the ~30 arms below, where the next one added would
    // silently draw nothing.
    plx_machine::idle::invalidate();
    if et == SDL_KEYDOWN {
        // A remote key is a deliberate new interaction and releases any suppression of the
        // pointer gesture that was carried through startup buffering.
        app.ptr.startup_motion_suppressed = false;
    }
    if et == SDL_KEYDOWN
        || et == SDL_KEYUP
        || et == SDL_TEXTINPUT
        || et == SDL_TEXTEDITING
    {
        // 48 bytes, not 32: a TEXTINPUT event's `text[32]` starts at +16 on the
        // television (LG's `inputSource` shifts it), so it ENDS at exactly +48. At the
        // old width the one event whose payload the offsets are most easily wrong
        // about would have left a forensic trail that stopped just before the payload.
        let mut hex = String::with_capacity(96);
        for b in &app.ev[..48] {
            hex.push_str(&format!("{b:02x}"));
        }
        let what = match et {
            SDL_TEXTINPUT => "text",
            // **The IME's PRE-EDIT, and the reason it is logged at all.** The panel's
            // word prediction is a REPLACE — tapping "summer" under a typed "summ"
            // means *delete what I was predicting on, then commit this* — and the app
            // sees only the commit, so the field reads "summsummer" (reported from the
            // couch 2026-08-15). Whether the delete half reaches us as `SDL_TEXTEDITING`
            // (`text_model.delete_surrounding_text` mapped onto SDL's pre-edit) or as
            // nothing at all decides whether the fix can be exact or has to be a
            // heuristic — and this arm is the only way to find out, since nothing in
            // the app has ever read this…108812 tokens truncated…Server {
        fn new() -> Self {
            Self {
                stopped: Arc::new(AtomicBool::new(false)),
                hold_reads: Arc::new(AtomicBool::new(false)),
                hold_stop: Arc::new(AtomicBool::new(false)),
                refuse_stop: Arc::new(AtomicBool::new(false)),
            }
        }

        fn rig(&self, ends: bool) -> Option<Rig> {
            let (stopped, hold, hold_stop) = (self.stopped.clone(), self.hold_reads.clone(), self.hold_stop.clone());
            let refuse = self.refuse_stop.clone();
            Rig::serving_with(move |request: &str| {
                if request.contains("/decision?") {
                    return r#"{"MediaContainer":{"Metadata":[{"Media":[{"Part":[{"decision":"directplay"}]}]}]}}"#.into();
                }
                if request.contains("/:/timeline") {
                    if request.contains("state=stopped") {
                        for _ in 0..5_000 {
                            if !hold_stop.load(Ordering::SeqCst) { break; }
                            std::thread::sleep(Duration::from_millis(2));
                        }
                        if refuse.load(Ordering::SeqCst) {
                            return "!503".into();
                        }
                        stopped.store(true, Ordering::SeqCst);
                    }
                    return r#"{"MediaContainer":{}}"#.into();
                }
                if request.contains("/:/scrobble") {
                    return "{}".into();
                }
                if request.starts_with("GET /library/metadata/") {
                    // Bounded by count, not by a clock: a failing test must not hang.
                    for _ in 0..5_000 {
                        if !hold.load(Ordering::SeqCst) { break; }
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    let seen = stopped.load(Ordering::SeqCst);
                    let facts = match (seen, ends) {
                        (true, true) => r#","viewCount":1"#,
                        (true, false) => r#","viewOffset":150000"#,
                        (false, _) => "",
                    };
                    return format!(r#"{{"MediaContainer":{{"Metadata":[{{"ratingKey":"1","type":"movie","title":"First","duration":600000{facts}}}]}}}}"#);
                }
                queue_body(request, false).to_owned()
            })
        }
    }

    fn requests_matching(rig: &Rig, needle: &str, from: usize) -> usize {
        rig.requests.lock().unwrap().iter().skip(from).filter(|r| r.contains(needle)).count()
    }

    /// The Library page on top of a grid holding `items` unwatched items of the rig's server.
    fn library_with_grid(rig: &mut Rig, t: &mut u32, items: usize) {
        rig.release.send(()).unwrap(); // nothing parks: the first request answers at once
        rig.app.bridge.seed_library_grid_for_test(rig.sid, items);
        super::super::bridge::nav_select_tab(&mut rig.app.pages, AppArg::Library);
        settle(&mut rig.app, t);
        assert!(matches!(rig.app.route(), AppArg::Library), "premise: the Library is on top");
    }

    /// Play item 1 from the page on top (the fixture's first request is already released) and wait
    /// for the player to be mounted.
    fn play_from_here(rig: &mut Rig, t: &mut u32) {
        rig.request();
        rig.accept_start();
        poll_until("the plan did not land through update", || {
            let mut fr = Frame::begin(&rig.app.player.session, rig.app.bridge.metadata_view());
            fr.now = *t;
            unsafe { update(&mut rig.app, &mut fr); }
            rig.app.adapters.player.is_live()
        });
        settle(&mut rig.app, t);
        assert!(super::super::bridge::player(&rig.app.pages).is_some(), "premise: the player is on top");
    }

    /// **The plan's headline test.** Library -> play -> the stream reaches its end -> back on the
    /// Library: the grid tile reads watched, without the page being re-opened, and the origin
    /// server was not scrobbled a second time (the stop it reported made the view count).
    #[test]
    fn library_play_to_the_end_returns_with_the_tile_watched() {
        let _serial = plx_base::testlock::serial();
        let server = StopServer::new();
        let Some(mut rig) = server.rig(true) else { return };
        // A second source: with one server the end-of-stream fan-out has nowhere to go and is never
        // queued, which would leave the "no second scrobble" check below guarding nothing.
        plx_plex::plex::register_for_test("lifecycle-fixture-copy", "127.0.0.1", 9, "synthetic", "lifecycle-copy");
        let mut t = product_transition(&mut rig.app);
        settle(&mut rig.app, &mut t);
        library_with_grid(&mut rig, &mut t, 1);
        assert_eq!(rig.app.bridge.grid_tile_for_test(0), Some((false, false, 0)), "premise: an untouched tile");
        let origin = rig.app.pages.nav.top_page().unwrap().id;
        play_from_here(&mut rig, &mut t);
        end_the_stream(&mut rig.app, &mut t);
        settle(&mut rig.app, &mut t);
        assert_eq!(rig.app.pages.nav.top_page().map(|e| e.id), Some(origin), "the end returned to the Library");
        // the rig has no engine, so teardown posts no `stopped`: the engine's own call, at the end
        plx_media::route::scrobble_stop(&mut rig.app.player.session, Some(("1".into(), 600_000, 600_000)), None);
        plx_media::route::drain_scrobble();
        poll_until("the grid tile never read watched", || {
            step_and_run_jobs(&mut rig, &mut t);
            rig.app.bridge.grid_tile_for_test(0) == Some((true, false, 0))
        });
        // The read-back that found the end queued the fan-out-only request (a second server is
        // registered, so there is a copy to reach); only once ViewState has run it is "no second
        // scrobble" a statement about it and not about the moment before it started.
        poll_until("ViewState never went idle", || {
            step_and_run_jobs(&mut rig, &mut t); // the fan-out leaves as a job the held runner parks
            !rig.app.bridge.viewstate_busy_for_test()
        });
        assert_eq!(requests_matching(&rig, "/:/scrobble", 0), 0, "no second scrobble on the origin");
    }

    /// The Recommended page's rows ARE the server's hubs, and nothing else fetches them: a page
    /// mounted on `LibraryRecommended` (no shelf seeded by the test) must reach the server's
    /// `/hubs/sections/{id}`. The publish request it sends was once dropped because only the
    /// full grid's route was let through. A test build parks the section hubs' read until the test
    /// runs it off the frame (`StoreRunner::Held`), so each pass of the wait runs what the page's
    /// step left parked, and the request it sends is the one the server counts.
    #[test]
    fn the_recommended_page_fetches_the_sections_hubs() {
        let _serial = plx_base::testlock::serial();
        let server = StopServer::new();
        let Some(mut rig) = server.rig(true) else { return };
        let mut t = product_transition(&mut rig.app);
        settle(&mut rig.app, &mut t);
        library_with_grid(&mut rig, &mut t, 1);
        assert_eq!(requests_matching(&rig, "/hubs/sections/", 0), 0, "premise: the grid page alone asks for no hubs");
        super::super::bridge::nav_push(&mut rig.app.pages, AppArg::LibraryRecommended { section: 0 });
        settle(&mut rig.app, &mut t);
        assert!(matches!(rig.app.route(), AppArg::LibraryRecommended { .. }), "premise: the Recommended page is on top");
        poll_until("the Recommended page never fetched the section's hubs", || {
            step(&mut rig.app, &mut t, vec![]);
            rig.app.bridge.run_held_store_jobs_of_for_test(plx_data::stores::StoreId::Browse);
            requests_matching(&rig, "/hubs/sections/", 0) > 0
        });
    }

    /// **The owner's path, through the whole application**: the Library's Collections page (its
    /// listing fetched from a server answering as PMS does), OK on a collection tile, OK on a member
    /// of the collection. Returns the app once the member's OK has been given, with the requests
    /// the server saw.
    fn collections_page_to_member_ok() -> Option<(Rig, u32)> {
        let rig = Rig::serving_with(|request: &str| {
            if request.contains("type=18") && request.contains("/library/sections/") {
                return r#"{"MediaContainer":{"size":1,"totalSize":1,"Metadata":[
                    {"ratingKey":"50001","key":"/library/collections/50001/children","type":"collection",
                     "subtype":"movie","title":"Set","titleSort":"Set","childCount":"3","smart":"0",
                     "librarySectionID":1,"librarySectionTitle":"Movies"}]}}"#.into();
            }
            if request.contains("/library/collections/50001/children") {
                return r#"{"MediaContainer":{"size":3,"totalSize":3,"Metadata":[
                    {"ratingKey":"7001","type":"movie","title":"First","year":2001,"librarySectionID":1},
                    {"ratingKey":"7002","type":"movie","title":"Second","year":2002,"librarySectionID":1},
                    {"ratingKey":"7003","type":"movie","title":"Third","year":2003,"librarySectionID":1}]}}"#.into();
            }
            if request.starts_with("GET /library/metadata/") || request.starts_with("GET /library/collections/") {
                return r#"{"MediaContainer":{"size":1,"Metadata":[{"ratingKey":"50001","type":"collection","title":"Set","childCount":"3"}]}}"#.into();
            }
            r#"{"MediaContainer":{"size":0}}"#.into()
        })?;
        let mut rig = rig;
        let mut t = product_transition(&mut rig.app);
        settle(&mut rig.app, &mut t);
        rig.release.send(()).unwrap();
        rig.app.bridge.seed_registered_browse_for_test([rig.sid, rig.sid]);
        super::super::bridge::nav_select_tab(&mut rig.app.pages, AppArg::Library);
        settle(&mut rig.app, &mut t);
        super::super::bridge::nav_push(&mut rig.app.pages, AppArg::LibraryCollections { section: 0 });
        for _ in 0..120 { step(&mut rig.app, &mut t, vec![]); std::thread::sleep(Duration::from_millis(2)); }
        let page = rig.app.pages.nav.top_page().unwrap();
        assert!(page.arg == AppArg::LibraryCollections { section: 0 }, "premise: the Collections page is on top");
        let instance = rig.app.pages.nav.instance_of(page.id).unwrap();
        rig.app.pages.emit(plx_machine::machine::MachineId::Nav, plx_machine::machine::Fx::Deliver(
            plx_machine::machine::MachineId::Instance(instance),
            plx_machine::machine::Delivery::Screen(plx_ui::screen::ScreenEvent::App(
                plx_screens::registry::AppMsg::Library(plx_screens::registry::LibraryCmd::FocusGrid { row: 0, col: 0 })))));
        for _ in 0..40 { step(&mut rig.app, &mut t, vec![]); }
        assert!(rig.app.bridge.grid_tile_for_test(0).is_some(), "premise: the page lists the server's collection");
        press_ok(&mut rig.app, &mut t);
        // A test build parks the collection's requests until the test runs them, off the frame
        // (`StoreRunner::Held`): each frame's pump emits the next one, which runs before the next frame.
        for _ in 0..60 {
            rig.app.bridge.run_held_store_jobs_for_test();
            step(&mut rig.app, &mut t, vec![]);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(super::super::words::route_word(&rig.app.route()), "collection",
            "OK on the collection tile opens the collection");
        for _ in 0..40 {
            rig.app.bridge.run_held_store_jobs_for_test();
            step(&mut rig.app, &mut t, vec![]);
            std::thread::sleep(Duration::from_millis(2));
        }
        // Down off the header onto the first member, rest, then OK.
        let tick = |t: u32| plx_machine::machine::Tick { ms: t + 16, dt_us: 16_000 };
        for _ in 0..3 {
            if rig.app.bridge.collection_card_focused(&rig.app.pages) { break; }
            let down = plx_ui::fixture::key(plx_machine::machine::Key::Down, tick(t));
            step(&mut rig.app, &mut t, vec![down]);
            let up = plx_ui::fixture::key_up(plx_machine::machine::Key::Down, tick(t));
            step(&mut rig.app, &mut t, vec![up]);
            for _ in 0..40 {
                rig.app.bridge.run_held_store_jobs_for_test();
                step(&mut rig.app, &mut t, vec![]);
            }
        }
        assert!(rig.app.bridge.collection_card_focused(&rig.app.pages), "premise: a member holds the focus");
        press_ok(&mut rig.app, &mut t);
        for _ in 0..40 { step(&mut rig.app, &mut t, vec![]); }
        Some((rig, t))
    }

    fn press_ok(app: &mut App, t: &mut u32) {
        let tick = |t: u32| plx_machine::machine::Tick { ms: t + 16, dt_us: 16_000 };
        let down = plx_ui::fixture::key(plx_machine::machine::Key::Ok, tick(*t));
        step(app, t, vec![down]);
        for _ in 0..3 { step(app, t, vec![]); }
        let up = plx_ui::fixture::key_up(plx_machine::machine::Key::Ok, tick(*t));
        step(app, t, vec![up]);
    }

    #[test]
    fn a_member_ok_on_a_collection_opened_from_the_collections_page_opens_its_detail() {
        let _serial = plx_base::testlock::serial();
        let Some((mut rig, mut t)) = collections_page_to_member_ok() else { return };
        assert_eq!(super::super::words::route_word(&rig.app.route()), "detail",
            "OK on a member opens its detail page");
        // BACK walks the way it came: the collection, then the Collections page.
        for (word, listing) in [("collection", false), ("library", true)] {
            let tick = plx_machine::machine::Tick { ms: t + 16, dt_us: 16_000 };
            step(&mut rig.app, &mut t, vec![plx_ui::fixture::key(plx_machine::machine::Key::Back, tick)]);
            let tick = plx_machine::machine::Tick { ms: t + 16, dt_us: 16_000 };
            step(&mut rig.app, &mut t, vec![plx_ui::fixture::key_up(plx_machine::machine::Key::Back, tick)]);
            for _ in 0..60 { step(&mut rig.app, &mut t, vec![]); }
            assert_eq!(super::super::words::route_word(&rig.app.route()), word, "BACK returns to the {word} page");
            assert_eq!(matches!(rig.app.route(), AppArg::LibraryCollections { .. }), listing);
        }
    }

    /// **`/tmp/plxnative-libosc` on the Collections page** (`fps:library-collections`): the sweep is
    /// the library family's, so it moves the Collections page's scroll, and the focus fingerprint
    /// (`--arm focus`) reports that scroll as `viewport_y`. Both were gated on `AppArg::Library`
    /// alone: the TV run saw 3 focus lines in 80 s and a median of 1 fps against a floor of 30.
    #[test]
    fn the_library_sweep_moves_the_collections_page_and_the_fingerprint_reports_it() {
        let _serial = plx_base::testlock::serial();
        let Some(mut rig) = Rig::serving_with(|request: &str| {
            if request.contains("type=18") && request.contains("/library/sections/") {
                let tiles: Vec<String> = (0..60).map(|i| format!(
                    r#"{{"ratingKey":"{}","key":"/library/collections/{}/children","type":"collection","subtype":"movie","title":"Set {i}","titleSort":"Set {i}","childCount":"3","smart":"0","librarySectionID":1,"librarySectionTitle":"Movies"}}"#,
                    50000 + i, 50000 + i)).collect();
                return format!(r#"{{"MediaContainer":{{"size":60,"totalSize":60,"Metadata":[{}]}}}}"#, tiles.join(","));
            }
            r#"{"MediaContainer":{"size":0}}"#.into()
        }) else { return };
        let mut t = product_transition(&mut rig.app);
        settle(&mut rig.app, &mut t);
        rig.release.send(()).unwrap();
        rig.app.bridge.seed_registered_browse_for_test([rig.sid, rig.sid]);
        super::super::bridge::nav_select_tab(&mut rig.app.pages, AppArg::Library);
        settle(&mut rig.app, &mut t);
        super::super::bridge::nav_push(&mut rig.app.pages, AppArg::LibraryCollections { section: 0 });
        for _ in 0..120 { step(&mut rig.app, &mut t, vec![]); std::thread::sleep(Duration::from_millis(2)); }
        assert!(matches!(rig.app.route(), AppArg::LibraryCollections { .. }), "premise: the Collections page is on top");
        let page = rig.app.pages.nav.top_page().unwrap();
        let instance = rig.app.pages.nav.instance_of(page.id).unwrap();
        rig.app.pages.emit(plx_machine::machine::MachineId::Nav, plx_machine::machine::Fx::Deliver(
            plx_machine::machine::MachineId::Instance(instance),
            plx_machine::machine::Delivery::Screen(plx_ui::screen::ScreenEvent::App(
                plx_screens::registry::AppMsg::Library(plx_screens::registry::LibraryCmd::FocusGrid { row: 0, col: 0 })))));
        for _ in 0..40 { step(&mut rig.app, &mut t, vec![]); }
        assert!(rig.app.bridge.grid_tile_for_test(0).is_some(), "premise: the page lists the server's collections");
        let scroll = |app: &App| app.pages.nav.top_page().and_then(|page| page.inst.as_ref())
            .and_then(|inst| inst.screen.as_any()).and_then(|any| any.downcast_ref::<plx_screens::library::LibraryScreen>())
            .expect("the Collections page is a LibraryScreen").probe_scroll_target();
        let viewport_y = |app: &App| {
            let line = super::super::bridge::content_probe(&app.pages, &app.bridge);
            line.split("viewport_y=").nth(1).and_then(|rest| rest.split_whitespace().next()).and_then(|v| v.parse::<f32>().ok())
        };
        assert_eq!(scroll(&rig.app), 0.0, "premise: the page rests at its top");
        assert!(viewport_y(&rig.app).is_some(), "the fingerprint reports the Collections page's viewport");
        rig.app.scenarios.dev.lib_osc = true;
        for _ in 0..900 {
            step(&mut rig.app, &mut t, vec![]);
            if scroll(&rig.app) > 0.5 { break; }
        }
        assert!(scroll(&rig.app) > 0.5, "the sweep moved the Collections page's scroll, now {}", scroll(&rig.app));
        for _ in 0..30 { step(&mut rig.app, &mut t, vec![]); }
        assert!(viewport_y(&rig.app).is_some_and(|y| y > 0.0),
            "the fingerprint reports the scroll the sweep made: {:?}", viewport_y(&rig.app));
    }

    /// A stop in the middle: the tile shows the new resume point on the frame the stop lands, with
    /// the server's answer still parked, and the answer then agrees with it.
    #[test]
    fn a_stop_in_the_middle_shows_its_resume_point_before_the_server_answers() {
        let _serial = plx_base::testlock::serial();
        let server = StopServer::new();
        let Some(mut rig) = server.rig(false) else { return };
        let mut t = product_transition(&mut rig.app);
        settle(&mut rig.app, &mut t);
        library_with_grid(&mut rig, &mut t, 1);
        play_from_here(&mut rig, &mut t);
        server.hold_reads.store(true, Ordering::SeqCst); // the play's own reads are done
        let back = back_key(t + 16);
        step(&mut rig.app, &mut t, vec![back]);
        plx_media::route::scrobble_stop(&mut rig.app.player.session, Some(("1".into(), 150_000, 600_000)), None);
        plx_media::route::drain_scrobble();
        poll_until("the tile never showed the resume point", || {
            step(&mut rig.app, &mut t, vec![]);
            rig.app.bridge.grid_tile_for_test(0).is_some_and(|tile| tile.2 == 150_000)
        });
        server.hold_reads.store(false, Ordering::SeqCst);
        for _ in 0..8 { step(&mut rig.app, &mut t, vec![]); }
        assert_eq!(rig.app.bridge.grid_tile_for_test(0).map(|tile| tile.2), Some(150_000));
    }

    /// **A stop never outlives the identity it was made under.** A `stopped` POST stalled on a slow
    /// server returns after the user switched profile; the new identity must get no read-back (a
    /// request on its token) from the old one's stop.
    #[test]
    fn a_stop_that_lands_after_an_identity_change_reads_nothing_back() {
        let _serial = plx_base::testlock::serial();
        let server = StopServer::new();
        let Some(mut rig) = server.rig(false) else { return };
        let mut t = product_transition(&mut rig.app);
        settle(&mut rig.app, &mut t);
        library_with_grid(&mut rig, &mut t, 1);
        play_from_here(&mut rig, &mut t);
        let back = back_key(t + 16);
        step(&mut rig.app, &mut t, vec![back]);
        let from = rig.requests.lock().unwrap().len();
        server.hold_stop.store(true, Ordering::SeqCst);
        plx_media::route::scrobble_stop(&mut rig.app.player.session, Some(("1".into(), 150_000, 600_000)), None);
        poll_until("premise: the stopped POST never reached the server", || {
            requests_matching(&rig, "state=stopped", from) == 1
        });
        let _ = super::super::boot::activate_server_owned(&mut rig.app.bridge); // the identity changed
        server.hold_stop.store(false, Ordering::SeqCst);
        plx_media::route::drain_scrobble();
        assert!(!plx_media::route::stops_owed(), "the retired stop is no longer owed");
        // before any frame: the mailbox itself, which the app would otherwise drain into the new stores
        assert!(plx_media::route::take_landed_stops().is_empty(), "the old identity's stop reached the mailbox");
        // the read-back is a `StoreJob`: the rig's `Held` runner parks it, and only the sweep runs it
        for _ in 0..12 { step_and_run_jobs(&mut rig, &mut t); }
        assert_eq!(requests_matching(&rig, "GET /library/metadata/", from), 0, "the old identity's stop was read back");
    }

    /// **A stop the server refused still reaches the page it returns to** (a #575 regression the
    /// read-back alone cannot fix: a refused stop asks for none). The Detail page shows the
    /// position the viewer reached the frame the stop lands, with its own re-read still out, and
    /// that re-read, asked for by the stop itself, then puts the server's word back (nothing was
    /// stored, so the resume point is the one the server holds).
    #[test]
    fn a_refused_stop_patches_the_detail_page_and_the_page_then_re_reads_itself() {
        let _serial = plx_base::testlock::serial();
        let server = StopServer::new();
        let Some(mut rig) = server.rig(false) else { return };
        let mut t = product_transition(&mut rig.app);
        settle(&mut rig.app, &mut t);
        rig.release.send(()).unwrap(); // nothing parks: the first request answers at once
        super::super::bridge::open_detail(&mut rig.app.pages, &mut rig.app.bridge, rig.sid, "1", None, None);
        settle(&mut rig.app, &mut t);
        land_detail_reads(&mut rig, &mut t);
        let resume = |rig: &Rig| rig.app.bridge.metadata_view().current().map(|d| (d.rk.clone(), d.resume_ms));
        assert_eq!(resume(&rig), Some(("1".into(), 0)), "premise: the Detail page is loaded, unplayed");
        play_from_here(&mut rig, &mut t);
        server.refuse_stop.store(true, Ordering::SeqCst);
        let back = back_key(t + 16);
        step(&mut rig.app, &mut t, vec![back]);
        settle(&mut rig.app, &mut t);
        assert!(rig.app.route().same_instance(&detail_arg(rig.sid)), "premise: Back returned to the Detail page");
        // Back re-requests the page it uncovers; that read is out (held) when the stop lands
        let asked = plx_data::metadata::detail_generation_for_test(rig.app.bridge.metadata_mut().adapter_ref());
        plx_media::route::scrobble_stop(&mut rig.app.player.session, Some(("1".into(), 150_000, 600_000)), None);
        plx_media::route::drain_scrobble();
        poll_until("the Detail page never showed the position the viewer reached", || {
            step(&mut rig.app, &mut t, vec![]);
            resume(&rig) == Some(("1".into(), 150_000))
        });
        assert!(!server.stopped.load(Ordering::SeqCst), "premise: the server kept nothing");
        assert!(plx_data::metadata::detail_generation_for_test(rig.app.bridge.metadata_mut().adapter_ref()) > asked,
            "the refused stop asked the page for a re-read of its own");
        let from = rig.requests.lock().unwrap().len();
        land_detail_reads(&mut rig, &mut t);
        assert!(requests_matching(&rig, "GET /library/metadata/1", from) >= 1, "the page read the server");
        assert_eq!(resume(&rig), Some(("1".into(), 0)), "and settled on the server's word");
    }

    /// Network reconcile work is held while the player is on top: a mark-watched that lands under
    /// the player owes a read-back and a hubs refetch, and neither goes out until the player leaves.
    #[test]
    fn no_reconcile_read_goes_out_while_the_player_is_on_top() {
        let _serial = plx_base::testlock::serial();
        let server = StopServer::new();
        let Some(mut rig) = server.rig(false) else { return };
        let mut t = product_transition(&mut rig.app);
        settle(&mut rig.app, &mut t);
        library_with_grid(&mut rig, &mut t, 1);
        play_from_here(&mut rig, &mut t);
        let (reads_from, refetches) = (rig.requests.lock().unwrap().len(), rig.app.bridge.hub_refetches_for_test());
        assert!(rig.app.bridge.viewstate_run(plx_data::stores::viewstate::ViewStateCmd::Request {
            sid: rig.sid, rk: "1".into(), write: plx_data::viewstate::Write::Watched,
            scope: plx_data::viewstate::Scope::Leaf, guid: String::new(),
        }).changed);
        poll_until("the write never reached the server", || {
            step_and_run_jobs(&mut rig, &mut t);
            requests_matching(&rig, "/:/scrobble", reads_from) == 1
        });
        // The held jobs run in these frames too: a read-back sent under the player would reach the
        // server here, so the count below is a statement about what the store sent.
        for _ in 0..12 { step_and_run_jobs(&mut rig, &mut t); }
        assert!(super::super::bridge::player(&rig.app.pages).is_some(), "premise: still under the player");
        assert_eq!(requests_matching(&rig, "GET /library/metadata/", reads_from), 0, "a read-back went out under the player");
        assert_eq!(rig.app.bridge.hub_refetches_for_test(), refetches, "a hubs refetch started under the player");
        let back = back_key(t + 16);
        step(&mut rig.app, &mut t, vec![back]);
        poll_until("the read-back never went out after the player left", || {
            step_and_run_jobs(&mut rig, &mut t);
            requests_matching(&rig, "GET /library/metadata/", reads_from) >= 1
        });
        for _ in 0..12 { step_and_run_jobs(&mut rig, &mut t); }
        assert_eq!(requests_matching(&rig, "GET /library/metadata/", reads_from), 1, "one batch, at the exit");
    }

    /// An Up Next chain stops one episode after another with no frame between them (the previous
    /// worker may be stalled, and the main thread joins it before the next stop): every episode
    /// still gets its own patch.
    #[test]
    fn two_stops_with_no_frame_between_them_each_patch_their_own_tile() {
        let _serial = plx_base::testlock::serial();
        let server = StopServer::new();
        let Some(mut rig) = server.rig(false) else { return };
        let mut t = product_transition(&mut rig.app);
        settle(&mut rig.app, &mut t);
        library_with_grid(&mut rig, &mut t, 2);
        play_from_here(&mut rig, &mut t);
        server.hold_reads.store(true, Ordering::SeqCst); // the play's own reads are done
        plx_media::route::scrobble_stop(&mut rig.app.player.session, Some(("1".into(), 30_000, 60_000)), None);
        plx_media::route::scrobble_stop(&mut rig.app.player.session, Some(("2".into(), 20_000, 60_000)), None);
        plx_media::route::drain_scrobble();
        step(&mut rig.app, &mut t, vec![]);
        let resumes = |rig: &Rig| [0, 1].map(|i| rig.app.bridge.grid_tile_for_test(i).map(|tile| tile.2));
        assert_eq!(resumes(&rig), [Some(30_000), Some(20_000)], "an episode's stop was lost");
        server.hold_reads.store(false, Ordering::SeqCst);
    }

    /// The same session drained to its end with nothing queued after it: `finish_playback` leaves
    /// through `exit_player`, so an end of stream lands where BACK and Stop do — the page the
    /// session was launched from (§5.1), not Home.
    #[test]
    fn an_end_of_stream_without_up_next_returns_to_the_detail_page() {
        let _serial = plx_base::testlock::serial();
        let Some(mut rig) = Rig::serving(false) else {
            eprintln!("SKIPPED an_end_of_stream_without_up_next_returns_to_the_detail_page: \
                lifecycle fixture worker thread could not be spawned");
            return;
        };
        let mut t = product_transition(&mut rig.app);
        settle(&mut rig.app, &mut t);
        super::super::bridge::open_detail(&mut rig.app.pages, &mut rig.app.bridge, rig.sid, "1", None, None);
        settle(&mut rig.app, &mut t);
        let origin = play_with_a_landing_inside_the_dip(&mut rig, &mut t);
        assert!(plx_media::route::up_next(&rig.app.player.session).is_none(), "a film: nothing queued");
        end_the_stream(&mut rig.app, &mut t);
        settle(&mut rig.app, &mut t);
        assert!(
            rig.app.route().same_instance(&detail_arg(rig.sid)),
            "end of stream landed on {:?}, not the detail page",
            rig.app.route().id(),
        );
        assert_eq!(rig.app.pages.nav.top_page().map(|e| e.id), Some(origin));
    }

    /// A session started straight from Home (the hero's Play, or a deck tile in Play mode) has no
    /// detail page in between, so the page the session returns to is Home itself — the same entry,
    /// with its memory.
    #[test]
    fn back_from_a_session_started_on_home_returns_to_home() {
        let _serial = plx_base::testlock::serial();
        let Some(mut rig) = Rig::serving(false) else {
            eprintln!("SKIPPED back_from_a_session_started_on_home_returns_to_home: \
                lifecycle fixture worker thread could not be spawned");
            return;
        };
        let mut t = product_transition(&mut rig.app);
        settle(&mut rig.app, &mut t);
        let origin = play_with_a_landing_inside_the_dip(&mut rig, &mut t);
        let back = back_key(t + 16);
        step(&mut rig.app, &mut t, vec![back]);
        settle_after_back(&mut rig.app, &mut t);
        assert!(matches!(rig.app.route(), AppArg::Home));
        assert_eq!(rig.app.pages.nav.top_page().map(|e| e.id), Some(origin));
    }

    /// Puts the Next episode preference back whatever the test did.
    struct NextEpisodeGuard(plx_media::route::NextEpisodeMode);
    impl NextEpisodeGuard {
        fn set(mode: plx_media::route::NextEpisodeMode) -> Self {
            let prior = plx_media::route::next_episode_mode();
            plx_media::route::restore_next_episode_mode(mode);
            Self(prior)
        }
    }
    impl Drop for NextEpisodeGuard {
        fn drop(&mut self) {
            plx_media::route::restore_next_episode_mode(self.0);
        }
    }

    /// A resolved, mounted Player whose playhead sits inside a `final` credits marker, so the REAL
    /// `player_hud::slot` (via `Frame::begin`) decides the control row instead of a slot the test
    /// built itself. Returns the frame clock after one settled frame.
    fn playing_inside_final_credits(rig: &mut Rig) -> u32 {
        rig.request();
        rig.resolve();
        super::super::bridge::nav_push(&mut rig.app.pages, AppArg::Player);
        frame(&mut rig.app, 16);
        plx_data::metadata::set_playing_markers_for_test(
            rig.app.bridge.metadata_mut().state_mut(),
            vec![plx_data::metadata::Marker {
                kind: plx_data::metadata::MarkerKind::Credits,
                start_ms: 0,
                end_ms: 60_000,
                final_seg: true,
            }],
        );
        plx_media::player::restore_state_for_test(plx_media::player::PlaybackState::Playing as u8);
        plx_media::player::SHARED
            .playpos_ns
            .store(30_000_000_000, std::sync::atomic::Ordering::Relaxed);
        let mut t = 16;
        // The slot reaches the screen in `update`, which runs after the frame that ticks the
        // countdown — so it takes two frames for the countdown to see it.
        step(&mut rig.app, &mut t, vec![]);
        step(&mut rig.app, &mut t, vec![]);
        t
    }

    /// **Next episode: After credits.** The credits of an episode with a successor queued arm no
    /// Up Next countdown, so nothing is requested however long they run; the end of the stream
    /// then plays the successor exactly as it always has.
    #[test]
    fn after_credits_requests_no_successor_until_the_stream_ends() {
        let _serial = plx_base::testlock::serial();
        let Some(mut rig) = Rig::new() else {
            eprintln!("SKIPPED after_credits_requests_no_successor_until_the_stream_ends: \
                lifecycle fixture worker thread could not be spawned");
            return;
        };
        let _mode = NextEpisodeGuard::set(plx_media::route::NextEpisodeMode::AfterCredits);
        let mut t = playing_inside_final_credits(&mut rig);
        let fr = Frame::begin(&rig.app.player.session, rig.app.bridge.metadata_view());
        assert!(fr.ctrl.is_discs(), "inside the credits the row keeps the discs");
        t += plx_appkit::up_next::COUNTDOWN_MS;
        step(&mut rig.app, &mut t, vec![]);
        let player = super::super::bridge::player(&rig.app.pages).unwrap();
        assert!(!player.up_next.expired(t), "no countdown was armed for the credits");
        assert!(!plx_media::route::play_pending(), "the credits alone must not request the successor");
        assert_eq!(plx_media::route::up_next(&rig.app.player.session).unwrap().rk, "2");

        // No engine in this rig (the state is forced), so nothing recovers under the end.
        plx_media::player::SHARED.ended.store(true, std::sync::atomic::Ordering::Relaxed);
        let mut fr = Frame::begin(&rig.app.player.session, rig.app.bridge.metadata_view());
        fr.now = t;
        unsafe { playback_tick(&mut rig.app, &mut fr); }
        assert!(plx_media::route::play_pending(), "the end of the stream still plays the next episode");
    }

    /// **Next episode: Countdown** on the same rig as the test above, so the two differ only in
    /// the preference: the credits put the Up Next tile up and its countdown requests the
    /// successor before the stream ends.
    #[test]
    fn countdown_requests_the_successor_from_inside_the_credits() {
        let _serial = plx_base::testlock::serial();
        let Some(mut rig) = Rig::new() else {
            eprintln!("SKIPPED countdown_requests_the_successor_from_inside_the_credits: \
                lifecycle fixture worker thread could not be spawned");
            return;
        };
        let _mode = NextEpisodeGuard::set(plx_media::route::NextEpisodeMode::Countdown);
        let mut t = playing_inside_final_credits(&mut rig);
        let fr = Frame::begin(&rig.app.player.session, rig.app.bridge.metadata_view());
        assert!(
            matches!(fr.ctrl, plx_appkit::player_hud::ControlSlot::UpNext(_)),
            "inside the credits the tile takes over",
        );
        assert!(!plx_media::route::play_pending(), "the countdown is still running");
        // The successor's resolve answers at once (only the first request is held), so a worker
        // scheduled between `playback_tick` and `update` inside one `step` lands and is pumped before
        // the assertion below: hold its answers, so the request it makes is what is graded.
        rig.hold_answers();
        t += plx_appkit::up_next::COUNTDOWN_MS;
        step(&mut rig.app, &mut t, vec![]);
        assert!(plx_media::route::play_pending(), "the countdown requests the successor");
    }

    /// **Next episode: Off.** The end of an episode that HAS a successor leaves the player like a
    /// film does: back to the page the session was launched from, nothing requested.
    #[test]
    fn off_returns_to_the_detail_page_at_the_end_of_an_episode_with_a_successor() {
        let _serial = plx_base::testlock::serial();
        let Some(mut rig) = Rig::serving(true) else {
            eprintln!("SKIPPED off_returns_to_the_detail_page_at_the_end_of_an_episode_with_a_successor: \
                lifecycle fixture worker thread could not be spawned");
            return;
        };
        let _mode = NextEpisodeGuard::set(plx_media::route::NextEpisodeMode::Off);
        let mut t = product_transition(&mut rig.app);
        settle(&mut rig.app, &mut t);
        super::super::bridge::open_detail(&mut rig.app.pages, &mut rig.app.bridge, rig.sid, "1", None, None);
        settle(&mut rig.app, &mut t);
        let origin = play_with_a_landing_inside_the_dip(&mut rig, &mut t);
        assert!(plx_media::route::up_next(&rig.app.player.session).is_some(), "premise: a successor is queued");
        end_the_stream(&mut rig.app, &mut t);
        settle(&mut rig.app, &mut t);
        assert!(!plx_media::route::play_pending(), "Off must not request the successor");
        assert!(
            rig.app.route().same_instance(&detail_arg(rig.sid)),
            "end of stream landed on {:?}, not the detail page",
            rig.app.route().id(),
        );
        assert_eq!(rig.app.pages.nav.top_page().map(|e| e.id), Some(origin));
    }

    // ---- leaving an item from its credits marks it watched --------------------------------------
    //
    // `PlaybackSession::finished` is armed by the app and taken by the stop it precedes, so nothing
    // can read it in between: the server's request log is the observable. The stop only reports
    // when a real Engine stands (`teardown` builds its final report from the Engine's clock), hence
    // `playing_live_at`.

    /// A real Engine over E1 of the fixture's two-episode queue (60 s long), its playhead at
    /// `pos_ms`: the stop that ends it reports that position, as the loop's own stop does.
    fn playing_live_at(rig: &mut Rig, pos_ms: i64) {
        start_film(rig);
        assert!(rig.app.adapters.player.is_live(), "fixture must create the actual Engine");
        plx_media::player::SHARED.duration_ns.store(60_000_000_000, Ordering::Relaxed);
        plx_media::player::SHARED.playpos_ns.store(pos_ms * 1_000_000, Ordering::Relaxed);
    }

    /// Where E1's `state=stopped` report and its `/:/scrobble` sit in the fixture's request log,
    /// once the stop's worker has finished.
    fn e1_stop_requests(rig: &Rig) -> (Option<usize>, Option<usize>) {
        plx_media::route::drain_scrobble();
        let seen = rig.requests.lock().unwrap();
        (
            seen.iter().position(|l| {
                l.contains("/:/timeline") && l.contains("ratingKey=1&") && l.contains("state=stopped")
            }),
            seen.iter().position(|l| l.contains("/:/scrobble?key=1&")),
        )
    }

    /// **Up Next from the credits marks the episode it leaves watched.** Credits start before the
    /// server's own watched threshold, so the `stopped` report at 30 s of 60 s would leave the
    /// episode unwatched: the loop's `play_up_next` arms the flag and the stop scrobbles after it.
    #[test]
    fn up_next_from_the_credits_scrobbles_the_episode_it_leaves() {
        let _serial = plx_base::testlock::serial();
        let Some(mut rig) = Rig::new() else {
            eprintln!("SKIPPED up_next_from_the_credits_scrobbles_the_episode_it_leaves: \
                lifecycle fixture worker thread could not be spawned");
            return;
        };
        let _mode = NextEpisodeGuard::set(plx_media::route::NextEpisodeMode::Countdown);
        playing_live_at(&mut rig, 30_000);
        let mut fr = rig.expire_countdown_on_mounted_player();
        unsafe { playback_tick(&mut rig.app, &mut fr); }
        assert!(plx_media::route::play_pending(), "the countdown requested the successor");

        let (stopped, scrobbled) = e1_stop_requests(&rig);
        let stopped = stopped.expect("E1's `stopped` report reached the server");
        assert!(
            scrobbled.is_some_and(|s| s > stopped),
            "E1 is marked watched after its `stopped` report: {:?}",
            rig.requests.lock().unwrap(),
        );
    }

    /// **Up Next past the server's threshold leaves the view to the server.** The same hand-off at
    /// 59 s of 60 s (the end of the stream and the countdown's expiry reach `play_up_next` too, and
    /// on a typical episode the credits start past 90 %): the `stopped` report already makes the
    /// server count the view, and a scrobble on top would record a second one.
    #[test]
    fn up_next_past_the_servers_threshold_posts_stopped_alone() {
        let _serial = plx_base::testlock::serial();
        let Some(mut rig) = Rig::new() else {
            eprintln!("SKIPPED up_next_past_the_servers_threshold_posts_stopped_alone: \
                lifecycle fixture worker thread could not be spawned");
            return;
        };
        let _mode = NextEpisodeGuard::set(plx_media::route::NextEpisodeMode::Countdown);
        playing_live_at(&mut rig, 59_000);
        let mut fr = rig.expire_countdown_on_mounted_player();
        unsafe { playback_tick(&mut rig.app, &mut fr); }
        assert!(plx_media::route::play_pending(), "the countdown requested the successor");

        let (stopped, scrobbled) = e1_stop_requests(&rig);
        assert!(stopped.is_some(), "E1's `stopped` report reached the server");
        assert!(scrobbled.is_none(), "no second view: {:?}", rig.requests.lock().unwrap());
    }

    /// **Skip on a `final` credits marker marks the item watched.** With Next episode Off nothing
    /// starts after it, so `play_up_next` never runs and the skip itself must arm the flag: the
    /// stop that leaves the player scrobbles the item the viewer finished.
    #[test]
    fn skipping_final_credits_scrobbles_the_item_it_finishes() {
        let _serial = plx_base::testlock::serial();
        let Some(mut rig) = Rig::new() else {
            eprintln!("SKIPPED skipping_final_credits_scrobbles_the_item_it_finishes: \
                lifecycle fixture worker thread could not be spawned");
            return;
        };
        let _mode = NextEpisodeGuard::set(plx_media::route::NextEpisodeMode::Off);
        playing_live_at(&mut rig, 30_000);
        let skip = plx_appkit::player_hud::ControlSlot::Skip(plx_appkit::skip_pill::prompt_for(
            plx_data::metadata::Marker {
                kind: plx_data::metadata::MarkerKind::Credits,
                start_ms: 0,
                end_ms: 60_000,
                final_seg: true,
            },
        ));
        let app = &mut rig.app;
        super::super::playback::activate_ctrl_row(
            &mut app.player,
            &mut app.adapters.player,
            &mut app.repause_at,
            skip,
            &mut app.refresh_hubs,
            0,
            &mut app.pages,
            &mut app.bridge,
        );
        assert!(!plx_media::route::play_pending(), "Off: the skip leaves the player and requests nothing");

        let (stopped, scrobbled) = e1_stop_requests(&rig);
        let stopped = stopped.expect("E1's `stopped` report reached the server");
        assert!(
            scrobbled.is_some_and(|s| s > stopped),
            "E1 is marked watched after its `stopped` report: {:?}",
            rig.requests.lock().unwrap(),
        );
    }
}

#[cfg(test)]
mod video_plane_gate_tests {
    //! **The present gate's plane term, end to end.** It began in `plx_machine::idle`'s tests and lives here
    //! because it needs three things no lower layer owns together: a `Player` (the bit's one
    //! writer), the gate (`plx_machine::idle`, the machine layer) and this file's own source (the loop is
    //! the one consumer a unit test cannot drive). The gate-only half stayed beside the gate:
    //! `plx_machine::idle`'s `the_present_gate_answers_true_only_while_the_plane_bit_is_set`.
    use plx_machine::idle::{
        invalidate, reset_for_test, should_present, take_local_damage, video_plane_bound,
    };

    /// **Spec §9, §4.4, §16 risk 10 — the whole-frame gate is turned off by the PLANE'S BIT, not
    /// by the player route, and both of its edges are carried.**
    ///
    /// Three separate claims, because three separate things were wrong before phase 9.
    ///
    /// 1. *Only while bound.* The term used to be `|| fr.player` at the call site, i.e. "the
    ///    player SCREEN is up" — true through the whole pre-bind spinner and the whole post-unbind
    ///    fade, when the compositor has an ordinary UI surface and nothing is slaved to it.
    /// 2. *The false edge presents.* The frame the plane goes away on is very often one the gate
    ///    would otherwise skip: the picture is gone and no spring is moving. If that frame is not
    ///    presented, the surface keeps whatever the last video frame left and the opaque region is
    ///    asserted for a plane that is no longer there (§3.3 step 9).
    /// 3. *`opaque_route` is asked on every frame, from the bit.* Pinned from the loop's own source
    ///    — this is the one consumer a unit test cannot drive, `run` needing a live SDL window.
    ///
    /// Observed RED (simulated — the fix changes the signatures the old code called, so the test
    /// cannot be compiled against 88841d3e): restoring `idle::should_present`'s pre-phase-9 body by
    /// deleting the `|| VIDEO_PLANE.load(Relaxed)` term fails claim 1 at
    /// "while the plane is bound every frame presents"; deleting the `!bound` `invalidate()` in
    /// `Player::set_video_plane_bound` fails claim 2 at "the unbind frame presents"; and putting
    /// `fr.player` back as `opaque_route`'s argument fails claim 3.
    #[test]
    fn the_present_gate_answers_true_only_while_the_plane_is_bound() {
        let _g = plx_base::testlock::serial();
        reset_for_test();
        let mut player = plx_media::player::machine::Player::new();
        assert!(!video_plane_bound(), "a fresh machine has no plane");

        invalidate();
        assert!(should_present(0), "the damage just raised selects this frame");
        assert!(!should_present(16), "settled, inside the keepalive: nothing to send");

        // ---- the TRUE edge, and what it buys ----
        player.set_video_plane_bound(true);
        assert!(video_plane_bound(), "the machine's edge is the gate's only input");
        for t in [32u32, 48, 64, 80] {
            assert!(
                should_present(t),
                "while the plane is bound every frame presents, unconditionally — nothing about                  this frame moved",
            );
        }

        // A LEVEL is not an edge. Writing the same value again must publish nothing: a second
        // formula term feeding the gate is exactly what risk 10 names.
        let _ = take_local_damage();
        player.set_video_plane_bound(true);
        assert_eq!(
            take_local_damage(),
            0,
            "re-asserting the same bit raised damage — the bit is published on EDGES only",
        );

        // ---- the FALSE edge, on a frame that would otherwise not present ----
        // Nothing else has happened: no input, no landing, no spring, and the keepalive is not due
        // (LAST_PRESENT is 0 and KEEPALIVE_MS is 2000). Without the edge's own report this frame
        // is skipped, and the last video frame stays on the panel behind a stale opaque region.
        player.set_video_plane_bound(false);
        assert!(!video_plane_bound());
        assert!(
            should_present(96),
            "the unbind frame presents even though nothing else about it moved",
        );
        assert!(
            !should_present(112),
            "…and the frame after it is an ordinary idle frame again, which is the whole point",
        );

        // ---- claim 3: the loop asks the compositor on EVERY frame, from the bit ----
        let whole = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/app/run.rs"),
        )
        .expect("read run.rs");
        // The loop's own text — everything above this module — so the strings this pin searches
        // for are not found in the pin itself.
        let src = whole
            .split("#[cfg(test)]\nmod video_plane_gate_tests")
            .next()
            .expect("the loop's source");
        assert!(
            !src.contains("plx_platform::tv::window::opaque_route(fr.player)"),
            "the opaque region must not be keyed on the ROUTE — the plane's bit is the question",
        );
        // **UNCONDITIONAL.** The claim this pins is not where the call sits relative to the
        // present decision — spec §3.3 step 9 puts it AFTER, and since phase 11 so does the loop,
        // because the render cache's upload step now runs on the PRESENTING side of that decision
        // and this call has to follow it. The claim is that the call is never nested inside an
        // `if fr.present` block: the false edge after an unbind may land on a frame the gate does
        // not present, and nothing else in the loop would carry it. This assertion used to be
        // `call < gate`, which was a proxy for that and stopped being one when the upload moved.
        //
        // D1 extracted the prepare window into `prepare_window(app, fr)` to get `run` under its
        // 200-line budget, so "the loop body's own depth" is now two claims: the call is at
        // `prepare_window`'s own body depth (four spaces, never inside that function's one
        // `if fr.present`), and `prepare_window` itself is called at the loop body's (eight).
        const CALL: &str = "    plx_platform::tv::window::opaque_route(app.player.video_plane_bound);";
        assert_eq!(
            src.lines().filter(|l| *l == CALL).count(),
            1,
            "`opaque_route` must be called exactly once, unnested — inside `if fr.present` it is \
             lost on exactly the frames it matters on",
        );
        assert!(
            src.lines().any(|l| l == "        prepare_window(app, fr);"),
            "…and the window that holds it runs on every iteration, at the loop body's own depth",
        );
        let call = src.find(CALL.trim_start()).expect("the call");
        let draw = src
            .find("let (_vx, _vy, _vw, _vh) = draw(app, fr);")
            .expect("the loop's draw");
        assert!(call < draw, "the compositor is told before the frame is drawn");
    }
}

#[cfg(test)]
mod pump_wiring_tests {
    //! `app/run.rs`'s route-unconditional per-frame block has no host fixture — `App` needs a real
    //! SDL/GL context — so this pins the source text itself, the same idiom
    //! `app::words::focusprobe_player_overlay_delegates_to_the_shared_overlay_word_function` uses
    //! for the identical reason. Why `pump_season()` needs this pin at all: see
    //! `plx_data::stores::metadata`'s module doc, and the commit that added this test.
    /// **A frame's modal-dim count is cleared at the head of EVERY iteration**, on the player path
    /// and the preview-bound page too: neither calls `popover::host::begin_frame` (a hardware
    /// plane has no host to snapshot), so a count left by the last popover frame survived into the
    /// first one after the player. `App` has no host fixture, so this pins the source text: one
    /// whole-line call at the loop body's depth, before the frame's draw.
    #[test]
    fn the_frame_head_clears_the_modal_dims_on_every_route() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/app/run.rs"),
        )
        .expect("read run.rs");
        const CALL: &str = "        plx_ui::popover::host::clear_dims();";
        assert_eq!(
            src.lines().filter(|l| *l == CALL).count(),
            1,
            "`clear_dims` must be called exactly once, at the loop body's own depth — unnested, so \
             no route skips it",
        );
        let call = src.find(CALL.trim_start()).expect("the call");
        let draw = src.find("let (_vx, _vy, _vw, _vh) = draw(app, fr);").expect("the loop's draw");
        assert!(call < draw, "the count is cleared before the frame is drawn");
    }

    #[test]
    fn the_route_unconditional_frame_update_pumps_the_season_landing_after_detail() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/app/run.rs"),
        )
        .expect("read run.rs");
        // The exact statement shape `pump_detail()`'s own call already has, indentation included:
        // a bare `.contains("pump_season()")` would still pass if the call were wrapped in a
        // route/feature gate, buried behind a route check, or merely mentioned in a comment (none
        // of which pump the mailbox route-unconditionally). Matching by WHOLE LINE (not a
        // substring search, which extra leading indentation would still satisfy) against the
        // literal `if app.bridge.metadata_pump_season() {` at the SAME indentation as
        // `metadata_pump_detail()`'s own `if` proves it is a sibling statement in the same block
        // — not nested one level deeper inside some other conditional.
        //
        // Stage C1 note: these two now name `Bridge`'s own owned-store wrappers
        // (`metadata_pump_detail`/`metadata_pump_season`), not the old free
        // `plx_data::stores::metadata::pump_detail`/`pump_season` — Metadata moved from a
        // crate-global dispatcher to a per-`Bridge` owner in Stage B, and `run.rs`'s call site
        // moved with it. The pin exists to catch exactly that kind of silent drop, so it must
        // track the real call shape rather than the pre-ownership one.
        const DETAIL_STMT: &str = "        if app.bridge.metadata_pump_detail() {";
        const SEASON_STMT: &str = "        if app.bridge.metadata_pump_season() {";
        let line_index = |needle: &str| {
            src.lines().position(|line| line == needle)
        };
        let detail_at = line_index(DETAIL_STMT)
            .expect("run.rs must still pump the async detail landing every frame, at this exact indentation");
        let season_at = line_index(SEASON_STMT).unwrap_or_else(|| {
            panic!(
                "metadata_pump_season() must be called route-unconditionally, at the same nesting \
                 depth as metadata_pump_detail() (found no `{SEASON_STMT}` line) — its call site \
                 went missing in the phase-7 owned-screens migration and nothing replaced it, so \
                 season_loading() never clears after a season switch: the episode row's spinner \
                 spins forever and every episode press is refused (episodes::action gates on \
                 season_loading())."
            )
        });
        // metadata_pump_detail() must run FIRST: a landed detail's `install_landed_detail` calls
        // `supersede_season()`, invalidating any season fetch for the item being replaced.
        // Pumping season first could apply a stale season landing to CURRENT in the one frame
        // before metadata_pump_detail() replaces it.
        assert!(
            season_at > detail_at,
            "metadata_pump_season() must be pumped AFTER metadata_pump_detail(), not before — \
             metadata_pump_detail() is what supersedes a stale in-flight season fetch when a \
             fresh detail lands"
        );
        // Both statements must be in the SAME enclosing function: no line starting a new `fn` —
        // a new function's own leading `fn`, not the word appearing mid-identifier — between them.
        let no_intervening_fn = src.lines().skip(detail_at + 1).take(season_at - detail_at - 1)
            .all(|line| !line.trim_start().starts_with("fn ")
                && !line.trim_start().starts_with("pub"));
        assert!(
            no_intervening_fn,
            "pump_detail() and pump_season() must be pumped from the same function — found what \
             looks like an intervening function boundary between them"
        );
    }
}

#[cfg(test)]
mod frame_publication_tests {
    //! **The loop publishes the calendar date once per iteration** (`Bridge::publish_today`), so a
    //! screen receives it through `AppViews::today` and reads no clock. `App` has no host fixture
    //! (it needs a real SDL/GL context), so the behaviour is graded on [`publish_frame`] — the one
    //! call the loop makes — and the loop's own text is pinned for that call, the idiom of
    //! `pump_wiring_tests`.
    use super::{clock, publish_frame};
    use crate::app::bridge::Bridge;

    #[test]
    fn an_iteration_publishes_the_replay_pinned_date_and_a_new_one_replaces_it() {
        let mut bridge = Bridge::for_test(|| 0);
        let session = plx_media::route::idle_session_for_test();
        assert_eq!(bridge.views().today, None, "nothing is published before the first iteration");
        // A replay frame runs on the recorded tick, and with it the pinned date: the same on every
        // run and every day, whatever the wall clock says.
        clock::set_replay(100);
        publish_frame(&mut bridge, session, false);
        assert_eq!(bridge.views().today, Some(clock::REPLAY_DATE));
        // The publication is the frame's: the next iteration replaces it, so a midnight is carried.
        bridge.publish_today(Some([2001, 2, 3]));
        assert_eq!(bridge.views().today, Some([2001, 2, 3]));
        publish_frame(&mut bridge, session, false);
        assert_eq!(bridge.views().today, Some(clock::REPLAY_DATE), "every iteration publishes anew");
    }

    #[test]
    fn the_loop_publishes_the_frame_before_the_dispatcher_frame_and_the_draw() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/app/run.rs"),
        )
        .expect("read run.rs");
        const CALL: &str = "        publish_frame(&mut app.bridge, &app.player.session, was_player);";
        assert_eq!(
            src.lines().filter(|l| *l == CALL).count(),
            1,
            "`publish_frame` must be called exactly once, at the loop body's own depth: unnested, \
             so no route skips the date",
        );
        let call = src.find(CALL.trim_start()).expect("the call");
        let frame = src.find("let supplied = match app.rec.replay_results(").expect("the dispatcher's frame");
        let draw = src.find("let (_vx, _vy, _vw, _vh) = draw(app, fr);").expect("the loop's draw");
        assert!(call < frame && call < draw, "the date is published before the frame that reads it");
    }
}

#[cfg(test)]
mod wake_input_tests {
    use super::{wake_input, ForegroundClock, ForegroundInput, WakePress};

    /// **`wake_input`'s doc table, cell for cell.** Only a press that asks to play claims with
    /// `PlayKey`, and a toggle asks to play only over a paused save; with nothing claimable (no
    /// saved clock) only PLAY still asks.
    #[test]
    fn only_a_press_that_asks_to_play_claims_playing() {
        use ForegroundClock::{Paused, Playing};
        use ForegroundInput::{DidForeground, PlayKey};
        for (press, saved, want) in [
            (WakePress::Play, Some(Paused), PlayKey),
            (WakePress::Play, Some(Playing), PlayKey),
            (WakePress::Play, None, PlayKey),
            (WakePress::Toggle, Some(Paused), PlayKey),
            (WakePress::Toggle, Some(Playing), DidForeground),
            (WakePress::Toggle, None, DidForeground),
            (WakePress::Wake, Some(Paused), DidForeground),
            (WakePress::Wake, Some(Playing), DidForeground),
            (WakePress::Wake, None, DidForeground),
        ] {
            assert_eq!(wake_input(press, saved), want, "{press:?} over {saved:?}");
        }
    }
}
