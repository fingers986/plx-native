Warning: truncated output (original token count: 6111)
Total output lines: 464

//! Native goldens for the SYNTHESIZED text-origin style (`text_ass::header`, plan
//! `libass-text-subs` §2.2-§2.4): a text track drawn by the pinned libass at the 1920x1080 logical
//! frame, with the settings a text-origin renderer gets (libunibreak wrapping, the line spacing
//! that turns libass's face-height advance into today's pitch). Nothing here is wired to a player;
//! it is the measurement that fixes the style's constants.

use super::*;
use crate::player::text_ass::{self, TextDialect};
use crate::player::text_style::{self, HudBand, TextStyle};
use plx_plex::plex::session::{SubtitlePosition, SubtitleSize};

fn text_key() -> Key {
    Key {
        epoch: 1,
        source_id: 1,
        revision: 1,
        width: 1920,
        height: 1080,
        storage_width: 1920,
        storage_height: 1080,
    }
}

/// One cue of a synthesized track: the header style it names and its ASS text.
#[derive(Clone)]
struct Cue {
    style: &'static str,
    body: String,
}

/// A cue as the converter makes it from SubRip text: the style the alignment picks and the body.
fn cue(text: &str) -> Cue {
    let converted = text_ass::convert(text, TextDialect::Srt).expect("a visible cue");
    Cue { style: text_ass::style_name(converted.align), body: converted.body }
}

/// A cue with an ASS body written by hand, in the style of numpad alignment `align`.
fn raw(align: u8, body: &str) -> Cue {
    Cue { style: text_ass::style_name(align), body: body.to_string() }
}

/// A renderer holding the synthesized header for `style` and `cues`, set up the way a text-origin
/// renderer is: unicode line breaking on, and the line spacing that makes the pitch 48 x scale.
fn text_native(style: &TextStyle, cues: &[Cue]) -> Native {
    let mut script = text_ass::header(style);
    for cue in cues {
        script.push_str(&format!(
            "Dialogue: 0,0:00:00.00,0:00:05.00,{},,0,0,0,,{}\n",
            cue.style, cue.body
        ));
    }
    let mut native = native_with(&[], &script);
    assert!(native.set_wrap_unicode(true));
    assert!(native.set_line_spacing(text_style::line_spacing(style.scale())));
    native
}

fn style(size: SubtitleSize, position: SubtitlePosition) -> TextStyle {
    TextStyle { size, position }
}

/// The frame of `cues` at 1 s.
fn text_frame(style: &TextStyle, cues: &[Cue]) -> Vec<Rect> {
    let mut native = text_native(style, cues);
    native.render(text_key(), 1000, &[]).expect("a valid track").expect("a first frame")
}

/// The pixels of a frame as `(x, y, [r, g, b, a])`.
fn pixels(rects: &[Rect]) -> impl Iterator<Item = (i32, i32, [u8; 4])> + '_ {
    rects.iter().flat_map(|r| {
        (0..r.height).flat_map(move |y| {
            (0..r.width).map(move |x| {
                let at = ((y * r.width + x) * 4) as usize;
                (r.x + x, r.y + y, [r.rgba[at], r.rgba[at + 1], r.rgba[at + 2], r.rgba[at + 3]])
            })
        })
    })
}

/// The white fill (not the black outline, and not the soft edge of either): straight RGBA with
/// most of its colour and opacity.
fn is_fill(px: [u8; 4]) -> bool {
    px[0] >= 128 && px[3] >= 128
}

/// The row just under the last fill row of the first glyph on the line, which is where the glyph
/// stands: for a cue starting "H" this is the baseline, with no descender and no outline in it.
fn first_glyph_baseline(rects: &[Rect]) -> i32 {
    let fill: Vec<(i32, i32)> = pixels(rects).filter(|(_, _, px)| is_fill(*px)).map(|(x, y, _)| (x, y)).collect();
    let left = fill.iter().map(|p| p.0).min().expect("fill ink");
    // The first glyph is the first run of columns that all carry fill.
    let mut right = left;
    while fill.iter().any(|p| p.0 == right + 1) {
        right += 1;
    }
    fill.iter().filter(|p| (left..=right).contains(&p.0)).map(|p| p.1).max().unwrap() + 1
}

