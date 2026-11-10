//! The strip of the clips of the actions picked, under the bar of the
//! video, the same on the desktop and on Android: where each clip is, the
//! ones overlapping as one, where each is drawn across the strip and what a
//! click (a touch) on it asks for. The apps draw it and send the clicks.

use crate::{VideoEvent, count_of, video::target};

/// The narrowest a clip is drawn: a short one, on a narrow strip, still
/// seen.
pub const MIN_WIDTH: f32 = 3.0;

/// How far from a clip a click of the mouse still takes it, in pixels: a
/// short one is easy to hit.
pub const MOUSE_REACH: f32 = 4.0;

/// How far from a clip a touch still takes it, in dp: a finger is wider
/// than the pointer of a mouse.
pub const TOUCH_REACH: f32 = 12.0;

/// A clip of the actions picked, in the video: the ones overlapping as
/// one, from the first action in it (`event`, in the list).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ClipSpan {
    pub from_ms: u64,
    pub to_ms: u64,
    pub playing: bool,
    pub event: u32,
}

/// Where a clip is drawn across the strip, from its start to its end.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct StripBar {
    pub from: f32,
    pub to: f32,
}

/// What a click on the strip asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum StripClick {
    /// the clip of this action (in the list), from its start
    Clip { event: u32 },
    /// the video at this point
    At { ms: u64 },
}

/// Where the clips of `events` (the actions picked) are in the video, from
/// `before_ms` before each to `after_ms` after it: the ones overlapping as
/// one, in order. The one of the action `chosen` (or of one at its time)
/// is the one playing.
pub fn clip_spans<E: std::borrow::Borrow<VideoEvent>>(
    events: &[E],
    before_ms: i64,
    after_ms: u64,
    chosen: usize,
) -> Vec<ClipSpan> {
    let chosen = events
        .get(chosen)
        .and_then(|e| target(e.borrow(), before_ms));
    let mut spans: Vec<ClipSpan> = events
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let e = e.borrow();
            let from = target(e, before_ms)?;
            let end = e.offset_ms?.max(0).unsigned_abs() + after_ms;
            Some(ClipSpan {
                from_ms: from,
                to_ms: end,
                playing: Some(from) == chosen,
                event: count_of(i),
            })
        })
        .collect();
    spans.sort_by_key(|sp| sp.from_ms);
    spans
        .into_iter()
        .fold(vec![], |mut merged: Vec<ClipSpan>, sp| {
            match merged.last_mut() {
                Some(last) if sp.from_ms <= last.to_ms => {
                    last.to_ms = last.to_ms.max(sp.to_ms);
                    last.playing |= sp.playing;
                }
                _ => merged.push(sp),
            }
            merged
        })
}

/// Where each clip is drawn across a strip `width` wide, from its start
/// to its end: at least `MIN_WIDTH`, inside the strip even at the end of
/// the video (or past it).
#[uniffi::export]
pub fn strip_bars(spans: &[ClipSpan], duration_ms: u64, width: f32) -> Vec<StripBar> {
    let x = |ms: u64| {
        (ms.min(duration_ms) as f64 / duration_ms.max(1) as f64 * f64::from(width)) as f32
    };
    let last = (width - MIN_WIDTH).max(0.0);
    spans
        .iter()
        .map(|sp| {
            let from = x(sp.from_ms).min(last);
            StripBar {
                from,
                to: x(sp.to_ms).max(from + MIN_WIDTH),
            }
        })
        .collect()
}

