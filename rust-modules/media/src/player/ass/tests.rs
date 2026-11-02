Warning: truncated output (original token count: 18482)
Total output lines: 1651

use super::*;

fn event(id: u32, start_ms: i64, duration_ms: i64) -> Event {
    Event {
        start_ms,
        duration_ms,
        payload: format!("{id},0,Default,,0,0,0,,hello").into_bytes().into(),
    }
}

fn source(events: Vec<Event>) -> Source {
    Source {
        id: 1,
        revision: 1,
        origin: Origin::Authored,
        content: Content::Embedded {
            header: Arc::from(&b"[Script Info]\n"[..]),
            events: events.into(),
            fonts: Arc::from([]),
        },
    }
}

#[test]
fn window_retirement_keeps_native_fonts_and_retained_overlapping_events() {
    let long = event(0, 0, 20_000);
    let ended = event(1, 0, 1000);
    let old = source(vec![long.clone(), ended, event(2, 2000, 3000)]);
    let new = source(vec![long, event(2, 2000, 3000), event(3, 6000, 2000)]);
    assert_eq!(append_from(&old, &new), Some(2));
}

#[test]
fn append_preserves_readorder_but_replacement_cannot_append() {
    let old = source(vec![event(0, 0, 1000)]);
    let new = source(vec![event(0, 0, 1000), event(1, 0, 1000)]);
    assert_eq!(append_from(&old, &new), Some(1));
    let replacement = source(vec![event(2, 0, 1000)]);
    assert_eq!(append_from(&old, &replacement), None);
    let mut seek = new;
    seek.id = 2;
    assert_eq!(append_from(&old, &seek), None);
}

#[test]
fn rejects_unbounded_and_invalid_events_before_native_parsing() {
    let mut invalid = event(0, i64::MAX, 1);
    assert!(validate(&source(vec![invalid.clone()])).is_err());
    invalid.start_ms = 0;
    invalid.duration_ms = 0;
    assert!(validate(&source(vec![invalid.clone()])).is_err());
    invalid.duration_ms = 1000;
    invalid.payload = vec![b'a'; MAX_EVENT_BYTES + 1].into();
    assert!(validate(&source(vec![invalid])).is_err());
    assert!(validate(&source(vec![event(0, 0, 1000)])).is_ok());
}

#[test]
fn ids_do_not_alias_cancellation_or_each_other() {
    let a = next_source_id();
    let b = next_source_id();
    assert_ne!(a, 0);
    assert_ne!(a, b);
}

fn request_key(source: &Source) -> Key {
    Key {
        epoch: 1,
        source_id: source.id,
        revision: source.revision,
        width: 320,
        height: 180,
        storage_width: 320,
        storage_height: 180,
    }
}

#[test]
fn off_retires_a_racing_publication_and_pending_source_outside_the_mailbox_lock() {
    let runtime = Runtime::new();
    let source = Arc::new(source(vec![event(0, 0, 2000)]));
    let key = request_key(&source);
    let frame = error_frame(key, Fault::RenderFailed);
    let source_witness = Arc::downgrade(&source);
    let frame_witness = Arc::downgrade(&frame);
    let mut engine = Engine {
        source: Some(source.clone()),
        frame: Some(frame.clone()),
        ..Engine::default()
    };
    runtime.source_id.store(source.id, Ordering::Release);
    {
        let mut mailbox = runtime.mailbox.lock().unwrap();
        mailbox.request(key, 100, &source);
        // The worker owns the lock and has already accepted this epoch. Cancel
        // must return while the lock is held; the old publication then races in.
        clear_runtime(&runtime);
        mailbox.publish_error(key, frame.clone());
    }
    drop(source);
    drop(frame);
    let retired = {
        let mut mailbox = runtime.mailbox.lock().unwrap();
        retire_off(&runtime, &mut mailbox, &mut engine).expect("Off has resources to retire")
    };
    assert!(
        runtime.mailbox.try_lock().is_ok(),
        "destruction must happen after unlocking"
    );
    assert!(engine.is_empty());
    {
        let mailbox = runtime.mailbox.lock().unwrap();
        assert!(
            mailbox.is_empty(),
            "retire stale work, requests and publications before considering them for rendering"
        );
    }
    assert!(source_witness.upgrade().is_some());
    assert!(frame_witness.upgrade().is_some());
    drop(retired);
    assert!(source_witness.upgrade().is_none());
    assert!(frame_witness.upgrade().is_none());
    assert!(
        retire_off(&runtime, &mut runtime.mailbox.lock().unwrap(), &mut engine).is_none(),
        "an empty Off worker can stay parked without periodic cleanup"
    );
}