/// Rows holding any fill: `(first, last + 1)` of every band of consecutive rows.
fn fill_bands(rects: &[Rect]) -> Vec<(i32, i32)> {
    let rows: std::collections::BTreeSet<i32> =
        pixels(rects).filter(|(_, _, px)| is_fill(*px)).map(|(_, y, _)| y).collect();
    let mut bands: Vec<(i32, i32)> = Vec::new();
    for y in rows {
        match bands.last_mut() {
            Some((_, end)) if *end == y => *end = y + 1,
            _ => bands.push((y, y + 1)),
        }
    }
    bands
}

/// Fill columns `(left, right + 1)` within rows `[top, bottom)`.
fn fill_extent(rects: &[Rect], top: i32, bottom: i32) -> (i32, i32) {
    let xs: Vec<i32> = pixels(rects)
        .filter(|(_, y, px)| (top..bottom).contains(y) && is_fill(*px))
        .map(|(x, _, _)| x)
        .collect();
    (*xs.iter().min().expect("fill ink"), xs.iter().max().unwrap() + 1)
}

/// Lowest row of any ink at all (outline included), exclusive.
fn ink_bottom(rects: &[Rect]) -> i32 {
    ink(rects).bands.last().expect("ink").1 + 1
}

const SIZES: [SubtitleSize; SubtitleSize::COUNT] = SubtitleSize::LADDER;
const POSITIONS: [SubtitlePosition; 3] = SubtitlePosition::LADDER;

/// libass's line box for a one-line Inter Bold cue: `win ascent` above the baseline and `win
/// descent` below, at the style's em (`36 x scale`).
fn win_ascent_px(scale: f32) -> f32 {
    (text_style::INTER_BOLD_WIN_ASCENT_EM * text_style::CAPTION_FACE as f64 * scale as f64) as f32
}

fn win_descent_px(scale: f32) -> f32 {
    (text_style::INTER_BOLD_WIN_DESCENT_EM * text_style::CAPTION_FACE as f64 * scale as f64) as f32
}

fn near(what: &str, got: f32, want: f32, tolerance: f32) {
    assert!((got - want).abs() <= tolerance, "{what}: got {got}, want {want} +/- {tolerance}");
}

/// Plan §2.2: the ink baseline of a one-line "Hxg" cue is within 2 px of today's, at all three
/// positions and every size. This is the measurement `INK_TRIM` was fixed by: with `MarginV =
/// BOTTOM_GAP` the Medium baseline was 968 against today's 971.
#[test]
#[ignore = "requires the pinned native host libass artifact and packaged fonts"]
fn native_the_ink_baseline_of_a_one_line_cue_is_todays_at_every_position_and_size() {
    for size in SIZES {
        for position in POSITIONS {
            let picks = style(size, position);
            let rects = text_frame(&picks, &[cue("Hxg")]);
            let today = text_style::today_ink_baseline(position, picks.scale(), false, HudBand::CLASSIC);
            near(&format!("{size:?} {position:?}"), first_glyph_baseline(&rects) as f32, today, 2.0);
            // The descender of the "g" stays inside the line box libass anchors at the margin.
            let anchor = text_style::bottom_anchor_y(position);
            assert!(ink_bottom(&rects) as f32 <= anchor + 2.0, "{size:?} {position:?}: ink past the anchor {anchor}");
        }
    }
}

/// The bottom of the line box is the anchor: a one-line cue stands `win descent` above it.
#[test]
#[ignore = "requires the pinned native host libass artifact and packaged fonts"]
fn native_the_bottom_anchor_is_where_the_last_line_box_ends() {
    for size in SIZES {
        for position in POSITIONS {
            let picks = style(size, position);
            let baseline = first_glyph_baseline(&text_frame(&picks, &[cue("Hxg")])) as f32;
            let anchor = text_style::bottom_anchor_y(position);
            near(&format!("{size:?} {position:?}"), baseline, anchor - win_descent_px(picks.scale()), 1.5);
        }
    }
}

/// Plan §2.2: the line pitch is 48 x scale and the block stays bottom-anchored (its last line on the
/// one-line baseline), so a three-line cue clears the Timing capsule exactly as today.
#[test]
#[ignore = "requires the pinned native host libass artifact and packaged fonts"]
fn native_the_line_pitch_is_forty_eight_times_the_scale() {
    for size in SIZES {
        let picks = style(size, SubtitlePosition::TvScreen);
        let scale = picks.scale();
        let one = first_glyph_baseline(&text_frame(&picks, &[cue("H")]));
        let two = fill_bands(&text_frame(&picks, &[cue("H\\NH")]));
        assert_eq!(two.len(), 2, "{size:?}: two lines");
        near(&format!("{size:?} pitch"),…2111 tokens truncated…p.2[..3].iter().all(|c| *c < 24)), "the stroke is black (a fill edge may tint it a little)");
}