/// What the click at `x`, across a strip `width` wide, asks for: the clip
/// nearest within `reach` (`MOUSE_REACH`, `TOUCH_REACH`), otherwise the
/// video at that point.
#[uniffi::export]
pub fn strip_clicked(
    spans: &[ClipSpan],
    duration_ms: u64,
    width: f32,
    x: f32,
    reach: f32,
) -> StripClick {
    let distance = |bar: StripBar| (bar.from - x).max(x - bar.to).max(0.0);
    let near = strip_bars(spans, duration_ms, width)
        .into_iter()
        .zip(spans)
        .map(|(bar, sp)| (distance(bar), sp.event))
        .filter(|(d, _)| *d <= reach)
        .min_by(|a, b| a.0.total_cmp(&b.0));
    match near {
        Some((_, event)) => StripClick::Clip { event },
        None => {
            let share = f64::from((x / width.max(1.0)).clamp(0.0, 1.0));
            StripClick::At {
                ms: (share * duration_ms as f64).round() as u64,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(from_ms: u64, to_ms: u64, event: u32) -> ClipSpan {
        ClipSpan {
            from_ms,
            to_ms,
            playing: false,
            event,
        }
    }

    fn bars(spans: &[ClipSpan], duration: u64, width: f32) -> Vec<(f32, f32)> {
        strip_bars(spans, duration, width)
            .into_iter()
            .map(|b| (b.from, b.to))
            .collect()
    }

    /// The clips where they are across the strip, a short one still seen,
    /// inside it at the end of the video and past it.
    #[test]
    fn the_bars_of_the_strip() {
        let spans = [span(1000, 2000, 3), span(5000, 5001, 7)];
        assert_eq!(
            bars(&spans, 10_000, 1000.0),
            [(100.0, 200.0), (500.0, 503.0)]
        );
        // narrow: the short one still 3 wide
        assert_eq!(bars(&spans, 10_000, 100.0), [(10.0, 20.0), (50.0, 53.0)]);
        // at the end and past it: inside the strip
        assert_eq!(
            bars(
                &[span(10_000, 10_000, 0), span(9000, 99_000, 1)],
                10_000,
                100.0
            ),
            [(97.0, 100.0), (90.0, 100.0)]
        );
        // no video yet, no strip wide: nothing divided by zero
        assert_eq!(bars(&spans, 0, 0.0), [(0.0, 3.0), (0.0, 3.0)]);
    }

    /// A click on a clip or near it takes it, between the clips the point
    /// of the video; the nearer of two within reach.
    #[test]
    fn the_strip_clicked() {
        let spans = [span(1000, 2000, 3), span(5000, 5001, 7)];
        let click = |width, x| strip_clicked(&spans, 10_000, width, x, MOUSE_REACH);
        let clip = |event| StripClick::Clip { event };
        let at = |ms| StripClick::At { ms };
        assert_eq!(click(1000.0, 150.0), clip(3));
        // the short one, on it and a little beside it
        assert_eq!(click(1000.0, 501.0), clip(7));
        assert_eq!(click(1000.0, 497.0), clip(7));
        assert_eq!(click(1000.0, 506.0), clip(7));
        assert_eq!(click(1000.0, 508.0), at(5080));
        // between the clips, before the first, after the last
        assert_eq!(click(1000.0, 300.0), at(3000));
        assert_eq!(click(1000.0, 0.0), at(0));
        assert_eq!(click(1000.0, 1000.0), at(10_000));
        assert_eq!(click(1000.0, 1200.0), at(10_000));
        // narrow: the same points, nearer to each other
        assert_eq!(click(100.0, 15.0), clip(3));
        assert_eq!(click(100.0, 55.0), clip(7));
        assert_eq!(click(100.0, 35.0), at(3500));
        let close = [span(1000, 1100, 1), span(1300, 1400, 2)];
        assert_eq!(
            strip_clicked(&close, 10_000, 1000.0, 112.0, MOUSE_REACH),
            clip(1)
        );
        assert_eq!(
            strip_clicked(&close, 10_000, 1000.0, 127.0, MOUSE_REACH),
            clip(2)
        );
        // no video yet, no strip wide
        assert_eq!(strip_clicked(&spans, 0, 0.0, 0.0, MOUSE_REACH), clip(3));
        assert_eq!(strip_clicked(&[], 0, 0.0, 0.0, MOUSE_REACH), at(0));
        // a finger: farther, the nearer of two still
        let touch = |x| strip_clicked(&spans, 10_000, 1000.0, x, TOUCH_REACH);
        assert_eq!(touch(511.0), clip(7));
        assert_eq!(touch(486.0), at(4860));
        assert_eq!(
            strip_clicked(&close, 10_000, 1000.0, 114.0, TOUCH_REACH),
            clip(1)
        );
        assert_eq!(
            strip_clicked(&close, 10_000, 1000.0, 126.0, TOUCH_REACH),
            clip(2)
        );
    }
}
