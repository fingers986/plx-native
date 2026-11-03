Warning: truncated output (original token count: 24132)
Total output lines: 2074

#[test]
fn a_hold_doubles_its_scrub_speed_every_step_up_to_the_cap() {
    use super::{scrub_speed, SCRUB_BASE, SCRUB_MAX, SCRUB_STEP_MS};
    assert_eq!(scrub_speed(0), SCRUB_BASE);
    assert_eq!(scrub_speed(SCRUB_STEP_MS - 1), SCRUB_BASE, "a step holds until its dwell is up");
    assert_eq!(scrub_speed(SCRUB_STEP_MS), 2.0 * SCRUB_BASE);
    assert_eq!(scrub_speed(2 * SCRUB_STEP_MS), 4.0 * SCRUB_BASE);
    assert_eq!(scrub_speed(3 * SCRUB_STEP_MS), SCRUB_MAX);
    assert_eq!(SCRUB_MAX, 8.0 * SCRUB_BASE, "the cap is a whole step of the ladder");
    assert_eq!(scrub_speed(u32::MAX), SCRUB_MAX, "a very long hold stays at the cap");
    let mut last = 0.0;
    for ms in (0..20_000).step_by(100) {
        let v = scrub_speed(ms);
        assert!(v >= last && v <= SCRUB_MAX, "{ms} ms -> {v}");
        last = v;
    }
}

/// #585: a cue of authored line breaks that each wrap again ran past three lines and the tail
/// was silently dropped. Every line of an ordinary long cue must survive.
#[test]
fn a_long_cue_keeps_every_line_past_three() {
    let cue = "First authored line that is long enough to wrap onto a second row\n\
               Second authored line that is long enough to wrap onto a second row\n\
               Third authored line that is long enough to wrap onto a second row";
    let lines = caption_wrap(cue, 1.0, &FM);
    assert_eq!(lines.len(), 3, "each authored line stays intact below the fixed wrap width: {lines:?}");
    assert!(lines.last().unwrap().ends_with("second row"), "{lines:?}");
}

#[test]
fn chapter_panel_retains_selection_without_borrowing_hud_focus() {
    assert_eq!(super::classic::hud_tab_state(false, GROUP_TABS, 1, 1), (false, true));
    assert_eq!(super::classic::hud_tab_state(false, GROUP_TABS, 1, 0), (false, false));
    assert_eq!(super::classic::hud_tab_state(true, GROUP_TABS, 1, 1), (true, false));
    assert_eq!(super::classic::hud_tab_state(true, GROUP_SCRUB, 1, 1), (false, false));
    // The same ownership rule covers Info, without conflating it with Chapters.
    assert_eq!(super::classic::hud_tab_state(false, GROUP_TABS, 0, 0), (false, true));
}

use super::classic::*;
use super::geometry::*;
use super::motion::*;
use super::*;
use plx_ui::widgets::{StatusKind, StatusOverlay};

/// The title at the default look (the cell classic and the trailer draw it in), `right` the cell's right edge.
fn draw_player_title(
    p: Painter,
    kicker: Kicker,
    title: *const std::os::raw::c_char,
    right: f32,
    measure: &dyn plx_machine::machine::Measure,
    paused: bool,
    clock: &plx_ui::marquee::Clock,
) {
    draw_player_title_at(p, TitleLook::default(), kicker, title, right, measure, paused, clock,
    );
}

/// **The right edge of the title's cell in the compiled classic** for a frame's slot: where the compile's
/// `title.main` text ends (`w: fill` up to the control row's occupant). The value the hand-written
/// `title_right` reported, frozen in `classic-title-right.golden.tsv`.
fn compiled_title_right(compiled: &crate::hud_format::compile::Compiled, slot: ControlSlot) -> f32 {
    use crate::hud_format::compile::DrawOp;
    let (state, skip) = frame_state(compiled, 0, slot, true, false).expect("classic has a state for every slot");
    state
        .skip_variant(skip)
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Text { bind, rect, .. } if bind == "title.main" => Some(rect.x + rect.w),
            _ => None,
        })
        .expect("the compact states carry the title")
}