/// A pixel of the colour-tagged run `(r, g, b)` at full opacity, clear of any edge blend.
fn solid(px: [u8; 4], r: bool, g: bool, b: bool) -> bool {
    let on = |c: u8, set: bool| if set { c >= 200 } else { c <= 40 };
    px[3] >= 250 && on(px[0], r) && on(px[1], g) && on(px[2], b)
}

fn centroid_x(rects: &[Rect], pick: impl Fn([u8; 4]) -> bool) -> f32 {
    let xs: Vec<i32> = pixels(rects).filter(|(_, _, px)| pick(*px)).map(|(x, _, _)| x).collect();
    assert!(!xs.is_empty(), "no pixels of the run");
    xs.iter().sum::<i32>() as f32 / xs.len() as f32
}

const RED: &str = "{\\1c&H0000FF&}";
const WHITE: &str = "{\\1c&HFFFFFF&}";

/// `Encoding -1` picks the base direction per paragraph. A line that starts with Arabic is RTL, so
/// its trailing neutral (the full stop) lands at the LEFT end; a line that starts with Latin is LTR
/// and the stop lands at the right. Arabic first, Latin after: Arabic stands to the right.
#[test]
#[ignore = "requires the pinned native host libass artifact and packaged fonts"]
fn native_a_mixed_latin_and_arabic_line_takes_the_direction_of_its_first_letter() {
    let picks = style(SubtitleSize::from_percent(125), SubtitlePosition::TvScreen);
    let arabic = "مرحبا";
    let rtl = text_frame(&picks, &[raw(2, &format!("{RED}{arabic}{WHITE}.")) ]);
    assert!(
        centroid_x(&rtl, |px| solid(px, true, true, true)) < centroid_x(&rtl, |px| solid(px, true, false, false)),
        "an Arabic paragraph puts its closing full stop on the left"
    );
    let ltr = text_frame(&picks, &[raw(2, &format!("abc{RED} {arabic}{WHITE}."))]);
    // The Latin word leads on the left; the stop after the Arabic run, in an LTR paragraph, is the
    // right-hand end of the line.
    let latin = centroid_x(&ltr, |px| solid(px, true, true, true));
    let red = centroid_x(&ltr, |px| solid(px, true, false, false));
    assert!(latin < red, "the Latin word leads on the left of the Arabic run");
    let (left, right) = fill_extent(&ltr, 0, 1080);
    let stop_x = pixels(&ltr).filter(|(_, _, px)| solid(*px, true, true, true)).map(|(x, _, _)| x).max().unwrap();
    assert!(stop_x >= right - 2, "an LTR paragraph ends in its full stop ({stop_x} vs {right})");
    let abc_x = pixels(&ltr).filter(|(_, _, px)| solid(*px, true, true, true)).map(|(x, _, _)| x).min().unwrap();
    assert!(abc_x <= left + 2, "and starts with the Latin word");
    // Arabic first, then Latin: the Arabic run stands to the right of the Latin word.
    let mixed = text_frame(&picks, &[raw(2, &format!("{RED}{arabic}{WHITE} abc"))]);
    assert!(
        centroid_x(&mixed, |px| solid(px, true, false, false)) > centroid_x(&mixed, |px| solid(px, true, true, true)),
        "in an RTL paragraph the first run is the rightmost"
    );
}

