//! The actions picked made a single video, to share: their pieces of the
//! video one after the other, each after a card telling the action (who,
//! the set, the score), and over them how the match stands, as on screen
//! (`overlay_view`). The actions of a team, match after match: each match
//! after a card of its own (the opponent, the day). On a thread: decoded (`player::Frames`), drawn on
//! (tiny-skia, the fonts and the icons of the app), encoded (openh264) and
//! made an MP4 as a recording is (`live::raw`). No sound: the player
//! decodes none.

use {
    super::{
        Standing,
        player::{Frame, Frames},
    },
    crate::{live::raw, theme},
    ab_glyph::{Font, FontRef, PxScale, ScaleFont, point},
    openh264::{
        OpenH264API,
        encoder::{BitRate, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, UsageType},
        formats::{RgbaSliceU8, YUVBuffer},
    },
    scout4all_core::shapes::enums::PhaseEnum,
    std::{
        path::{Path, PathBuf},
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
        thread,
    },
    tiny_skia::{
        Color, FillRule, FilterQuality, IntSize, Paint, PathBuilder, Pixmap, PixmapPaint, Rect,
        Transform,
    },
};

/// The video made is at most this tall: enough for a phone, light to send.
const HEIGHT: u32 = 720;
/// How long a card stays.
pub const CARD_MS: u64 = 2000;
/// The pixels a second of the video made gets.
const BITS_PER_SECOND: u32 = 3_500_000;

/// A piece of the video, from its time to its time, after its card.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub from_ms: u64,
    pub to_ms: u64,
    pub card: Card,
}

/// What a card tells: a line in large letters, then the others.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub title: String,
    pub lines: Vec<String>,
}

/// How the match stands over the video: what to draw at a time of it.
pub struct Board {
    pub team: String,
    pub opponent: String,
    pub set_n: &'static str,
    pub side_out: &'static str,
    pub break_point: &'static str,
    /// how the match stands at a time of the video
    pub at: Box<dyn Fn(u64) -> Option<Standing> + Send>,
}

/// What to make: the matches one after the other.
pub struct Plan {
    pub matches: Vec<MatchPlan>,
    pub output: PathBuf,
}

/// A match of the video made: its files, its pieces, how it stands.
pub struct MatchPlan {
    pub files: Vec<PathBuf>,
    pub pieces: Vec<Piece>,
    /// none: the video without it
    pub board: Option<Board>,
    /// the card before its pieces (against whom, when); none with a single
    /// match
    pub title: Option<Card>,
}

impl Plan {
    /// How long the video made is.
    fn total_ms(&self) -> u64 {
        self.matches
            .iter()
            .map(|m| {
                let title = m.title.as_ref().map_or(0, |_| CARD_MS);
                title
                    + m.pieces
                        .iter()
                        .map(|p| CARD_MS + p.to_ms.saturating_sub(p.from_ms))
                        .sum::<u64>()
            })
            .sum()
    }
}

/// How far it got.
#[derive(Debug, Clone, Default)]
pub struct Progress {
    pub done_ms: u64,
    pub total_ms: u64,
    /// the video made, or why not; none while making it
    pub result: Option<Result<PathBuf, String>>,
}

impl Progress {
    /// From 0 to 100.
    pub fn percent(&self) -> u64 {
        (self.done_ms * 100)
            .checked_div(self.total_ms)
            .unwrap_or(0)
            .min(100)
    }
}

/// The video being made, on its thread.
pub struct Montage {
    progress: Arc<Mutex<Progress>>,
    cancel: Arc<AtomicBool>,
}