#[test]
fn cancellation_before_worker_parks_is_remembered_and_releases_resources() {
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    let runtime = Arc::new(Runtime::new());
    let source = Arc::new(source(vec![event(0, 0, 2000)]));
    let key = request_key(&source);
    let frame = error_frame(key, Fault::RenderFailed);
    let witness = Arc::downgrade(&frame);
    runtime.source_id.store(source.id, Ordering::Release);
    runtime.mailbox.lock().unwrap().publish_error(key, frame.clone());
    let observed_idle = Arc::new(AtomicBool::new(false));
    let may_park = Arc::new(AtomicBool::new(false));
    let (done_tx, done_rx) = mpsc::channel();
    let worker = {
        let runtime = runtime.clone();
        let observed_idle = observed_idle.clone();
        let may_park = may_park.clone();
        plx_base::task::spawn("ASS cancellation test", move || {
            runtime.worker.set(std::thread::current()).unwrap();
            let mut engine = Engine {
                source: Some(source),
                frame: Some(frame),
                ..Engine::default()
            };
            {
                let mailbox = runtime.mailbox.lock().unwrap();
                assert!(mailbox.is_empty() || mailbox.published.is_some());
                assert_ne!(runtime.source_id.load(Ordering::Acquire), 0);
            }
            // Main will cancel only AFTER the idle observation and BEFORE park.
            // Spin on an atomic gate so no other blocking primitive can consume
            // this thread's park permit during the forced interleaving.
            observed_idle.store(true, Ordering::Release);
            while !may_park.load(Ordering::Acquire) {
                std::hint::spin_loop();
            }
            std::thread::park();
            let retired = {
                let mut mailbox = runtime.mailbox.lock().unwrap();
                retire_off(&runtime, &mut mailbox, &mut engine)
                    .expect("remembered Off wakes cleanup")
            };
            drop(retired);
            done_tx.send(()).unwrap();
        })
        .expect("test worker")
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    while !observed_idle.load(Ordering::Acquire) {
        assert!(
            Instant::now() < deadline,
            "worker must reach the controlled idle boundary"
        );
        std::thread::yield_now();
    }
    clear_runtime(&runtime);
    may_park.store(true, Ordering::Release);
    let completed = done_rx.recv_timeout(Duration::from_secs(2));
    // If the wake was lost, release the test thread before reporting failure.
    // The timeout observes completion; it never chooses the race interleaving.
    if completed.is_err() {
        worker.thread().unpark();
    }
    worker.join().unwrap();
    completed.expect("cancel's wake permit must survive until park");
    assert!(
        witness.upgrade().is_none(),
        "Off retains neither published nor worker-owned frame"
    );
}

const HEADER: &str = "[Script Info]\nScriptType: v4.00+\nPlayResX: 320\nPlayResY: 180\nScaledBorderAndShadow: yes\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Inter,24,&H0000FF00,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,0,0,7,0,0,0,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n";

fn script(lines: &str) -> Arc<Source> {
    Arc::new(Source {
        id: next_source_id(),
        revision: 1,
        origin: Origin::Authored,
        content: Content::Script {
            bytes: format!("{HEADER}{lines}").into_bytes().into(),
            fonts: Arc::from([]),
        },
    })
}

fn job(source: &Arc<Source>, now_ms: i64, width: i32, height: i32, storage_width: i32, storage_height: i32) -> Job {
    Job {
        kind: Kind::Normal,
        key: Key {
            epoch: 1,
            source_id: source.id,
            revision: source.revision,
            width,
            height,
            storage_width,
            storage_height,
        },
        now_ms,
        seq: 1,
        source: source.clone(),
        queued: None,
    }
}

fn render(engine: &mut Engine, source: &Arc<Source>, now_ms: i64) -> Arc<Frame> {
    let frame = engine.render(&job(source, now_ms, 320, 180, 320, 180)).frame;
    assert_eq!(
        frame.error, None,
        "native fixture requires the built host library in pkg"
    );
    frame
}

fn pixel(frame: &Frame, x: i32, y: i32) -> [u8; 4] {
    let Some(r) = frame
        .rects
        .iter()
        .find(|r| x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height)
    else {
        return [0; 4];
    };
    let start = ((y - r.y) * r.width + x - r.x) as usize * 4;
    r.rgba[start..start + 4].try_into().unwrap()
}

/// An actual pixel contract against the same pinned library shipped to the TV.
/// Run after `make libass-host` with `--include-ignored`.
#[test]
#[ignore = "requires the pinned native host libass artifact and packaged fonts"]
fn native_pixels_preserve_position_layers_alpha_motion_karaoke_and_readorder() {
    let red = r"{\an7\pos(20,20)\c&H0000FF&\p1}m 0 0 l 40 0 40 40 0 40";
    let blue = r"{\an7\pos(40,30)\c&HFF0000&\alpha&H80&\p1}m 0 0 l 40 0 40 40 0 40";
    let source = script(&format!(
        "Dialogue: 0,0:00:00.00,0:00:02.00,Default,,0,0,0,,{red}\nDialogue: 1,0:00:00.00,0:00:02.00,Default,,0,0,0,,{blue}\n"));
    let mut engine = Engine::default();
    let first = render(&mut engine, &source, 100);
    assert_eq!(pixel(&first, 25, 25), [255, 0, 0, 255]);
    assert_eq!(pixel(&first, 45, 35…13482 tokens truncated…
/// D12 part 1: a seek (new id, same header and fonts) and a switch between tracks that share a
/// header reuse the renderer; only a different header or font set makes a new one.
#[test]
#[ignore = "requires the pinned native host libass artifact and packaged fonts"]
fn native_reuse_counts_renderers_for_seek_switch_and_authored_change() {
    let mut engine = Engine::default();
    let a = track(HEADER, &[(0, 2000, RED)]);
    assert_eq!(pixel(&render(&mut engine, &a, 100), 25, 25), [255, 0, 0, 255]);
    assert_eq!(open_calls(), 1, "the first track opens a renderer");

    let seek = track(HEADER, &[(0, 2000, RED), (2000, 1000, BLUE)]);
    let frame = render(&mut engine, &seek, 100);
    assert_eq!(frame.source_id, seek.id);
    assert_eq!(pixel(&frame, 25, 25), [255, 0, 0, 255]);
    assert_eq!(open_calls(), 1, "a seek (new id, same header) opens no renderer");

    let other = track(HEADER, &[(0, 2000, BLUE)]);
    let frame = render(&mut engine, &other, 100);
    assert_eq!(frame.source_id, other.id);
    assert_eq!(pixel(&frame, 25, 25), [0; 4], "only the new track's events are shown");
    assert_eq!(pixel(&frame, 105, 105), [0, 0, 255, 255]);
    assert_eq!(open_calls(), 1, "a switch between identical headers opens no renderer");

    let styled = HEADER.replace("&H0000FF00", "&H000000FF");
    let authored = track(&styled, &[(0, 2000, RED)]);
    assert_eq!(render(&mut engine, &authored, 100).source_id, authored.id);
    assert_eq!(open_calls(), 2, "a different authored header opens a new renderer");
}

/// libass says "unchanged" when the renderer keeps showing what it showed before a reuse. The
/// frame it answers with must name the source it now answers for, or `request` never returns it.
#[test]
#[ignore = "requires the pinned native host libass artifact and packaged fonts"]
fn native_unchanged_picture_after_a_switch_carries_the_new_source_id() {
    // both blank at 5 s
    let mut engine = Engine::default();
    let a = track(HEADER, &[(0, 1000, RED)]);
    let b = track(HEADER, &[(0, 1000, BLUE)]);
    render(&mut engine, &a, 5000);
    let frame = render(&mut engine, &b, 5000);
    assert_eq!(frame.source_id, b.id, "blank to blank");
    assert!(frame.rects.is_empty());
    // the same picture on both
    let c = track(HEADER, &[(0, 9000, RED)]);
    let d = track(HEADER, &[(0, 9000, RED)]);
    let shown = render(&mut engine, &c, 100);
    let frame = render(&mut engine, &d, 100);
    assert_eq!(frame.source_id, d.id, "same picture");
    assert_eq!(frame.rects, shown.rects);
    assert_eq!(open_calls(), 1);
}

fn validity_at(engine: &mut Engine, source: &Arc<Source>, now_ms: i64) -> (Arc<Frame>, Validity) {
    let out = engine.render(&job(source, now_ms, 320, 180, 320, 180));
    assert_eq!(out.frame.error, None, "native fixture requires the built host library in pkg");
    (out.frame, out.validity)
}

/// The window must agree with what libass really draws, for the odd stamps `string2timecode`
/// reads its own way (a fraction is centiseconds; no fraction is 0).
#[test]
#[ignore = "requires the pinned native host libass artifact and packaged fonts"]
fn native_validity_boundaries_are_where_libass_draws() {
    let line = |start: &str, end: &str| {
        format!("Dialogue: 0,{start},{end},Default,,0,0,0,,{{\\an7\\pos(20,20)\\p1}}m 0 0 l 40 0 40 40 0 40\n")
    };
    // 1 s + 5 cs .. 2 s + 50 cs; the second line has no fractions at all: libass reads 0..0
    let source = script(&format!("{}{}", line("0:00:01.5", "0:00:02.50"), line("0:00:03", "0:00:05")));
    let mut engine = Engine::default();
    let (frame, v) = validity_at(&mut engine, &source, 500);
    assert!(frame.rects.is_empty());
    assert_eq!((v.from_ms, v.until_ms), (i64::MIN, 1050));
    // rendering exactly at each computed boundary shows the change
    assert!(validity_at(&mut engine, &source, 1049).0.rects.is_empty());
    let (frame, v) = validity_at(&mut engine, &source, 1050);
    assert_eq!(pixel(&frame, 25, 25), [0, 255, 0, 255], "visible from 1050 ms");
    assert_eq!((v.from_ms, v.until_ms), (1050, 2500));
    assert_eq!(pixel(&validity_at(&mut engine, &source, 2499).0, 25, 25)[3], 255);
    let (frame, v) = validity_at(&mut engine, &source, 2500);
    assert!(frame.rects.is_empty(), "gone at 2500 ms");
    assert_eq!((v.from_ms, v.until_ms), (2500, i64::MAX));
    assert!(validity_at(&mut engine, &source, 4000).0.rects.is_empty(), "the stamp-less line never shows");
}

#[test]
#[ignore = "requires the pinned native host libass artifact and packaged fonts"]
fn native_karaoke_and_motion_are_animated_only_while_active() {
    let source = script(concat!(
        "Dialogue: 0,0:00:00.00,0:00:02.00,Default,,0,0,0,,{\\an7\\pos(20,100)}{\\kf100}AAAA\n",
        "Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,{\\an7\\pos(20,20)\\p1}m 0 0 l 4 0 4 4 0 4\n",
    ));
    let mut engine = Engine::default();
    assert!(validity_at(&mut engine, &source, 500).1.animated);
    let (frame, v) = validity_at(&mut engine, &source, 3500);
    assert!(!v.animated && !frame.rects.is_empty());
}

#[test]
#[ignore = "requires the pinned native host libass artifact and packaged fonts"]
fn native_script_window_clamps_validity_and_non_windowed_scripts_have_timings() {
    // 30 000 lines a second apart, with a two-minute hole after the 15 000th
    let t = |ms: i64| format!("{}:{:02}:{:02}.{:02}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000 / 10);
    let mut lines = String::new();
    for i in 0..30_000i64 {
        let at = i * 1000 + if i >= 15_000 { 120_000 } else { 0 };
        lines.push_str(&format!(
            "Dialogue: 0,{},{},Default,,0,0,0,,{{\\an7\\pos(10,10)\\p1}}m 0 0 l 50 0 50 50 0 50\n",
            t(at), t(at + 900)));
    }
    let source = script(&lines);
    let mut engine = Engine::default();
    let now = 15_060_000;
    let (frame, v) = validity_at(&mut engine, &source, now);
    assert!(frame.rects.is_empty(), "in the hole");
    let (lo, hi) = engine.script.as_ref().unwrap().window.unwrap();
    assert_eq!(lo, now - 10_000);
    assert_eq!(v.from_ms, lo, "the raw boundary is older than the window, which only reaches back to lo");
    assert_eq!(v.until_ms, 15_120_000);
    assert!(v.until_ms <= hi);

    // a script that is loaded whole keeps its timings too
    let small = script("Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,{\\an7\\pos(10,10)\\p1}m 0 0 l 5 0 5 5 0 5\n");
    let (_, v) = validity_at(&mut engine, &small, 500);
    assert_eq!((v.from_ms, v.until_ms), (i64::MIN, 1000));
    assert!(engine.script.as_ref().unwrap().window.is_none());
}

/// A Text-origin sidecar script past the 8 MiB a script loaded whole may be (twelve events of 900 KiB:
/// a few lines, many bytes) is not refused: it is fed in windows within the bound, and each cue is drawn
/// when the playhead reaches it, including after a seek back.
#[test]
#[ignore = "requires the pinned native host libass artifact and packaged fonts"]
fn native_a_text_script_past_the_whole_script_bound_renders_through_its_windows() {
    let style = TextStyle::default();
    let mut text = text_ass::header(&style);
    let pad = "x".repeat(900 * 1024);
    for k in 0..12i64 {
        text.push_str(&format!(
            "Dialogue: 0,{},{},Default,,0,0,0,,{{\\an7\\pos(10,10)\\p1{pad}}}m 0 0 l 50 0 50 50 0 50\n",
            ass_time(k * 2000), ass_time(k * 2000 + 1000)));
    }
    assert!(text.len() > MAX_SCRIPT_BYTES);
    let source = Arc::new(Source {
        id: next_source_id(),
        revision: 1,
        origin: Origin::Text(style),
        content: Content::Script { bytes: text.into_bytes().into(), fonts: Arc::from([]) },
    });
    assert_eq!(validate(&source), Ok(()), "a windowed script is bounded by its source");
    let mut engine = Engine::default();
    for now in [500, 10_500, 22_500, 4_500] {
        let (frame, _) = validity_at(&mut engine, &source, now);
        assert!(!frame.rects.is_empty(), "the cue alive at {now} ms is drawn");
        let (lo, hi) = engine.script.as_ref().unwrap().window.expect("fed in windows");
        assert!((lo..hi).contains(&now));
    }
    let (frame, _) = validity_at(&mut engine, &source, 1_500);
    assert!(frame.rects.is_empty(), "and nothing between two cues");
}

mod synth;

/// A paused playhead whose subtitle offset moves both ways (2 s cues back to back: 20-22, 22-24,
/// 24-26): every new clock gets the frame for ITS cue, whatever the worker was doing (an
/// ahead render armed by the previous frame, or nothing) when the offset changed.
#[test]
fn a_changed_offset_on_a_paused_playhead_publishes_the_frame_of_the_new_clock() {
    use super::mailbox::{Mailbox, Rendered};
    let source = Arc::new(source(vec![event(0, 20_000, 2000), event(1, 22_000, 2000), event(2, 24_000, 2000)]));
    let key = request_key(&source);
    let cue_of = |now: i64| (now / 2000) * 2000;
    let validity_of = |now: i64| ass_timing::Validity { from_ms: cue_of(now), until_ms: cue_of(now) + 2000, animated: false };
    let mut mailbox = Mailbox::new();
    // The worker drains its mailbox: the frame is `error_frame` (any frame), the validity the cue's.
    let drain = |mailbox: &mut Mailbox| {
        while let Some(job) = mailbox.take() {
            let rendered = Rendered { frame: error_frame(job.key, Fault::RenderFailed), validity: validity_of(job.now_ms), aligns: None };
            mailbox.finish(&job, Some(rendered));
        }
    };
    // Paused at 23.0 s, the offset 0, then +2.5 s, then -2.5 s, then back, each one a frame step.
    for clock in [23_000, 20_500, 25_500, 20_500, 25_500, 23_000] {
        mailbox.request(key, clock, &source);
        drain(&mut mailbox);
        mailbox.request(key, clock, &source);
        let shown = mailbox.view(&key).expect("a frame");
        let published = mailbox.published.as_ref().map(|p| p.validity).expect("published");
        assert!(published.contains(clock), "clock {clock}: the frame on screen covers {published:?} ({:?})", shown.serial);
    }
}