/// The escape rules of `text_ass` (plan §1.4) against the real parser: what the viewer SEES.
#[test]
#[ignore = "requires the pinned native host libass artifact and packaged fonts"]
fn native_escapes_show_what_the_subtitle_says() {
    let picks = style(SubtitleSize::from_percent(125), SubtitlePosition::TvScreen);
    let frame = |text: &str| text_frame(&picks, &[cue(text)]);
    let extent = |rects: &[Rect]| {
        let (l, r) = fill_extent(rects, 0, 1080);
        r - l
    };
    let lines = |text: &str| fill_bands(&frame(text)).len();

    // `\N` and `\n` are line breaks (a literal `\n` reads as a space with WrapStyle 1).
    assert_eq!(lines("a\\Nb"), 2, "\\N");
    assert_eq!(lines("a\\nb"), 2, "\\n");
    assert_eq!(lines("C:\\new"), 2, "C:\\new is `C:` and `ew`");
    assert_eq!(lines("a\nb"), 2, "a real line feed");
    assert_eq!(lines("a\n\n\nb"), 2, "runs of empty lines collapse");
    // `\h` is a space that does not break: the same advance as a space.
    assert_eq!(frame("a\\hb"), frame("a b"), "\\h draws as a space");

    // A lone brace is shown, exactly as libass's own `\{` / `\}` escapes draw it.
    assert_eq!(frame("a{b"), text_frame(&picks, &[raw(2, "a\\{b")]), "a lone {{");
    assert_eq!(frame("a}b"), text_frame(&picks, &[raw(2, "a\\}b")]), "a lone }}");
    assert!(extent(&frame("a{b")) > extent(&frame("ab")) + 8, "the brace has ink and width");

    // A backslash that would otherwise glue to the next character is drawn, then the override
    // after it applies: `a\` + `{\i1}b` is a, a backslash, an italic b.
    let plain = extent(&frame("ab"));
    let slashed = frame("a\\{\\i1}b");
    assert!(extent(&slashed) > plain + 8, "the backslash is drawn: {} vs {plain}", extent(&slashed));
    assert_ne!(slashed, text_frame(&picks, &[raw(2, "a\\\u{200b}b")]), "and the b is italic");
    // `C:\{}` ends in a backslash: the matched empty block is dropped, as the bespoke draw did.
    assert!(extent(&frame("C:\\{}")) > extent(&frame("C:")) + 8, "C:\\ shows the backslash");
    assert_eq!(lines("C:\\{}"), 1);

    // Only the whitelist survives: placement and animation requests are dropped.
    assert_eq!(frame("{\\pos(100,100)}x"), frame("x"), "\\pos is dropped");
    assert_eq!(frame("{\\move(1,1,9,9)\\fs200}x"), frame("x"), "\\move and \\fs are dropped");
    // The whitelist does apply: italics change the glyph, a font colour changes the ink.
    assert_ne!(frame("<i>x</i>"), frame("x"), "italic");
    assert_ne!(frame("{\\i1}x"), frame("x"), "italic from an override block");
    let red = frame("<font color=\"#ff0000\">x</font>");
    assert!(pixels(&red).any(|(_, _, px)| solid(px, true, false, false)), "a red font colour");
    assert!(!pixels(&frame("x")).any(|(_, _, px)| solid(px, true, false, false)));
}

/// Plan §2.4 (amended by PR 5b): with the transport up, a text caption lifts by the band's `lift`,
/// and its deepest ink (Devanagari below-base signs, Arabic descenders, outline included) must still
/// end above the HUD's control row, the band's `controls_top_y`. The plan's first wording, "entirely
/// above the ceiling", cannot hold: at Low the anchor is 983 and classic's ceiling 780, so even Latin
/// "Hxg" ends past it. The row is what the lift has to clear; the corpus ends 3 px above it at Low.
/// The lift is `SUB_BASE_Y - ceil_y` and the ceiling sits a fixed clearance above the row, so the
/// margin is the same for every band: the test runs for classic's and for one that is not.
#[test]
#[ignore = "requires the pinned native host libass artifact and packaged fonts"]
fn native_hud_up_deep_marks_clear_the_transport_controls() {
    for band in [HudBand::CLASSIC, HudBand::from_controls_top(700.0)] {
        for position in POSITIONS {
            for size in SIZES {
                let picks = style(size, position);
                // Non-vacuity: the corpus reaches below what plain Latin with a descender does.
                let latin = ink_bottom(&text_frame(&picks, &[cue("Hxg")]));
                for (script, text) in [
                    ("Devanagari", "कृ दु ह्रू"),
                    ("Devanagari prose", "कृपया दुकान ह्रूँ गुरु"),
                    ("Arabic", "مجموعة الجيم والعين"),
                    ("Arabic descenders", "ليج جحخ عغ ڤ"),
                ] {
                    let depth = ink_bottom(&text_frame(&picks, &[cue(text)]));
                    let lifted = depth as f32 - band.lift - (text_style::BOTTOM_GAP - text_style::position_distance(position)).max(0.0);
                    assert!(
                        lifted <= band.controls_top_y,
                        "{script} {size:?} {position:?}: lifted ink ends at {lifted}, controls start at {}",
                        band.controls_top_y
                    );
                    if position == SubtitlePosition::TvScreen && size == SubtitleSize::from_percent(150) {
                        assert!(depth > latin, "{script}: no deeper than Latin \"Hxg\" ({depth} vs {latin})");
                    }
                }
            }
        }
    }
}