impl Montage {
    pub fn start(plan: Plan) -> Montage {
        let total_ms = plan.total_ms();
        let progress = Arc::new(Mutex::new(Progress {
            total_ms,
            ..Progress::default()
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let (shared, stop) = (progress.clone(), cancel.clone());
        let spawned = thread::Builder::new()
            .name("montage".into())
            .spawn(move || {
                let result = make(&plan, &shared, &stop);
                if let Ok(mut p) = shared.lock() {
                    p.result = Some(result);
                }
            });
        if let (Err(e), Ok(mut p)) = (spawned, progress.lock()) {
            p.result = Some(Err(e.to_string()));
        }
        Montage { progress, cancel }
    }

    pub fn progress(&self) -> Progress {
        self.progress.lock().map(|p| p.clone()).unwrap_or_default()
    }

    /// Stopped: what was made left out.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for Montage {
    fn drop(&mut self) {
        self.cancel();
    }
}

const CANCELLED: &str = "cancelled";

fn make(plan: &Plan, progress: &Mutex<Progress>, cancel: &AtomicBool) -> Result<PathBuf, String> {
    // the size and the pace from the first frame of the first piece
    let (first_match, first) = plan
        .matches
        .iter()
        .find_map(|m| Some((m, m.pieces.first()?)))
        .ok_or("no actions")?;
    let mut frames = Frames::open(&first_match.files)?;
    let frame_ms = frames.frame_ms.max(1);
    frames.seek(first.from_ms)?;
    let (width, height) = match frames.next()? {
        Some((_, f)) => fitted(f.width, f.height),
        None => return Err("no frames".to_string()),
    };
    let config = EncoderConfig::new()
        .usage_type(UsageType::CameraVideoRealTime)
        .bitrate(BitRate::from_bps(BITS_PER_SECOND))
        .max_frame_rate(FrameRate::from_hz(1000.0 / frame_ms as f32))
        .intra_frame_period(IntraFramePeriod::from_num_frames(
            u32::try_from(2000 / frame_ms).unwrap_or(60).max(1),
        ));
    let mut encoder =
        Encoder::with_api_config(OpenH264API::from_source(), config).map_err(|e| e.to_string())?;
    let base = plan.output.with_extension("");
    let (stream, index) = raw::raw_paths(&base);
    let _ = std::fs::remove_file(&stream);
    let _ = std::fs::remove_file(&index);
    // the raw files gone however it ends
    let _raw = Removed(vec![stream, index]);
    let mut writer = raw::RawWriter::create(&base)?;
    let mut out_ms = 0u64;
    let mut done_ms = 0u64;
    let mut write = |pixmap: &Pixmap| -> Result<(), String> {
        let rgba = RgbaSliceU8::new(pixmap.data(), (width as usize, height as usize));
        let yuv = YUVBuffer::from_rgb_source(rgba);
        let bits = encoder.encode(&yuv).map_err(|e| e.to_string())?.to_vec();
        writer.write_shown(out_ms, 0, &bits)?;
        out_ms += frame_ms;
        Ok(())
    };
    let advance = |done: u64| {
        if let Ok(mut p) = progress.lock() {
            p.done_ms = done;
        }
    };
    let fonts = Fonts::new()?;
    let mut opened = Some(frames);
    for m in &plan.matches {
        // the frames of the first match already open
        let mut frames = match opened.take() {
            Some(frames) => frames,
            None => Frames::open(&m.files)?,
        };
        let painter = Painter {
            width,
            height,
            fonts: Fonts {
                regular: fonts.regular.clone(),
                bold: fonts.bold.clone(),
            },
            board: m.board.as_ref(),
        };
        let cards = m.title.iter().map(|t| (t, None)).chain(
            m.pieces
                .iter()
                .map(|p| (&p.card, Some((p.from_ms, p.to_ms)))),
        );
        for (card, piece) in cards {
            let card = painter.card(card)?;
            for _ in 0..CARD_MS / frame_ms {
                write(&card)?;
            }
            done_ms += CARD_MS;
            advance(done_ms);
            let Some((from_ms, to_ms)) = piece else {
                continue;
            };
            frames.seek(from_ms)?;
            // each frame shown until the next one's time: as many frames
            // as the piece lasts at the pace of the video made, also when
            // this match was filmed at another one
            let length = to_ms.saturating_sub(from_ms);
            let mut shown = 0u64;
            let mut last: Option<Pixmap> = None;
            while let Some((at, frame)) = frames.next()? {
                if cancel.load(Ordering::Relaxed) {
                    return Err(CANCELLED.to_string());
                }
                if at >= to_ms {
                    break;
                }
                let into = at.saturating_sub(from_ms);
                if let Some(pixmap) = &last {
                    while shown < into {
                        write(pixmap)?;
                        shown += frame_ms;
                    }
                }
                last = Some(painter.frame(&frame, at)?);
                advance(done_ms + into);
            }
            if let Some(pixmap) = &last {
                while shown < length {
                    write(pixmap)?;
                    shown += frame_ms;
                }
            }
            done_ms += length;
            advance(done_ms);
        }
    }
    writer.flush()?;
    drop(writer);
    raw::remux(&base, &plan.output).map(|_| plan.output.clone())
}

/// Files removed once dropped.
struct Removed(Vec<PathBuf>);

impl Drop for Removed {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// The size of the video made: as the frames, at most `HEIGHT` tall, even
/// (the encoder halves the colors).
fn fitted(width: u32, height: u32) -> (u32, u32) {
    let h = height.clamp(2, HEIGHT);
    let w = u64::from(width) * u64::from(h) / u64::from(height.max(1));
    let even = |n: u64| u32::try_from(n & !1).unwrap_or(2).max(2);
    (even(w), even(u64::from(h)))
}

struct Fonts {
    regular: FontRef<'static>,
    bold: FontRef<'static>,
}

impl Fonts {
    fn new() -> Result<Fonts, String> {
        let [regular, bold] = theme::fonts().bytes;
        Ok(Fonts {
            regular: FontRef::try_from_slice(regular).map_err(|e| e.to_string())?,
            bold: FontRef::try_from_slice(bold).map_err(|e| e.to_string())?,
        })
    }
}

struct Painter<'a> {
    width: u32,
    height: u32,
    fonts: Fonts,
    board: Option<&'a Board>,
}

fn rgb(c: iced::Color) -> Color {
    Color::from_rgba(c.r, c.g, c.b, c.a).unwrap_or(Color::WHITE)
}

impl Painter<'_> {
    /// How much larger than at 720 lines.
    fn k(&self) -> f32 {
        self.height as f32 / 720.0
    }

    fn blank(&self) -> Result<Pixmap, String> {
        Pixmap::new(self.width, self.height).ok_or_else(|| "no picture".to_string())
    }

    /// The card of an action: its lines in the middle, on the background of
    /// the app, the ball above.
    fn card(&self, card: &Card) -> Result<Pixmap, String> {
        let mut pixmap = self.blank()?;
        let colors = theme::DARK;
        pixmap.fill(rgb(colors.background));
        let k = self.k();
        let (w, h) = (self.width as f32, self.height as f32);
        let ball = 72.0 * k;
        icon(
            &mut pixmap,
            theme::BALL,
            (w - ball) / 2.0,
            h * 0.22,
            ball,
            None,
        );
        let title = 46.0 * k;
        let line = 30.0 * k;
        let mut y = h * 0.22 + ball + 70.0 * k;
        let width = text_width(&self.fonts.bold, title, &card.title);
        draw_text(
            &mut pixmap,
            &self.fonts.bold,
            title,
            (w - width) / 2.0,
            y,
            rgb(colors.accent),
            &card.title,
        );
        y += title * 0.6 + line * 1.4;
        for (i, l) in card.lines.iter().enumerate() {
            let color = match i + 1 == card.lines.len() && card.lines.len() > 1 {
                true => rgb(colors.muted),
                false => rgb(colors.text),
            };
            let width = text_width(&self.fonts.regular, line, l);
            draw_text(
                &mut pixmap,
                &self.fonts.regular,
                line,
                (w - width) / 2.0,
                y,
                color,
                l,
            );
            y += line * 1.5;
        }
        Ok(pixmap)
    }

    /// A frame of the video at its size, how the match stands over it. A
    /// frame of another shape (a match filmed otherwise) whole, on black
    /// bars.
    fn frame(&self, frame: &Frame, at_ms: u64) -> Result<Pixmap, String> {
        let mut pixmap = self.blank()?;
        pixmap.fill(Color::BLACK);
        let size = IntSize::from_wh(frame.width, frame.height).ok_or("no frame")?;
        let source = Pixmap::from_vec(frame.rgba.to_vec(), size).ok_or("no frame")?;
        let (w, h) = (self.width as f32, self.height as f32);
        let (fw, fh) = (frame.width.max(1) as f32, frame.height.max(1) as f32);
        let scale = (w / fw).min(h / fh);
        let transform = Transform::from_translate((w - fw * scale) / 2.0, (h - fh * scale) / 2.0)
            .pre_scale(scale, scale);
        let paint = PixmapPaint {
            quality: FilterQuality::Bilinear,
            ..PixmapPaint::default()
        };
        pixmap.draw_pixmap(0, 0, source.as_ref(), &paint, transform, None);
        if let Some(board) = self.board
            && let Some(st) = (board.at)(at_ms)
        {
            self.board(&mut pixmap, board, &st);
        }
        Ok(pixmap)
    }

    /// As `overlay_view`: a line a team (the ball by the one to serve, the
    /// name, the sets, the points), the set and the phase beside, on a
    /// veil in the corner.
    fn board(&self, pixmap: &mut Pixmap, board: &Board, st: &Standing) {
        let k = self.k();
        let colors = theme::DARK;
        let white = Color::WHITE;
        let soft = Color::from_rgba(1.0, 1.0, 1.0, 0.8).unwrap_or(white);
        let (body, score) = (22.0 * k, 30.0 * k);
        let (pad_x, pad_y, gap) = (20.0 * k, 12.0 * k, 14.0 * k);
        let names = [
            super::overlay_name(&board.team),
            super::overlay_name(&board.opponent),
        ];
        let name_w = names
            .iter()
            .map(|n| text_width(&self.fonts.bold, body, n))
            .fold(0.0, f32::max);
        let digit_body = text_width(&self.fonts.regular, body, "0");
        let digit_score = text_width(&self.fonts.bold, score, "0");
        let ball = 22.0 * k;
        let (phase_color, phase_icon, phase_name) = match st.phase {
            PhaseEnum::Break => (colors.break_point, theme::icons::BREAK, board.break_point),
            PhaseEnum::SideOut => (colors.side_out, theme::icons::SIDE_OUT, board.side_out),
        };
        let set = format!("{} {}", board.set_n, st.set_number);
        let phase_w = 26.0 * k
            + text_width(&self.fonts.bold, body, phase_name)
            + gap
            + text_width(&self.fonts.bold, body, &st.rotation);
        let right_w = phase_w.max(text_width(&self.fonts.regular, body, &set));
        let left_w = ball + gap + name_w + gap + digit_body * 2.0 + gap + digit_score * 2.0;
        let row_h = score * 1.15;
        let (x0, y0) = (24.0 * k, 24.0 * k);
        let box_w = pad_x * 2.0 + left_w + gap * 2.0 + 1.0 + right_w;
        let box_h = pad_y * 2.0 + row_h * 2.0;
        fill_rounded(
            pixmap,
            x0,
            y0,
            box_w,
            box_h,
            12.0 * k,
            Color::from_rgba8(0, 0, 0, 140),
        );
        let serving_us = st.phase == PhaseEnum::Break;
        let rows = [
            (serving_us, &names[0], st.sets_us, st.score_us),
            (!serving_us, &names[1], st.sets_them, st.score_them),
        ];
        for (i, (serving, name, sets, points)) in rows.into_iter().enumerate() {
            let middle = y0 + pad_y + row_h * (i as f32 + 0.5);
            let baseline = |size: f32| middle + size * 0.35;
            let mut x = x0 + pad_x;
            if serving {
                icon(
                    pixmap,
                    theme::icons::VOLLEYBALL,
                    x,
                    middle - ball / 2.0,
                    ball,
                    Some(white),
                );
            }
            x += ball + gap;
            draw_text(
                pixmap,
                &self.fonts.bold,
                body,
                x,
                baseline(body),
                white,
                name,
            );
            x += name_w + gap;
            let sets = sets.to_string();
            let sw = text_width(&self.fonts.regular, body, &sets);
            draw_text(
                pixmap,
                &self.fonts.regular,
                body,
                x + digit_body * 2.0 - sw,
                baseline(body),
                soft,
                &sets,
            );
            x += digit_body * 2.0 + gap;
            let points = points.to_string();
            let pw = text_width(&self.fonts.bold, score, &points);
            draw_text(
                pixmap,
                &self.fonts.bold,
                score,
                x + digit_score * 2.0 - pw,
                baseline(score),
                white,
                &points,
            );
        }
        // the line between, then the set and the phase
        let line_x = x0 + pad_x + left_w + gap;
        fill_rounded(
            pixmap,
            line_x,
            y0 + pad_y,
            1.0_f32.max(k),
            row_h * 2.0,
            0.0,
            Color::from_rgba(1.0, 1.0, 1.0, 0.35).unwrap_or(white),
        );
        let x = line_x + gap;
        let middle = |i: f32| y0 + pad_y + row_h * (i + 0.5);
        draw_text(
            pixmap,
            &self.fonts.regular,
            body,
            x,
            middle(0.0) + body * 0.35,
            soft,
            &set,
        );
        let color = rgb(phase_color);
        let icon_size = 22.0 * k;
        icon(
            pixmap,
            phase_icon,
            x,
            middle(1.0) - icon_size / 2.0,
            icon_size,
            Some(color),
        );
        let name_x = x + 26.0 * k;
        draw_text(
            pixmap,
            &self.fonts.bold,
            body,
            name_x,
            middle(1.0) + body * 0.35,
            color,
            phase_name,
        );
        let rot_x = name_x + text_width(&self.fonts.bold, body, phase_name) + gap;
        draw_text(
            pixmap,
            &self.fonts.bold,
            body,
            rot_x,
            middle(1.0) + body * 0.35,
            white,
            &st.rotation,
        );
    }
}

fn fill_rounded(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, color: Color) {
    let r = r.min(w / 2.0).min(h / 2.0);
    let path = match r > 0.0 {
        true => {
            let mut pb = PathBuilder::new();
            pb.move_to(x + r, y);
            pb.line_to(x + w - r, y);
            pb.quad_to(x + w, y, x + w, y + r);
            pb.line_to(x + w, y + h - r);
            pb.quad_to(x + w, y + h, x + w - r, y + h);
            pb.line_to(x + r, y + h);
            pb.quad_to(x, y + h, x, y + h - r);
            pb.line_to(x, y + r);
            pb.quad_to(x, y, x + r, y);
            pb.close();
            pb.finish()
        }
        false => Rect::from_xywh(x, y, w, h).map(PathBuilder::from_rect),
    };
    let mut paint = Paint::default();
    paint.set_color(color);
    paint.anti_alias = true;
    if let Some(path) = path {
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// An icon of the app (an SVG of 24 × 24) at `size`, in `color` (none: its
/// own colors).
fn icon(pixmap: &mut Pixmap, svg: &[u8], x: f32, y: f32, size: f32, color: Option<Color>) {
    let svg = match color {
        // the color of the root, and of the paths in black
        Some(c) => {
            let c = c.to_color_u8();
            let hex = format!("#{:02x}{:02x}{:02x}", c.red(), c.green(), c.blue());
            String::from_utf8_lossy(svg)
                .replace("fill=\"#000\"", &format!("fill=\"{hex}\""))
                .replacen("<svg ", &format!("<svg fill=\"{hex}\" "), 1)
        }
        None => String::from_utf8_lossy(svg).into_owned(),
    };
    let Ok(tree) = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()) else {
        return;
    };
    let own = tree.size();
    let scale = size / own.width().max(own.height()).max(1.0);
    let transform = Transform::from_translate(x, y).pre_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
}

fn text_width(font: &FontRef<'_>, size: f32, text: &str) -> f32 {
    let scaled = font.as_scaled(PxScale::from(size));
    text.chars()
        .map(|c| scaled.h_advance(scaled.glyph_id(c)))
        .sum()
}

/// The text from `x`, on the line at `baseline`, blended in.
fn draw_text(
    pixmap: &mut Pixmap,
    font: &FontRef<'_>,
    size: f32,
    x: f32,
    baseline: f32,
    color: Color,
    text: &str,
) {
    let scaled = font.as_scaled(PxScale::from(size));
    let (width, height) = (pixmap.width() as i64, pixmap.height() as i64);
    let c = color.to_color_u8();
    let alpha = f32::from(c.alpha()) / 255.0;
    let pixels = pixmap.pixels_mut();
    let mut caret = x;
    for ch in text.chars() {
        let id = scaled.glyph_id(ch);
        let glyph = id.with_scale_and_position(PxScale::from(size), point(caret, baseline));
        caret += scaled.h_advance(id);
        let Some(outlined) = font.outline_glyph(glyph) else {
            continue;
        };
        let bounds = outlined.px_bounds();
        outlined.draw(|gx, gy, coverage| {
            let px = bounds.min.x as i64 + i64::from(gx);
            let py = bounds.min.y as i64 + i64::from(gy);
            if px < 0 || py < 0 || px >= width || py >= height {
                return;
            }
            let Ok(i) = usize::try_from(py * width + px) else {
                return;
            };
            let Some(pixel) = pixels.get_mut(i) else {
                return;
            };
            let a = (coverage * alpha).clamp(0.0, 1.0);
            let blend =
                |src: u8, dst: u8| (f32::from(src) * a + f32::from(dst) * (1.0 - a)).round() as u8;
            let (r, g, b) = (
                blend(c.red(), pixel.red()),
                blend(c.green(), pixel.green()),
                blend(c.blue(), pixel.blue()),
            );
            if let Some(p) = tiny_skia::PremultipliedColorU8::from_rgba(r, g, b, 255) {
                *pixel = p;
            }
        });
    }
}

/// The file the video goes to: in the folder of the files to share, named
/// after what it shows.
pub fn output(title: &str) -> PathBuf {
    let name = scout4all_mobile::video::montage_name(title);
    std::env::temp_dir()
        .join("scout4all-share")
        .join(format!("{name}.mp4"))
}

/// The folder of `output`, made if missing.
pub fn prepare(output: &Path) -> Result<(), String> {
    match output.parent() {
        Some(dir) => std::fs::create_dir_all(dir).map_err(|e| e.to_string()),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::source::Source;

    const CLIP: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/video/testdata/two_seconds.mp4"
    );
    /// 2 s at 15 frames a second, 120 × 96
    const OTHER_PACE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/video/testdata/two_seconds_15fps.mp4"
    );

    fn standing(set: u8) -> Standing {
        Standing {
            set_number: set,
            sets_us: 1,
            sets_them: 0,
            score_us: 12,
            score_them: 10,
            rotation: "P3".into(),
            phase: PhaseEnum::SideOut,
        }
    }

    fn board() -> Board {
        Board {
            team: "CORTONA VOLLEY 2026".into(),
            opponent: "POLISPORTIVA SAVINESE".into(),
            set_n: "Set",
            side_out: "SIDE-OUT",
            break_point: "BREAK",
            at: Box::new(|_| Some(standing(2))),
        }
    }

    fn card(title: &str) -> Card {
        Card {
            title: title.into(),
            lines: vec![
                "13 Alessandro Brilli".into(),
                "Set 2 · 12–10".into(),
                "1 / 2".into(),
            ],
        }
    }

    /// Two pieces of the video, each after its card: a single MP4 as long
    /// as the cards and the pieces, at the size of the frames (smaller than
    /// 720 lines).
    #[test]
    fn the_actions_made_a_video() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("Attacchi.mp4");
        let plan = Plan {
            matches: vec![MatchPlan {
                files: vec![PathBuf::from(CLIP), PathBuf::from(CLIP)],
                pieces: vec![
                    Piece {
                        from_ms: 500,
                        to_ms: 1500,
                        card: card("Attacco · F"),
                    },
                    // across the two files
                    Piece {
                        from_ms: 1500,
                        to_ms: 3000,
                        card: card("Battuta"),
                    },
                ],
                board: Some(board()),
                title: None,
            }],
            output: output.clone(),
        };
        let progress = Mutex::new(Progress {
            total_ms: 2 * CARD_MS + 2500,
            ..Progress::default()
        });
        let made = make(&plan, &progress, &AtomicBool::new(false)).unwrap();
        assert_eq!(made, output);
        assert_eq!(progress.lock().unwrap().percent(), 100);
        let video = Source::open(&output).unwrap();
        let expected = 2 * CARD_MS + 2500;
        assert!(
            video.duration_ms.abs_diff(expected) <= 100,
            "{} ms, not {expected}",
            video.duration_ms
        );
        // the raw files gone
        let left: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(left.len(), 1);
    }

    /// The actions of two matches, the second one filmed otherwise (15
    /// frames a second, 4:3): each match after its card, the pieces as long
    /// as they are in their match, at the size and the pace of the first.
    #[test]
    fn the_actions_of_two_matches_made_a_video() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("Tour.mp4");
        let title = |opponent: &str| Card {
            title: opponent.into(),
            lines: vec!["2026-10-12".into(), "Partita 1 di 2".into()],
        };
        let plan = Plan {
            matches: vec![
                MatchPlan {
                    files: vec![PathBuf::from(CLIP)],
                    pieces: vec![Piece {
                        from_ms: 0,
                        to_ms: 1000,
                        card: card("Attacco"),
                    }],
                    board: Some(board()),
                    title: Some(title("Siena")),
                },
                MatchPlan {
                    files: vec![PathBuf::from(OTHER_PACE)],
                    pieces: vec![
                        Piece {
                            from_ms: 0,
                            to_ms: 800,
                            card: card("Attacco"),
                        },
                        Piece {
                            from_ms: 1000,
                            to_ms: 2000,
                            card: card("Attacco"),
                        },
                    ],
                    board: Some(board()),
                    title: Some(title("Arezzo")),
                },
            ],
            output: output.clone(),
        };
        let expected = 5 * CARD_MS + 1000 + 800 + 1000;
        assert_eq!(plan.total_ms(), expected);
        let progress = Mutex::new(Progress {
            total_ms: expected,
            ..Progress::default()
        });
        make(&plan, &progress, &AtomicBool::new(false)).unwrap();
        assert_eq!(progress.lock().unwrap().percent(), 100);
        let video = Source::open(&output).unwrap();
        assert!(
            video.duration_ms.abs_diff(expected) <= 100,
            "{} ms, not {expected}",
            video.duration_ms
        );
        // at the pace of the first: 30 frames a second all along
        let frames = video.samples.len() as u64;
        assert!(
            frames.abs_diff(expected * 30 / 1000) <= 3,
            "{frames} frames"
        );
    }