#[test]
fn long_hud_titles_fit_before_every_control_slot() {
    use plx_base::fontcov::advances::ShippedMeasure;
    let measure = &ShippedMeasure;
    let long = CString::new("An extraordinarily long episode — Über den Wolken — ".repeat(20)).unwrap();
    let clock = plx_ui::marquee::Clock::new();
    for language in plx_platform::i18n::SHIPPED {
        let _language = plx_platform::i18n::language_on_this_thread_for_test(language);
        let compiled = super::loaded::load_classic("test", plx_platform::i18n::current(), measure).compiled;
        let slots = [
            ControlSlot::Discs(Controls::NONE),
            ControlSlot::Discs(Controls { previous: true, next: true, restart: true,
            }),
            slot_for(Some(marker(MarkerKind::Intro, false)), false, NextEpisodeMode::Countdown,
            ),
            slot_for(Some(marker(MarkerKind::Credits, true)), true, NextEpisodeMode::Countdown,
            ),
        ];
        for slot in slots {
            let right = compiled_title_right(&compiled, slot);
            let items = geometry(&compiled, 0, slot, true, false).items;
            let mut row = HudMotion::new();
            for focused in 0..slot.items() {
                for _ in 0..30 {
                    row.pop.step(Some(focused as usize), 1.0 / 60.0);
                    for i in 0..slot.items() {
                        let Some(rect) = items[i as usize] else { continue;
                        };
                        assert!(right + theme::space::MD <= rect.scaled(row.scale(i)).x + 0.001);
                    }
                    for kicker in [Kicker::Episode(long.as_ptr()), Kicker::Context(long.as_ptr()),
                    ] {
                        let runs = plx_ui::draw_census::capture_runs(|| {
                            draw_player_title(Painter::recording(), kicker, long.as_ptr(),
                                right, measure, false, &clock,
                            );
                        });
                        assert_eq!(runs.len(), 2);
                        for run in runs {
                            assert!(run.text.ends_with('…'), "{}: expected ellipsis: {run:?}", language.tag());
                            assert_eq!(run.at.x, SB_X);
                            assert!(measure.width_str(&run.text, run.size, run.bold) <= right - SB_X + 1.0,
                                "{}: fitted title exceeds the control boundary: {run:?}", language.tag());
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn short_hud_titles_keep_the_original_position_typography_and_ink() {
    let clock = plx_ui::marquee::Clock::new();
    for kicker in [Kicker::Episode(c"S1 · E1 · Pilot".as_ptr()), Kicker::Context(c"Trailer".as_ptr()),
    ] {
        let original = plx_ui::draw_census::capture_runs(|| {
            draw_title(Painter::recording(), kicker, c"Short title".as_ptr());
        });
        for paused in [false, true] {
            let bounded = plx_ui::draw_census::capture_runs(|| {
                draw_player_title(Painter::recording(), kicker, c"Short title".as_ptr(),
                    compiled_title_right(&super::loaded::classic_for_test(), ControlSlot::Discs(Controls::NONE),
                    ), &FM, paused, &clock,
                );
            });
            assert_eq!(original, bounded);
        }
    }
}

#[test]
fn paused_title_reveals_once_and_resume_restores_ellipsis() {
    let clock = plx_ui::marquee::Clock::new();
    let long = c"A very long movie title that should slowly reveal its final words after pausing";
    let right = compiled_title_right(&super::loaded::classic_for_test(), ControlSlot::Discs(Controls::NONE),
    );
    let paint = |paused| {
        plx_ui::draw_census::capture_runs(|| {
        draw_player_title(Painter::recording(), Kicker::Context(c"Movie".as_ptr()),
            long.as_ptr(), right, &FM, paused, &clock,
            );
        })
    };
    let start = paint(true);
    assert!(start[1].text.ends_with('…'));
    plx_machine::idle::frame_begin(2.0);
    let moving = paint(true);
    assert!(moving[1].at.x < SB_X);
    plx_machine::idle::frame_begin(30.0);
    let ended = paint(true);
    assert_eq!(ended[1].at.x, SB_X - (FM.width(long, HUD_TITLE_SZ, true) - (right - SB_X)));
    assert_eq!(paint(true)[1].at.x, ended[1].at.x, "the title must stay at the end");
    assert!(paint(false)[1].text.ends_with('…'), "resume restores a static ellipsis");
    assert!(paint(true)[1].text.ends_with('…'), "a later pause starts from the beginning");
}

/// An Arabic title's end is on its LEFT: the paused reveal moves it right, from its first words
/// on the window's right edge to its last words on the window's left edge, then rests there.
#[test]
fn a_paused_right_to_left_title_reveals_its_end_to_the_left() {
    let _g = plx_base::testlock::serial();
    let clock = plx_ui::marquee::Clock::new();
    let long = c"فيلم طويل جدا يجب أن يكشف كلماته الأخيرة ببطء بعد الإيقاف المؤقت للتشغيل";
    let right = compiled_title_right(&super::loaded::classic_for_test(), ControlSlot::Discs(Controls::NONE),
    );
    let (full, window) = (FM.width(long, HUD_TITLE_SZ, true), right - SB_X);
    assert!(full > window, "the fixture title overflows");
    let paint = |paused| {
        plx_ui::draw_census::capture_runs(|| {
        draw_player_title(Painter::recording(), Kicker::Context(c"Movie".as_ptr()),
            long.as_ptr(), right, &FM, paused, &clock,
            );
        })
    };
    let _ = paint(true);
    plx_machine::idle::frame_begin(2.0);
    let moving = paint(true);
    let elapsed = 2000.0 - plx_ui::marquee::TITLE.hold_ms;
    let off = elapsed / 1000.0 * plx_ui::marquee::SPEED;
    assert_eq!(moving[1].at.x, SB_X + window - full + off, "first words flush right, moving right");
    plx_machine::idle::frame_begin(60.0);
    assert_eq!(paint(true)[1].at.x, SB_X, "the last words rest on the window's left edge");
    assert_eq!(paint(true)[1].at.x, SB_X, "and stay");
}

/// **The overscan audit grades the compiled classic's own rects**, and the report carries what the page
/// places from: the occupant's items (an enabled disc has a rect, an absent one none) and the bar's
/// x-to-fraction map. (Was `frame_geometry_is_the_classic_layouts_own_rects` and
/// `geometry_fills_the_items_and_tabs_it_was_asked_for`, whose subjects were the hand-written
/// `frame_geometry`/`geometry`; the values they held are `classic-geometry.golden.tsv`'s.)
#[test]
fn the_overscan_audit_grades_the_compiled_classic_rects() {
    let g = audit_geometry();
    assert_eq!(g.playbar, Rect::new(SB_X, SB_Y, sb_w(), SB_H));
    assert_eq!(g.row, ctrl_row_hit_rect());
    assert_eq!(g.row_anchor, RowAnchor { right: CTRL_RIGHT, top: CTRL_Y, h: CTRL_H });
    let mut audited = Vec::new();
    overscan_rects(&mut audited);
    assert_eq!(
        audited,
        vec![
            ("player scrubber", g.playbar),
            ("player control row", g.row),
            ("player bottom tab row", g.tab_row),
            ("player title / elapsed clock", g.title),
        ]
    );
    assert_eq!(
        BTN_N, 3,
        "the built-in row has only Subtitle, Audio and More"
    );
    assert!(g.items[..BTN_N as usize].iter().all(Option::is_some),
        "all built-in discs have rects"
    );
    // the bar maps an x to a fraction of ITS width, clamped
    assert_eq!(g.scrub_frac(g.playbar.x - 50.0), 0.0);
    assert_eq!(g.scrub_frac(g.playbar.x + g.playbar.w * 0.5), 0.5);
    assert_eq!(g.scrub_frac(g.playbar.x + g.playbar.w + 50.0), 1.0);
}

/// The host tests' text capability (half an em per byte).
const FM: plx_ui::fixture::FixtureMeasure = plx_ui::fixture::FixtureMeasure;

/// **The transport's clocks are resident before the draw, this second's and the next one's.**
/// On the TV the draw met each new clock string cold AFTER the back-buffer wait (`clear:12.4 …
/// textx2:1.1`, once a second under an open Tracks/More popover), which is what pushed those
/// frames past 20 ms. `ClockWarm::queue` is what `PlayerScreen::prepare` calls; the drain is
/// the presenting side's (`app::run::prepare_window`), ahead of the clear.
#[test]
fn the_transport_clocks_are_rasterised_before_the_draw_and_one_second_ahead() {
    let _g = plx_base::testlock::serial();
    plx_gfx::text::reset_prewarm_for_test();
    let sz = theme::size::CAPTION;
    let mut warm = ClockWarm::default();
    // 1:23.4 into a 2:00 item: the draw shows "1:23" (bold) and "-0:36" (regular).
    let (pos, dur) = (83_400_000_000, 120_000_000_000);
    assert_eq!((fmt_time(pos, false), fmt_time(dur - pos, true)), ("1:23".into(), "-0:36".into()));
    let at = |pos_ns: i64| HudModel { playpos_ns: pos_ns, duration_ns: dur, ..HudModel::default() };
    warm.queue(&at(pos));
    // `prepare` may run on a frame the loop then does not present: it records, uploads nothing.
    assert!(plx_gfx::text::prewarm_pending(), "prepare queued the clocks");
    assert!(!plx_gfx::text::prewarm_resident_for_test(b"1:23", sz, 1), "prepare itself uploaded");
    // Six strings against a three-string frame budget: the second presented frame finishes them.
    while plx_gfx::text::prewarm_pending() {
        plx_ui::panel_motion::PanelMotion::drain_queued_text_for_test();
    }
    for (text, bold) in [("1:23", 1), ("-0:36", 0), ("1:24", 1), ("-0:35", 0)] {
        assert!(
            plx_gfx::text::prewarm_resident_for_test(text.as_bytes(), sz, bold),
            "{text:?} (bold={bold}) was left for the draw to rasterise after the back-buffer wait"
        );
    }
    // …and the elapsed clock at the size it GROWS to during a scrub with no card
    // (`scrub_preview::clock_faces`): a cold rung there is a font open plus a rasterisation in
    // the middle of a hold.
    let grown = crate::scrub_preview::CLOCK_GROWN_SZ;
    for text in ["1:23", "1:24"] {
        assert!(
            plx_gfx::text::prewarm_resident_for_test(text.as_bytes(), grown, 1),
            "{text:?} at the grown size ({grown}) was left cold"
        );
    }

    // The draw empties the queue every frame, so the offer is repeated every prepared frame —
    // and an undrained offer must come back rather than be remembered as done.
    plx_gfx::text::reset_prewarm_for_test();
    warm.queue(&at(pos + 16_000_000));
    assert!(plx_gfx::text::prewarm_pending(), "the same second was not offered again");

    // The end of the item: nothing below zero, and no string the clock cannot show.
    plx_gfx::text::reset_prewarm_for_test();
    warm.queue(&at(dur + 5_000_000_000));
    plx_ui::panel_motion::PanelMotion::drain_queued_text_for_test();
    assert!(plx_gfx::text::prewarm_resident_for_test(b"-0:00", sz, 0));
    assert!(plx_gfx::text::prewarm_resident_for_test(b"2:05", sz, 1));
    plx_gfx::text::reset_prewarm_for_test();
}

use crate::skip_pill::SkipAction;
use plx_data::metadata::{Marker, MarkerKind};
use plx_media::route::NextEpisodeMode;

/// **The image-subtitle display set is a render, and it says how much of one** (§8.3 rule (c)):
/// one texture per rect, the SOURCE pixels behind them, and nothing left claimed once the set
/// is released. The bytes cannot be derived from the screen rects the cache keeps — those are
/// where each bitmap lands, scaled into the video rect — which is why the count is carried.
#[test]
fn the_image_subtitle_set_reports_its_own_textures_and_bytes() {
    let mut subs = SubtitleBitmaps::stub(&[(720, 120), (300, 80)]);
    let r = subs.render_report();
    assert_eq!(r.textures, 2, "a two-rect display set is two textures");
    assert_eq!(r.bytes, (720 * 120 + 300 * 80) * 4);
    subs.release();
    assert_eq!(
        subs.render_report(),
        plx_ui::frame::RenderReport::NONE,
        "a released set holds nothing and must stop claiming bytes"
    );
}

fn marker(kind: MarkerKind, final_seg: bool) -> Marker {
    Marker {
        kind,
        start_ms: 1_000,
        end_ms: 2_000,
        final_seg,
    }
}

fn seg(start_ms: i64, end_ms: i64, final_seg: bool) -> Marker {
    Marker {
        kind: MarkerKind::Credits,
        start_ms,
        end_ms,
        final_seg,
    }
}

// A rail 1000px wide over a 1000ms item: 1px per ms, so an offset reads straight off the x.
const SX: f32 = 100.0;
const SW: f32 = 1000.0;
const DUR: i64 = 1_000;

/// The classic discs' rects, as the compile reports them (what the page places and hit-tests from).
fn disc_rects() -> [Option<Rect>; MAX_ROW_ITEMS] {
    geometry(&super::loaded::classic_for_test(), 0, ControlSlot::Discs(Controls::NONE), false, false,
    ).items
}

/// **Q5: the discs stand `CONTROL_GAP` (20) apart, not the old 22**, and the row's width is the three discs
/// and two of those gaps.
#[test]
fn the_control_row_gap_is_the_shared_control_gap() {
    let discs = disc_rects();
    let at = |i: i32| discs[i as usize].expect("a present disc");
    assert_eq!(plx_ui::widgets::CONTROL_GAP, 20.0);
    assert_eq!(at(BTN_AUDIO).x - (at(BTN_SUBTITLES).x + BTN_S), 20.0);
    assert_eq!(ctrl_row_w(), 3.0 * BTN_S + 2.0 * 20.0);
}

/// **The bottom tabs are one run `space::SM` apart**, read off the compile's tab pills; with no chapters
/// there is one pill, at the same place, and no second.
#[test]
fn the_bottom_tabs_stand_one_sm_apart_and_are_drawn_where_they_are_hit() {
    let compiled = super::loaded::classic_for_test();
    let two = geometry(&compiled, 0, ControlSlot::Discs(Controls::NONE), true, false,
    ).tabs;
    let (info, chapters) = (two[0].expect("Info"), two[1].expect("Chapters"));
    assert_eq!(info.x, SB_X);
    assert_eq!(chapters.x - (info.x + info.w), theme::space::SM);
    let one = geometry(&compiled, 0, ControlSlot::Discs(Controls::NONE), false, false,
    ).tabs;
    assert_eq!(one[0], Some(info), "with no chapters there is one pill, at the same place");
    assert_eq!(one[1], None, "…and no second");
}

/// The row is right-anchored and does not overlap itself: the LAST disc ends exactly at
/// `CTRL_RIGHT`, each disc sits a full `CONTROL_GAP` clear of its neighbour, and the whole span is
/// `ctrl_row_w`.
#[test]
fn the_disc_row_is_right_anchored_and_spans_ctrl_row_w() {
    let discs = disc_rects();
    let at = |i: i32| discs[i as usize].expect("a present disc");
    assert_eq!(
        at(BTN_N - 1).x + BTN_S,
        CTRL_RIGHT,
        "last disc must end at the row's edge"
    );
    for i in (BTN_SUBTITLES + 1)..BTN_N {
        assert_eq!(
            at(i).x - (at(i - 1).x + BTN_S),
            plx_ui::widgets::CONTROL_GAP,
            "gap before disc {i}"
        );
    }
    assert_eq!(
        CTRL_RIGHT - at(BTN_SUBTITLES).x,
        ctrl_row_w(),
        "drawn span vs the row's floor width"
    );
}

/// The control row's precedence, which used to live in the ORDER of five separate if-chains
/// across `player_hud` and `app.rs` — the draw, the OK handler, the pointer handler, the
/// LEFT/RIGHT clamp and `icon_hit` — with nothing keeping them in step and nothing to test.
#[test]
fn the_control_row_picks_the_most_specific_occupant() {
    // no segment under the playhead → the ordinary Subtitles + Audio pair
    assert!(slot_for(None, false, NextEpisodeMode::Countdown).is_discs());
    assert!(
        slot_for(None, true, NextEpisodeMode::Countdown).is_discs(),
        "a queued successor alone changes nothing"
    );
    assert_eq!(slot_for(None, true, NextEpisodeMode::Countdown).items(), BTN_N);
    assert_eq!(
        slot_for(None, true, NextEpisodeMode::Countdown).primary_btn(),
        BTN_SUBTITLES,
        "the discs open on Subtitles"
    );

    // an intro is always Skip, successor or not — "what's next" is an end-of-episode idea
    for has_next in [false, true] {
        let slot = slot_for(Some(marker(MarkerKind::Intro, false)), has_next, NextEpisodeMode::Countdown,
        );
        assert!(matches!(slot, ControlSlot::Skip(p) if p.kind == MarkerKind::Intro));
        assert_eq!(slot.items(), 1, "a Skip pill is the row's only item");
        assert_eq!(
            slot.primary_btn(),
            0,
            "…so it is also the one focus lands on"
        );
    }

    // credits WITH somewhere to go → Up Next outranks Skip Credits…
    let up = slot_for(Some(marker(MarkerKind::Credits, true)), true, NextEpisodeMode::Countdown,
    );
    assert!(matches!(up, ControlSlot::UpNext(_)));
    // …and it is the ONE stand-in with a pair, whose primary is the RIGHT-hand item. Asserted
    // together because they are one fact: an `items()` of 2 with a `primary_btn()` of 0 would
    // park the ring on Watch Credits, and `up_next::countdown_may_run` would then disarm the
    // countdown on the frame after it armed — a timer that visibly never runs.
    assert_eq!(up.items(), 2, "Watch Credits + Next Episode");
    assert_eq!(up.primary_btn(), crate::up_next::BTN_NEXT);
    // …and WITHOUT (a show's last episode) it stays Skip Credits, which is the whole reason
    // both still exist
    assert!(matches!(
        slot_for(Some(marker(MarkerKind::Credits, true)), false, NextEpisodeMode::Countdown),
        ControlSlot::Skip(p) if p.kind == MarkerKind::Credits
    ));
}

/// Up Next is only offered for a credits marker that runs to the end of the item (`final`).
/// A mid-item credits segment with a successor queued (a post-credits scene follows) is a
/// plain Skip Credits pill that seeks past it, not a countdown into the next episode.
#[test]
fn up_next_needs_a_final_credits_marker() {
    let mid = slot_for(Some(marker(MarkerKind::Credits, false)), true, NextEpisodeMode::Countdown,
    );
    assert!(
        matches!(mid, ControlSlot::Skip(p) if p.kind == MarkerKind::Credits
            && p.action == SkipAction::Seek(2_000 * 1_000_000)),
        "a non-final credits marker offers Skip Credits even with a successor queued"
    );
    assert!(matches!(slot_for(Some(marker(MarkerKind::Credits, true)), true, NextEpisodeMode::Countdown), ControlSlot::UpNext(_)));
}

/// **Next episode: After credits / Off.** During the credits of an episode with a successor the
/// control row offers nothing — no Up Next tile (so no countdown and no HUD raise, both of
/// which key off the slot), and no Skip Credits pill. Everything else keeps its occupant.
#[test]
fn after_credits_and_off_offer_nothing_during_the_credits() {
    for mode in [NextEpisodeMode::AfterCredits, NextEpisodeMode::Off] {
        assert!(slot_for(Some(marker(MarkerKind::Credits, true)), true, mode).is_discs(), "{mode:?}");
        // a show's last episode and a movie keep the Skip Credits pill, which finishes the item
        assert!(matches!(
            slot_for(Some(marker(MarkerKind::Credits, true)), false, mode),
            ControlSlot::Skip(p) if p.action == SkipAction::Finish
        ));
        // a mid-item credits segment and an intro are skippable as ever
        assert!(matches!(
            slot_for(Some(marker(MarkerKind::Credits, false)), true, mode),
            ControlSlot::Skip(p) if p.action == SkipAction::Seek(2_000 * 1_000_000)
        ));
        assert!(matches!(slot_for(Some(marker(MarkerKind::Intro, false)), true, mode), ControlSlot::Skip(_)));
    }
    assert!(matches!(
        slot_for(Some(marker(MarkerKind::Credits, true)), true, NextEpisodeMode::Countdown),
        ControlSlot::UpNext(_)
    ));
}

/// A `final` credits segment runs to the end of the item, so skipping it FINISHES rather than
/// seeks — seeking to its stated end would race the decoder against its own last frames.
#[test]
fn only_a_final_credits_segment_finishes_the_item() {
    let fin = match slot_for(Some(marker(MarkerKind::Credits, true)), false, NextEpisodeMode::Countdown,
    ) {
        ControlSlot::Skip(p) => p.action,
        _ => unreachable!(),
    };
    assert_eq!(fin, SkipAction::Finish);

    // a mid-item credits segment (a post-credits scene follows) seeks past it and plays on
    let mid = match slot_for(Some(marker(MarkerKind::Credits, false)), false, NextEpisodeMode::Countdown,
    ) {
        ControlSlot::Skip(p) => p.action,
        _ => unreachable!(),
    };
    assert_eq!(mid, SkipAction::Seek(2_000 * 1_000_000));
    // …as does an intro, `final` flag or not (PMS only sets it on credits)
    let intro = match slot_for(Some(marker(MarkerKind::Intro, true)), false, NextEpisodeMode::Countdown,
    ) {
        ControlSlot::Skip(p) => p.action,
        _ => unreachable!(),
    };
    assert_eq!(intro, SkipAction::Seek(2_000 * 1_000_000));
}

/// The ring only goes back to the scrubber on the EDGE where a stand-in vanished under it.
/// As a steady state (`is_discs() && focused`, which is how it shipped) it fired on every
/// frame the user had walked UP to the Subtitles/Audio discs, so focus could not rest there
/// and OK on a disc never reached the track menu — invisible to the pointer path, and so to
/// the on-device suite.
#[test]
fn only_a_vanishing_standin_takes_the_focus_ring_back() {
    let discs = slot_for(None, false, NextEpisodeMode::Countdown);
    let skip = slot_for(Some(marker(MarkerKind::Intro, false)), false, NextEpisodeMode::Countdown,
    );

    // the edge it exists for: the pill was there last frame, the discs are back, ring on the row
    assert!(standin_left_the_ring(true, discs, true));

    // the steady state it must NOT fire on: the discs were already there, so nothing vanished
    assert!(
        !standin_left_the_ring(false, discs, true),
        "walking UP to the discs is not a lost stand-in"
    );

    // nothing to take back if the ring is elsewhere, or if a stand-in still owns the row
    assert!(!standin_left_the_ring(true, discs, false));
    assert!(!standin_left_the_ring(true, skip, true));
}
fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}
fn assert_rect(r: Rect, x: f32, y: f32, w: f32, h: f32) {
    assert!(
        close(r.x, x) && close(r.y, y) && close(r.w, w) && close(r.h, h),
        "got ({}, {}, {}, {}), want ({x}, {y}, {w}, {h})",
        r.x,
        r.y,
        r.w,
        r.h
    );
}

/// **Every tone has an ink, the first is the white every earlier build drew, and each rung
/// under it is strictly darker** — the ladder's whole promise is "down is dimmer", and the
/// table is indexed by position, so a rung added to `SubtitleTone::LADDER` without an ink
/// here would silently draw white. Opaque and achromatic throughout: a tone gives up light,
/// never coverage (the outline depends on it) and never hue (it tints image subtitles).
#[test]
fn the_subtitle_tones_are_a_strictly_darkening_opaque_grey_ladder_from_white() {
    use plx_plex::plex::session::SubtitleTone;
    assert_eq!(theme::SUBTITLE_INKS.len(), SubtitleTone::LADDER.len());
    assert_eq!(subtitle_ink_for(SubtitleTone::White), [1.0, 1.0, 1.0, 1.0]);
    let mut prev = f32::MAX;
    for tone in SubtitleTone::LADDER {
        let ink = subtitle_ink_for(tone);
        assert!(ink[0] == ink[1] && ink[1] == ink[2], "{tone:?} has a hue");
        assert_eq!(ink[3], 1.0, "{tone:?} is not opaque");
        assert!(ink[0] < prev, "{tone:?} is not darker than the rung above it");
        prev = ink[0];
    }
    // the darkest rung still has to be READ over the 0.85 black outline it is drawn on
    assert!(prev > 0.25, "the darkest tone has sunk into its own outline");
}

/// A 1080p-authored PGS rect must land EXACTLY where it always did — the scale is identity,
/// not merely close to it, or every Blu-ray subtitle in the library moves a pixel.
#[test]
fn image_sub_1080p_canvas_is_identity() {
    assert_rect(
        sub_screen_rect((300, 900, 1320, 96), 1920, 1080),
        300.0,
        900.0,
        1320.0,
        96.0,
    );
}

/// The bug this unit exists for: a DVD VobSub rip is authored on a 720×480 canvas, so drawn
/// 1:1 it is a postage stamp covering the top-left third of the panel. Scaled from its own
/// canvas it spans the picture and sits at the bottom, exactly like the PGS case above.
#[test]
fn image_sub_dvd_canvas_fills_the_panel() {
    // a full-width, near-bottom VobSub line
    assert_rect(
        sub_screen_rect((0, 400, 720, 60), 720, 480),
        0.0,
        900.0,
        1920.0,
        135.0,
    );
    // and an inset one keeps its position within the picture
    let r = sub_screen_rect((180, 240, 360, 48), 720, 480);
    assert_rect(r, 480.0, 540.0, 960.0, 108.0);
    assert!(
        close(r.x + r.w * 0.5, SCR_W * 0.5),
        "a canvas-centred rect stays centred"
    );
}

/// PAL (720×576) and 4K PGS (3840×2160) are the other two canvases in the wild; the 4K one
/// scales DOWN, which the old 1:1 path drew at double size and half off-screen.
#[test]
fn image_sub_pal_and_4k_canvases() {
    assert_rect(
        sub_screen_rect((0, 480, 720, 72), 720, 576),
        0.0,
        900.0,
        1920.0,
        135.0,
    );
    assert_rect(
        sub_screen_rect((600, 1800, 2640, 192), 3840, 2160),
        300.0,
        900.0,
        1320.0,
        96.0,
    );
}

/// A decoder that never declares a canvas (0) must fall back to 1:1 — the behaviour of the
/// whole path before this change — rather than divide by zero or blank the cue.
#[test]
fn image_sub_unknown_canvas_stays_1to1() {
    assert_rect(
        sub_screen_rect((300, 900, 1320, 96), 0, 0),
        300.0,
        900.0,
        1320.0,
        96.0,
    );
    assert_rect(
        sub_screen_rect((300, 900, 1320, 96), -1, 1080),
        300.0,
        900.0,
        1320.0,
        96.0,
    );
}

/// The HUD lift: image cues rise just enough to clear the transport, and only then.
#[test]
fn image_sub_lifts_only_over_the_transport() {
    let dialogue = Rect::new(300.0, 900.0, 1320.0, 135.0); // bottom 1035, overhangs
                                                           // transport down: nothing moves, ever
    assert_eq!(hud_lift([dialogue].into_iter(), false, HudBand::CLASSIC), 0.0);
    // transport up: a bottom-anchored cue rises exactly its overhang, landing ON the ceiling
    let lift = hud_lift([dialogue].into_iter(), true, HudBand::CLASSIC);
    assert!(
        close(1035.0 - lift, HudBand::CLASSIC.ceil_y),
        "lifted bottom should sit at the ceiling"
    );
    // a sign at the top of frame already clears it and must stay put
    let sign = Rect::new(100.0, 100.0, 200.0, 100.0);
    assert_eq!(hud_lift([sign].into_iter(), true, HudBand::CLASSIC), 0.0);
   …9132 tokens truncated…ule_edge_is_fixed_and_a_taller_block_grows_away_from_it() {
    assert_eq!(CAPSULE_BOTTOM_Y, 876.0);
    for n in [1usize, 3, 6] {
        let lay = caption_layout(n, 1.0, SUB_BASE_Y, CAPTION_FLOOR_Y);
        assert!(
            lay.top >= CAPSULE_BOTTOM_Y,
            "{n} lines: block top {} rises into the capsule (bottom {CAPSULE_BOTTOM_Y})",
            lay.top
        );
    }
    assert_eq!(caption_layout(6, 1.0, SUB_BASE_Y, CAPTION_FLOOR_Y).top, 876.0);
}

mod playback_navigation_tests {
    use super::*;

    #[test]
    fn queue_neighbors_do_not_add_controls_to_the_builtin_row() {
        let compiled = super::super::loaded::classic_for_test();
        let rects = |c: Controls| geometry(&compiled, 0, ControlSlot::Discs(c), false, false).items;
        let (episode_rects, none_rects) = (rects(Controls::for_item(true, true, true, true)), rects(Controls::NONE),
        );
        for i in 0..BTN_N as usize {
            assert_eq!(
                episode_rects[i], none_rects[i],
                "episode neighbors do not alter disc {i}"
            );
            assert!(episode_rects[i].is_some());
        }
        assert_eq!(
            Controls::for_item(true, true, true, true).primary(),
            BTN_SUBTITLES
        );
        assert_eq!(
            Controls::for_item(true, true, true, true).command(BTN_SUBTITLES), Some(crate::player_hud::command::HudCommand::OpenTracks(
                crate::player_hud::command::TrackTab::Subtitles
            )));
    }

    #[test]
    fn episode_pair_is_absent_for_movies_extras_and_single_items() {
        for controls in [Controls::for_item(false, true, true, true),
            Controls::for_item(true, false, false, true),
        ] {
            assert_eq!(controls, Controls::NONE);
            assert_eq!(controls.primary(), BTN_SUBTITLES);
        }
        let previous_only = Controls::for_item(true, true, false, false);
        assert!(previous_only.previous && !previous_only.next);
        assert_eq!(
            previous_only.primary(),
            BTN_SUBTITLES,
            "queue navigation does not take a built-in focus slot"
        );
    }
}

/// **Under `builtin:classic`, the row resolved through the compile is the three-control table.**
/// Queue state does not add Previous/Next to the built-in row; those commands remain available to
/// package-defined controls and folded More rows.
#[test]
fn classic_resolved_through_the_compile_is_classics_table() {
    use super::command::HudCommand;
    use super::model::HudModel;
    use plx_base::fontcov::advances::ShippedMeasure;
    let hud = super::loaded::load_classic("test", plx_platform::i18n::english(), &ShippedMeasure);
    let compiled = &*hud.compiled;
    let model = HudModel::default();
    for episode in [false, true] {
        for previous in [false, true] {
            for next in [false, true] {
                for restart in [false, true] {
                    let controls = Controls::for_item(episode, previous, next, restart);
                    let slot = ControlSlot::Discs(controls);
                    for i in -1..=BTN_N + 2 {
                        let want: Option<HudCommand> = controls.command(i);
                        assert_eq!(row_command(compiled, 0, slot, &model, i), want, "{controls:?} item {i}");
                        assert_eq!(row_enabled(compiled, 0, slot, &model, i), controls.enabled(i), "{controls:?} item {i}");
                    }
                    assert_eq!(row_primary(compiled, 0, slot, &model), controls.primary(), "{controls:?}");
                }
            }
        }
    }
}

/// The playbar's package options (`knob`, `clocks`, `mark`): extended in place, with classic's values
/// the default, so the trailer's bar and the classic HUD's bar are the frames they were.
mod playbar_options {
    use super::*;
    use crate::hud_format::compile::{ClockMode, KnobMode, MarkMode};

    fn bar(knob: Knob, mark: TransportMark) -> Playbar {
        Playbar { pos_ns: 600_000_000_000, dur_ns: 1_200_000_000_000, knob, mark, now: 0, clock_grow: 0.0,
        }
    }
    fn census(b: Playbar, look: PlaybarLook) -> Vec<(u64, Rect)> {
        plx_ui::draw_census::capture(|| draw_playbar_look(Painter::recording(), b, look, &FM))
    }
    fn runs(b: Playbar, look: PlaybarLook) -> Vec<plx_ui::draw_census::Run> {
        plx_ui::draw_census::capture_runs(|| draw_playbar_look(Painter::recording(), b, look, &FM))
    }

    #[test]
    fn the_default_look_is_classics() {
        let look = PlaybarLook::default();
        assert_eq!((look.knob, look.clocks, look.mark), (KnobMode::Tick, ClockMode::FollowHead, MarkMode::AfterElapsed));
        for (knob, mark) in [(Knob::Tick, TransportMark::Pause), (Knob::Focused, TransportMark::Play), (Knob::Held, TransportMark::None),
        ] {
            let b = bar(knob, mark);
            let classic = plx_ui::draw_census::capture(|| draw_playbar(Painter::recording(), b, &FM));
            assert_eq!(classic, census(b, look), "{knob:?}");
        }
    }

    #[test]
    fn an_always_knob_replaces_the_tick_at_rest() {
        let tick = census(bar(Knob::Tick, TransportMark::None), PlaybarLook::default());
        let always = census(bar(Knob::Tick, TransportMark::None), PlaybarLook { knob: KnobMode::Always, ..PlaybarLook::default() },
        );
        let thin = |v: &[(u64, Rect)]| v.iter().any(|(_, r)| (r.w - 3.0).abs() < 0.01);
        let round = |v: &[(u64, Rect)]| {
            v.iter().any(|(_, r)| (r.w - 18.0).abs() < 0.01 && (r.h - 18.0).abs() < 0.01)
        };
        assert!(thin(&tick) && !round(&tick), "classic rests as a tick: {tick:?}");
        assert!(round(&always) && !thin(&always), "`always` rests as a knob: {always:?}");
        // a knob already shown (focused or held) is unchanged by the option
        for knob in [Knob::Focused, Knob::Held] {
            let b = bar(knob, TransportMark::None);
            assert_eq!(census(b, PlaybarLook::default()), census(b, PlaybarLook { knob: KnobMode::Always, ..PlaybarLook::default() }));
        }
    }

    #[test]
    fn ends_clocks_sit_at_the_bars_two_ends() {
        let look = PlaybarLook { clocks: ClockMode::Ends, ..PlaybarLook::default() };
        let b = bar(Knob::Tick, TransportMark::None);
        let elapsed = |look| {
            runs(b, look).into_iter().find(|r| r.align == 1).expect("the elapsed clock")
        };
        let follow = elapsed(PlaybarLook::default());
        let ends = elapsed(look);
        let l = look.track.x;
        assert!((follow.at.x - (l + look.track.w * 0.5)).abs() < 1.0, "classic: under the playhead, {:?}", follow.at);
        assert!((ends.at.x - ends.at.w * 0.5 - l).abs() < 1.0, "`ends`: the clock's left edge is the bar's start, {:?}", ends.at);
        let rem = runs(b, look).into_iter().find(|r| r.align == 2).expect("the remaining clock");
        assert!((rem.at.x - (l + look.track.w)).abs() < 0.01, "remaining ends at the bar's end");
        // near the end of the item the classic remaining clock gives way; `ends` keeps both apart
        let late = Playbar { pos_ns: 1_199_000_000_000, ..b };
        assert!(runs(late, PlaybarLook::default()).iter().all(|r| r.align != 2), "classic hides it");
        assert!(runs(late, look).iter().any(|r| r.align == 2), "`ends` never meets it");
    }

    #[test]
    fn a_busy_only_mark_draws_the_spinner_and_nothing_else() {
        let glyphs = |b: Playbar, look| {
            plx_ui::draw_census::capture_icons(|| {
                draw_playbar_look(Painter::recording(), b, look, &FM)
            })
        };
        let busy_only = PlaybarLook { mark: MarkMode::BusyOnly, ..PlaybarLook::default() };
        for mark in [TransportMark::Pause, TransportMark::Play, TransportMark::Rewind, TransportMark::FastForward,
        ] {
            let b = bar(Knob::Tick, mark);
            assert_eq!(glyphs(b, PlaybarLook::default()).len(), 1, "classic draws the {mark:?} glyph");
            assert!(glyphs(b, busy_only).is_empty(), "`busy_only` suppresses it: {mark:?}");
        }
        let working = bar(Knob::Tick, TransportMark::Working);
        assert_eq!(census(working, busy_only), census(working, PlaybarLook::default()), "the spinner stays");
    }
}

// --- PR8 stage 2a: what the screen reads to open, travel and close a section ---

fn compiled_classic() -> std::sync::Arc<crate::hud_format::compile::Compiled> {
    super::loaded::load_classic("0.0.0-test", plx_platform::i18n::english(), &plx_base::fontcov::advances::ShippedMeasure,
    ).compiled
}

fn compiled_reference() -> std::sync::Arc<crate::hud_format::compile::Compiled> {
    let text = include_str!("../hud_format/fixtures/reference.plxhud.json");
    super::loaded::load(text.as_bytes(), "0.0.0-test", plx_platform::i18n::english(), &plx_base::fontcov::advances::ShippedMeasure,
    )
        .expect("the reference loads")
        .compiled
}

#[test]
fn an_open_section_is_looked_up_over_the_discs_whatever_the_slot_is() {
    use crate::hud_format::compile::OPEN_CHAPTERS;
    let compiled = compiled_classic();
    let skip = slot_for(Some(marker(MarkerKind::Intro, false)), false, NextEpisodeMode::Countdown,
    );
    assert!(matches!(skip, ControlSlot::Skip(_)), "the fixture is a Skip occupant");
    let (compact, _) = frame_state(&compiled, 0, skip, true, false).expect("classic has the Skip state");
    let (open, _) = frame_state(&compiled, OPEN_CHAPTERS, skip, true, false).expect("an open section has a state under a Skip slot");
    assert_ne!(compact.key, open.key, "the open state is not the Skip one");
    assert_eq!(open.key.open, OPEN_CHAPTERS);
    assert_eq!(open.key.slot, crate::hud_format::reach::Slot::Discs, "D7: the stand-in waits behind the section");
}

#[test]
fn the_opener_of_a_section_is_its_tab_in_classic_and_its_disc_in_the_reference() {
    use super::command::Section;
    let classic = compiled_classic();
    let (state, _) = frame_state(&classic, 0, ControlSlot::Discs(Controls::NONE), true, false).unwrap();
    assert_eq!(opener_elem(state, Section::Info), Some(ELEM_TAB_BASE));
    assert_eq!(opener_elem(state, Section::Chapters), Some(ELEM_TAB_BASE + 1));
    // the open state keeps the tab that opened it (classic's tabs are always there)
    let (open, _) = frame_state(&classic, crate::hud_format::compile::OPEN_CHAPTERS, ControlSlot::Discs(Controls::NONE), true, false,
    ).unwrap();
    assert_eq!(opener_elem(open, Section::Chapters), Some(ELEM_TAB_BASE + 1));

    let reference = compiled_reference();
    let (state, _) = frame_state(&reference, 0, ControlSlot::Discs(Controls::NONE), true, false,
    ).unwrap();
    let chapters = opener_elem(state, Section::Chapters).expect("the reference opens Chapters with a disc in its row");
    assert!(chapters >= super::state::ELEM_CONTROL_BASE, "a row disc, not a tab: {chapters:#x}");
    assert_ne!(chapters, opener_elem(state, Section::Info).unwrap());
    assert_eq!(row_command(&reference, 0, ControlSlot::Discs(Controls::NONE), &HudModel { chapters: true, ..HudModel::default() }, (chapters - super::state::ELEM_CONTROL_BASE) as i32),
        Some(super::command::HudCommand::ToggleSection(Section::Chapters)));
}

#[test]
fn an_open_info_section_has_two_buttons_that_run_their_commands() {
    use super::command::Section;
    let compiled = compiled_classic();
    let (state, _) = frame_state(&compiled, crate::hud_format::compile::OPEN_INFO, ControlSlot::Discs(Controls::NONE), true, false,
    ).unwrap();
    assert_eq!(section_buttons(state).count(), 2);
    assert_eq!(section_command(state, 0), Some(super::command::HudCommand::FromBeginning));
    assert_eq!(section_command(state, 1), Some(super::command::HudCommand::GoToDetail));
    assert_eq!(section_command(state, 2), None);
    assert_eq!(Section::of_tab(Section::Info.tab()), Some(Section::Info));
    assert_eq!(Section::of_tab(Section::Chapters.tab()), Some(Section::Chapters));
    assert_eq!(Section::of_tab(2), None);
}

fn chapter(start_ms: i64) -> plx_data::metadata::Chapter {
    plx_data::metadata::Chapter { index: 0, start_ms, end_ms: 0, title: String::new(), thumb: String::new(),
    }
}

#[test]
fn chapters_seat_on_the_chapter_that_holds_the_position_the_bar_shows() {
    let chapters = [chapter(0), chapter(60_000), chapter(120_000)];
    let at = |model: HudModel| super::strip::chapter_at_shown(&chapters, &model);
    let base = HudModel { scrub_ns: -1, seek_display_ns: -1, playpos_ns: 90_000 * 1_000_000, ..HudModel::default() };
    assert_eq!(at(base), 1, "the playhead");
    assert_eq!(at(HudModel { scrub_ns: 130_000 * 1_000_000, ..base }), 2, "a scrub preview outranks the playhead");
    assert_eq!(at(HudModel { loading: true, seek_display_ns: 10_000 * 1_000_000, ..base }), 0, "a loading seek shows its target");
    assert_eq!(at(HudModel { playpos_ns: -5, ..base }), 0, "before the first chapter: the first");
}

/// **The captions rise clear of an open section** (`docs/hud-plugins.md`, "Captions: the band a HUD occupies"):
/// the limit is `min(P, S - 12)`, kept as a rise per Position, moved on the sections' own spring, and only
/// ever raised while a section stays open.
mod caption_rise_pins {
    use super::command::Section;
    use super::*;
    use crate::hud_format::compile::{Compiled, OPEN_CHAPTERS, OPEN_INFO};
    use plx_machine::machine::{PresentHandle, Tick};
    use plx_machine::present::Present;
    use plx_plex::plex::session::{SubtitlePosition as Pos, SubtitleSize};
    use std::sync::Arc;

    const LOW: usize = 0;

    fn top_of(c: &Compiled, open: u8) -> Option<f32> {
        geometry(c, open, ControlSlot::Discs(Controls::NONE), true, false).open_top()
    }

    /// The screen's own sequence, over the real motion: a key calls `follow` (and `dismissed_on` for a close), and
    /// each frame's tick aims the captions at the block the open set lays out and steps the appear springs.
    struct Rig {
        c: Arc<Compiled>,
        m: HudMotion,
        bits: u8,
        ms: u32,
        present: Present,
    }

    impl Rig {
        fn new(c: Arc<Compiled>) -> Self {
            Self { c, m: HudMotion::new(), bits: 0, ms: 1_000, present: Present::new(),
            }
        }
        fn open(&mut self, bits: u8) {
            self.m.follow(bits);
            self.bits = bits;
        }
        fn close(&mut self, bits: u8, section: Section) {
            self.m.follow(bits);
            self.m.dismissed_on(section, self.ms);
            self.bits = bits;
        }
        fn frame(&mut self) {
            self.m.follow_captions(self.bits, self.c.band(), top_of(&self.c, self.bits));
            let mut ph = PresentHandle::of(&mut self.present);
            self.m.step_appear(Tick { ms: self.ms, dt_us: 16_667,
                }, &mut ph,
            );
            self.ms += 16;
        }
        fn settle(&mut self) {
            for _ in 0..300 {
                self.frame();
            }
        }
        fn band(&self) -> HudBand {
            self.c.band().with_rise(self.m.caption_rise())
        }
        /// The bottom limit the bespoke caption reader places a block at, for `p`.
        fn limit(&self, p: Pos) -> f32 {
            caption_geometry_for(SubtitleSize::from_percent(100), p, true, self.band()).1
        }
        fn limits(&self) -> [f32; 3] {
            [Pos::VideoFrame, Pos::TvScreen, Pos::Custom(40)].map(|p| self.limit(p))
        }
    }

    fn classic() -> Rig {
        Rig::new(super::loaded::classic_for_test())
    }

    fn reference() -> Rig {
        Rig::new(super::loaded::reference_for_test())
    }

    fn same(got: [f32; 3], want: [f32; 3], what: &str) {
        assert!(got.iter().zip(want).all(|(g, w)| (g - w).abs() < 0.01), "{what}: {got:?} vs {want:?}");
    }

    /// The HUD reports the open block's height from its compiled layout, not from a constant in the caption code.
    #[test]
    fn the_hud_reports_the_open_blocks_height_from_the_compile() {
        let classic = super::loaded::classic_for_test();
        let g = |open| {
            geometry(&classic, open, ControlSlot::Discs(Controls::NONE), true, false,
            )
        };
        assert_eq!((g(0).open_height, g(0).open_top()), (0.0, None), "a compact state has no block");
        assert_eq!((g(OPEN_INFO).open_height, g(OPEN_INFO).open_top()), (412.0, Some(668.0)));
        assert_eq!((g(OPEN_CHAPTERS).open_height, g(OPEN_CHAPTERS).open_top()), (396.0, Some(684.0)));
        // the reference's block includes the transport its section lifted: the compile's own tops
        let reference = super::loaded::reference_for_test();
        let tops = [OPEN_CHAPTERS, OPEN_INFO, OPEN_CHAPTERS | OPEN_INFO].map(|open| top_of(&reference, open).unwrap());
        same(tops, [542.8, 572.2, 377.8], "the reference's block tops");
        assert_eq!(top_of(&reference, 0), None);
    }

    /// The table, classic's rows, exact at the settled limit through the real reader, for each Position.
    #[test]
    fn classic_rows_of_the_table_settle_exactly() {
        let mut rig = classic();
        same(rig.limits(), [780.0; 3], "HUD up, no section changes the position anchors");
        rig.open(OPEN_INFO);
        rig.settle();
        same(rig.limits(), [656.0; 3], "built-in Info (668)");
        rig.close(0, Section::Info);
        rig.settle();
        same(rig.limits(), [780.0; 3], "the last close returns to P",
        );
        rig.open(OPEN_CHAPTERS);
        rig.settle();
        same(rig.limits(), [672.0; 3], "built-in Chapters (684)",
        );
    }

    /// The image reader (Low's ceiling, no Position) and the bespoke reader land on the same limit, and the HUD
    /// down ignores the rise.
    #[test]
    fn the_image_reader_stands_on_the_same_limit() {
        let mut rig = classic();
        rig.open(OPEN_INFO);
        rig.settle();
        let dialogue = Rect::new(300.0, 900.0, 1320.0, 135.0);
        let lift = hud_lift([dialogue].into_iter(), true, rig.band());
        assert!(close(1035.0 - lift, 656.0), "the lifted bottom stands on Info's limit: {lift}");
        assert_eq!(hud_lift([dialogue].into_iter(), false, rig.band()), 0.0, "the HUD down moves nothing");
        assert_eq!(
            caption_geometry_for(SubtitleSize::from_percent(100), Pos::VideoFrame, false, rig.band()),
            caption_geometry_for(SubtitleSize::from_percent(100), Pos::VideoFrame, false, HudBand::CLASSIC)
        );
    }

    /// The reference's rows from its compiled heights, with both sections at Low and High.
    #[test]
    fn the_references_rows_come_from_its_compiled_block() {
        let mut rig = reference();
        let band = rig.c.band();
        let p = [Pos::VideoFrame, Pos::TvScreen, Pos::Custom(40)].map(|p| band.rest_limit(p));
        same(rig.limits(), p, "HUD up, nothing open");
        for (open, top) in [(OPEN_CHAPTERS, 542.8), (OPEN_INFO, 572.2), (OPEN_CHAPTERS | OPEN_INFO, 377.8),
        ] {
            let mut rig = reference();
            rig.open(open);
            rig.settle();
            same(rig.limits(), p.map(|p| p.min(top - 12.0)), &format!("reference open {open}"),
            );
        }
        // with both open at Low and High: Low stands on the block, High (already above) is the lower of the two
        rig.open(OPEN_CHAPTERS | OPEN_INFO);
        rig.settle();
        let [low, _, high] = rig.limits();
        assert!(close(low, 377.8 - 12.0) && close(high, 377.8 - 12.0), "{low} {high}: both sit on the block, {p:?}");
    }

    /// Rise only: Info -> Chapters holds, Chapters -> Info rises 16, two open then one closed holds, the last close
    /// returns to P.
    #[test]
    fn while_a_section_stays_open_the_limit_only_rises() {
        let mut rig = classic();
        rig.open(OPEN_INFO);
        rig.settle();
        rig.open(OPEN_CHAPTERS);
        rig.settle();
        same(rig.limits(), [656.0; 3], "Info -> Chapters holds",
        );

        let mut rig = classic();
        rig.open(OPEN_CHAPTERS);
        rig.settle();
        same(rig.limits(), [672.0; 3], "Chapters");
        rig.open(OPEN_INFO);
        rig.settle();
        same(rig.limits(), [656.0; 3], "Chapters -> Info rises 16 px",
        );

        let mut rig = reference();
        let p = [Pos::VideoFrame, Pos::TvScreen, Pos::Custom(40)].map(|p| rig.c.band().rest_limit(p));
        rig.open(OPEN_INFO);
        rig.settle();
        let info = rig.limits();
        rig.open(OPEN_INFO | OPEN_CHAPTERS);
        rig.settle();
        let both = rig.limits();
        assert!(both[LOW] < info[LOW], "a second section lifts it: {info:?} -> {both:?}");
        rig.close(OPEN_INFO, Section::Chapters);
        rig.settle();
        same(rig.limits(), both, "closing one of two holds it");
        rig.close(0, Section::Info);
        rig.settle();
        same(rig.limits(), p, "the last close returns to P");
        assert_eq!(rig.m.caption_rise(), [0.0; 3], "and exactly at rest");
    }

    /// Opening with a taller section first, then a shorter one: the shorter does not lower it.
    #[test]
    fn a_shorter_section_does_not_lower_a_held_limit() {
        let mut rig = reference();
        rig.open(OPEN_CHAPTERS | OPEN_INFO);
        rig.settle();
        let tall = rig.m.caption_rise_target();
        rig.open(OPEN_CHAPTERS);
        rig.frame();
        assert_eq!(rig.m.caption_rise_target(), tall);
    }

    /// The compact HUD keeps its limit with no section, whatever frames pass.
    #[test]
    fn with_no_section_the_limit_never_moves() {
        let mut rig = classic();
        for _ in 0..120 {
            rig.frame();
            assert_eq!(rig.m.caption_rise(), [0.0; 3]);
            assert_eq!(rig.limits(), [780.0; 3]);
        }
    }

    /// Where the caption's top would cross the safe area the limit stops at 54 + the caption height: the caption
    /// overlaps the section at its own size.
    #[test]
    fn a_caption_that_would_cross_the_safe_area_stops_at_it_and_keeps_its_size() {
        let size = SubtitleSize::from_percent(165);
        let scale = size_scale(size);
        let block_h = CAPTION_ANCHOR_LINES as f32 * CAPTION_LINE_PITCH * scale;
        // a section 880 px tall: its limit would leave the block 700 px above the top of the screen
        let band = HudBand::CLASSIC.with_rise(HudBand::CLASSIC.rise_to_clear(200.0));
        let (_, baseline, floor) = caption_geometry_for_block(size, Pos::VideoFrame, true, band, block_h);
        assert!(close(baseline, SAFE.y + block_h), "the block stops under the safe top: {baseline}");
        let lay = caption_layout(CAPTION_ANCHOR_LINES, scale, baseline, floor);
        assert!(close(lay.top, SAFE.y), "top {}", lay.top);
        assert!(baseline > 200.0, "it overlaps the section instead (captions draw above it)");
        let rest = caption_layout(CAPTION_ANCHOR_LINES, scale, caption_geometry_for(size, Pos::VideoFrame, true, HudBand::CLASSIC).1, floor,
        );
        assert_eq!((lay.face, lay.pitch), (rest.face, rest.pitch), "size never gives way");
        // a block that fits is not clamped
        let (_, free, _) = caption_geometry_for_block(SubtitleSize::from_percent(100), Pos::VideoFrame, true, HudBand::CLASSIC.with_rise(HudBand::CLASSIC.rise_to_clear(668.0)), 144.0,
        );
        assert!(close(free, 656.0), "{free}");
    }

    /// One delta for every part below the limit; a part above it stays, and nothing is rescaled.
    #[test]
    fn every_part_below_the_limit_moves_by_one_delta_and_the_rest_stay() {
        let mut rig = classic();
        rig.open(OPEN_INFO);
        rig.settle();
        let band = rig.band();
        let (l1, l2) = (Rect::new(300.0, 900.0, 1320.0, 60.0), Rect::new(300.0, 960.0, 1320.0, 75.0),
        );
        let above = Rect::new(300.0, 590.0, 1320.0, 60.0); // bottom 650: above Info's limit 656
        let sign = Rect::new(300.0, 100.0, 600.0, 50.0);
        let set = [l1, l2, above, sign];
        let moves: Vec<bool> = set.iter().map(|r| overhangs(*r, band)).collect();
        assert_eq!(moves, [true, true, false, false]);
        let lift = hud_lift(set.iter().copied(), true, band);
        assert!(close(lift, 1035.0 - 656.0), "{lift}");
        // the pair keeps its gap; the rest do not move
        let moved = |r: Rect| {
            if overhangs(r, band) { Rect::new(r.x, r.y - lift, r.w, r.h) } else { r
            }
        };
        assert!(close(moved(l2).y - moved(l1).y, l2.y - l1.y));
        assert_eq!((moved(above), moved(sign)), (above, sign));
        assert!(set.iter().all(|r| (moved(*r).w, moved(*r).h) == (r.w, r.h)), "nothing is rescaled");
    }

    /// A cue changing mid-travel is laid out at the limit's current y.
    #[test]
    fn a_cue_that_changes_mid_travel_stands_on_the_current_limit() {
        let mut rig = classic();
        rig.open(OPEN_INFO);
        for _ in 0..6 {
            rig.frame();
        }
        let rise = rig.m.caption_rise()[LOW];
        assert!(rise > 0.0 && rise < 124.0, "mid-travel: {rise}");
        let now = rig.limit(Pos::VideoFrame);
        assert!(close(now, 780.0 - rise));
        for lines in 1..=CAPTION_ANCHOR_LINES {
            let (scale, baseline, floor) = caption_geometry_for_block(SubtitleSize::from_percent(100), Pos::VideoFrame, true, rig.band(), anchored_height(lines, 1.0),
            );
            let lay = caption_layout(lines, scale, baseline, floor);
            assert!(close(lay.top + lines as f32 * lay.pitch, now), "{lines} lines end at the limit's current y");
        }
    }

    /// The spring stops asking for frames when `plx_machine::motion::moving` says it has stopped (about 0.125 px at
    /// a 124 px rise), so it must land exactly on its target there: a limit left 0.1 px short would draw captions
    /// off today's 780 with the section gone. The test reads the shared rest test, so loosening it cannot slip past.
    #[test]
    fn the_rise_lands_exactly_where_the_spring_stops_asking_for_frames() {
        for open in [true, false] {
            let mut rig = classic();
            rig.open(OPEN_INFO);
            rig.settle();
            if !open {
                rig.close(0, Section::Info);
            }
            for i in 0..300 {
                rig.frame();
                let (rise, target) = (rig.m.caption_rise()[LOW], rig.m.caption_rise_target()[LOW]);
                assert!(
                    rise == target || plx_machine::motion::moving(rise, target, 0.0, 0.016_667),
                    "open={open} frame {i}: {rise} is {} px short of {target} and nothing asks for another frame",
                    (rise - target).abs()
                );
            }
        }
    }

    /// **A section held at appear 0 for several frames holds the limit for as many.** The appear spring stays
    /// held while the frame's text is still being rasterised (`surface_text_pending`, bounded at 100 ms), and
    /// the snapshot fence does the same through `snapshot_pending`; the limit must wait with it and start on the
    /// frame the section does. The text latch is the one a host test can drive.
    #[test]
    fn a_section_held_for_several_frames_holds_the_limit_with_it() {
        plx_gfx::text::latch_surface_text_pending(true);
        let mut rig = classic();
        rig.open(OPEN_INFO);
        let (mut held, mut started) = (0, None);
        for i in 0..40 {
            rig.frame();
            let (appear, rise) = (rig.m.section_appear(Section::Info), rig.m.caption_rise()[LOW],
            );
            assert_eq!(appear > 0.0, rise > 0.0, "frame {i}: appear {appear}, rise {rise}");
            if appear == 0.0 { held += 1 } else if started.is_none() { started = Some(i) }
        }
        plx_gfx::text::latch_surface_text_pending(false);
        assert!(held >= 3, "the hold lasted {held} frames: the pin needs a multi-frame hold");
        assert!(started.is_some(), "and the section started once the bound was up");
    }

    /// The limit's first moving frame is the section's first appear frame, on open and on close, and it passes
    /// neither end (critically damped).
    #[test]
    fn the_limit_starts_with_the_section_and_never_overshoots() {
        for section in [Section::Info, Section::Chapters] {
            let bit = section.open_bit();
            let mut rig = classic();
            rig.open(bit);
            let target = rig.c.band().rise_to_clear(top_of(&rig.c, bit).unwrap())[LOW];
            let (mut first_appear, mut first_rise) = (None, None);
            let mut last = 0.0;
            for i in 0..300 {
                rig.frame();
                let (appear, rise) = (rig.m.section_appear(section), rig.m.caption_rise()[LOW]);
                if appear > 0.0 && first_appear.is_none() { first_appear = Some(i) }
                if rise > 0.0 && first_rise.is_none() { first_rise = Some(i) }
                assert!(rise >= last && rise <= target + 1e-3, "{section:?} frame {i}: {rise} after {last}, target {target}");
                last = rise;
            }
            assert!(first_appear.is_some() && first_appear == first_rise, "{section:?} open: appear {first_appear:?}, limit {first_rise:?}");
            assert_eq!(last, target, "settles exactly on the block");

            rig.close(0, section);
            let (mut first_appear, mut first_rise) = (None, None);
            let mut last = target;
            for i in 0..300 {
                rig.frame();
                let (appear, rise) = (rig.m.section_appear(section), rig.m.caption_rise()[LOW]);
                if appear < 1.0 && first_appear.is_none() { first_appear = Some(i) }
                if rise < target && first_rise.is_none() { first_rise = Some(i) }
                assert!(rise <= last && rise >= 0.0, "{section:?} close frame {i}: {rise} after {last}");
                last = rise;
            }
            assert!(first_appear.is_some() && first_appear == first_rise, "{section:?} close: appear {first_appear:?}, limit {first_rise:?}");
            assert_eq!(last, 0.0);
        }
    }
}