    /// A frame of another shape whole in the video made, on black bars.
    #[test]
    fn a_frame_of_another_shape_on_bars() {
        let painter = Painter {
            width: 160,
            height: 96,
            fonts: Fonts::new().unwrap(),
            board: None,
        };
        let frame = Frame {
            serial: 0,
            width: 120,
            height: 96,
            rgba: [255, 255, 255, 255].repeat(120 * 96).into(),
        };
        let pixmap = painter.frame(&frame, 0).unwrap();
        let at = |x: u32, y: u32| pixmap.pixel(x, y).unwrap();
        // black on the sides, the frame in the middle, as tall as the video
        assert_eq!(at(5, 48).red(), 0);
        assert_eq!(at(154, 48).red(), 0);
        assert_eq!(at(80, 1).red(), 255);
        assert_eq!(at(80, 94).red(), 255);
    }

    /// Stopped: no video.
    #[test]
    fn a_video_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let plan = Plan {
            matches: vec![MatchPlan {
                files: vec![PathBuf::from(CLIP)],
                pieces: vec![Piece {
                    from_ms: 0,
                    to_ms: 1000,
                    card: card("Attacco"),
                }],
                board: None,
                title: None,
            }],
            output: dir.path().join("x.mp4"),
        };
        let progress = Mutex::new(Progress::default());
        let made = make(&plan, &progress, &AtomicBool::new(true));
        assert_eq!(made, Err(CANCELLED.to_string()));
        // nothing left
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn the_size_of_the_video_made() {
        assert_eq!(fitted(1920, 1080), (1280, 720));
        assert_eq!(fitted(160, 96), (160, 96));
        assert_eq!(fitted(1440, 1081), (958, 720));
    }

    #[test]
    fn the_file_named_after_what_it_shows() {
        let path = output("Attacchi di 13 Brilli: dal 20");
        assert_eq!(
            path.file_name().unwrap(),
            "Attacchi di 13 Brilli dal 20.mp4"
        );
        assert_eq!(output("/").file_name().unwrap(), "scout4all.mp4");
    }

    /// A card and a frame with the match over it, as PNG, to look at
    /// (`MONTAGE_PNG=folder`).
    #[test]
    fn the_pictures_to_look_at() {
        let Ok(folder) = std::env::var("MONTAGE_PNG") else {
            return;
        };
        let fonts = Fonts::new().unwrap();
        let board = board();
        let painter = Painter {
            width: 1280,
            height: 720,
            fonts,
            board: Some(&board),
        };
        painter
            .card(&card("Attacco · F  #"))
            .unwrap()
            .save_png(Path::new(&folder).join("card.png"))
            .unwrap();
        let frame = Frame {
            serial: 0,
            width: 160,
            height: 90,
            rgba: [70, 120, 60, 255].repeat(160 * 90).into(),
        };
        painter
            .frame(&frame, 0)
            .unwrap()
            .save_png(Path::new(&folder).join("frame.png"))
            .unwrap();
    }
}
