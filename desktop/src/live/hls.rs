//! A source of video over HLS: a playlist (`.m3u8`) on a web server, its
//! segments (MPEG-TS or fragmented MP4) fetched as they're made and their
//! H.264 recorded as it is, as for RTSP and RTMP. A master playlist: the
//! highest quality in H.264, a lower one when its segments come slower
//! than they play. HLS comes some seconds late (a segment or more): the
//! frames placed when they were taken, from the time of their segment
//! (`PROGRAM-DATE-TIME`) or, without it, as soon as it could have been
//! made; what's left is shifted in the video screen (`[` `]`).

use {
    super::{
        network::{
            ANSWER_WITHIN, Asked, Io, LOOK_EVERY, NetworkSource, Problem, credentials,
            frame_annex_b, length_size, parameters_annex_b,
        },
        raw::nals,
    },
    chrono::{DateTime, FixedOffset, Utc},
    std::{
        collections::VecDeque,
        time::{Duration, Instant},
    },
};

/// The clock of MPEG-TS.
const TS_CLOCK: u64 = 90_000;
/// MPEG-TS times count up to 2³³, then from 0 again.
const TS_WRAP: u64 = 1 << 33;

/// Segments fetched slower than this share of how long they play, so many
/// in a row: a lower quality.
const SLOW_SHARE: f64 = 0.8;
const SLOW_IN_A_ROW: u32 = 3;

/// A late frame told by its segment's time beyond this: the clocks of the
/// server and of this computer apart, the time not trusted.
const LATE_AT_MOST: Duration = Duration::from_secs(120);

/// A quality of a master playlist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub uri: String,
    pub bandwidth: u64,
    /// H.265 only, as its codecs say: not played
    pub hevc: bool,
}

/// A segment of a media playlist.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub sequence: u64,
    pub uri: String,
    pub duration_ms: u64,
    /// when its first frame was taken, as the server says
    pub date: Option<DateTime<FixedOffset>>,
    /// its initialization (fragmented MP4)
    pub map: Option<String>,
    /// its times not following the ones before
    pub discontinuity: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Playlist {
    /// the qualities, the best first
    Master(Vec<Variant>),
    Media {
        target_ms: u64,
        segments: Vec<Segment>,
        /// no more segments to come: not live
        ended: bool,
    },
}

/// The attributes of a tag (`A=1,B="x,y"`).
fn attributes(list: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    let mut rest = list;
    while let Some((name, after)) = rest.split_once('=') {
        let (value, next) = match after.strip_prefix('"') {
            Some(quoted) => match quoted.split_once('"') {
                Some((v, n)) => (v, n.strip_prefix(',').unwrap_or(n)),
                None => (quoted, ""),
            },
            None => match after.split_once(',') {
                Some((v, n)) => (v, n),
                None => (after, ""),
            },
        };
        out.push((name.trim().to_string(), value.to_string()));
        rest = next;
    }
    out
}

fn date(text: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(text.trim())
        .or_else(|_| DateTime::parse_from_str(text.trim(), "%Y-%m-%dT%H:%M:%S%.f%z"))
        .ok()
}

/// A playlist read, if it is one.
pub fn playlist(text: &str) -> Option<Playlist> {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    if lines.next() != Some("#EXTM3U") {
        return None;
    }
    let mut variants: Vec<Variant> = vec![];
    let mut variant: Option<(u64, bool)> = None;
    let mut segments: Vec<Segment> = vec![];
    let mut target_ms = 0;
    let mut sequence = 0;
    let mut duration_ms: Option<u64> = None;
    let mut next_date: Option<DateTime<FixedOffset>> = None;
    let mut map: Option<String> = None;
    let mut discontinuity = false;
    let mut ended = false;
    let mut master = false;
    for line in lines {
        let (tag, value) = line.split_once(':').unwrap_or((line, ""));
        match tag {
            "#EXT-X-STREAM-INF" => {
                master = true;
                let attrs = attributes(value);
                let get = |n: &str| attrs.iter().find(|(k, _)| k == n).map(|(_, v)| v.as_str());
                let bandwidth = get("BANDWIDTH").and_then(|b| b.parse().ok()).unwrap_or(0);
                let codecs = get("CODECS").unwrap_or("avc1").to_lowercase();
                let hevc = (codecs.contains("hvc1") || codecs.contains("hev1"))
                    && !codecs.contains("avc1")
                    && !codecs.contains("avc3");
                variant = Some((bandwidth, hevc));
            }
            "#EXT-X-TARGETDURATION" => target_ms = value.trim().parse::<u64>().unwrap_or(0) * 1000,
            "#EXT-X-MEDIA-SEQUENCE" => sequence = value.trim().parse().unwrap_or(0),
            "#EXTINF" => {
                let seconds = value.split(',').next().unwrap_or("").trim();
                duration_ms = seconds
                    .parse::<f64>()
                    .ok()
                    .map(|s| (s * 1000.0).round() as u64);
            }
            "#EXT-X-PROGRAM-DATE-TIME" => next_date = date(value),
            "#EXT-X-MAP" => {
                map = attributes(value)
                    .into_iter()
                    .find(|(k, _)| k == "URI")
                    .map(|(_, v)| v);
            }
            "#EXT-X-DISCONTINUITY" => discontinuity = true,
            "#EXT-X-ENDLIST" => ended = true,
            _ if line.starts_with('#') => {}
            uri => match (variant.take(), duration_ms.take()) {
                (Some((bandwidth, hevc)), _) => variants.push(Variant {
                    uri: uri.to_string(),
                    bandwidth,
                    hevc,
                }),
                (None, Some(duration_ms)) => {
                    // the time of the one before, carried on
                    let date = next_date.take().or_else(|| {
                        segments.last().and_then(|s: &Segment| {
                            s.date.map(|d| {
                                d + chrono::TimeDelta::milliseconds(
                                    i64::try_from(s.duration_ms).unwrap_or(0),
                                )
                            })
                        })
                    });
                    segments.push(Segment {
                        sequence,
                        uri: uri.to_string(),
                        duration_ms,
                        date,
                        map: map.clone(),
                        discontinuity: std::mem::take(&mut discontinuity),
                    });
                    sequence += 1;
                }
                (None, None) => {}
            },
        }
    }
    match master {
        true => {
            variants.sort_by_key(|v| std::cmp::Reverse(v.bandwidth));
            Some(Playlist::Master(variants))
        }
        false => Some(Playlist::Media {
            target_ms,
            segments,
            ended,
        }),
    }
}

/// A frame of a segment, its times on the clock of the stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Annex B, its parameters before it when it's a keyframe
    pub annex_b: Vec<u8>,
    pub key: bool,
    pub dts: u64,
    pub pts: u64,
}

/// The NAL units of a frame as Annex B, without the delimiters between
/// frames (the MP4 doesn't want them); whether it's a keyframe.
fn cleaned(annex_b: &[u8]) -> (Vec<u8>, bool) {
    let mut out = Vec::with_capacity(annex_b.len() + 16);
    let mut key = false;
    for nal in nals(annex_b) {
        let kind = nal.first().map(|b| b & 0x1f);
        key |= kind == Some(5);
        if kind != Some(9) {
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(nal);
        }
    }
    (out, key)
}

/// The H.264 frames of an MPEG-TS segment, times on its 90 kHz clock
/// (not yet unwrapped).
pub fn ts_frames(data: &[u8]) -> Result<Vec<Frame>, Problem> {
    let start = data.iter().position(|b| *b == 0x47).unwrap_or(0);
    let mut pmt: Option<u16> = None;
    let mut video: Option<u16> = None;
    let mut hevc = false;
    let mut pes: Vec<u8> = vec![];
    let mut frames: Vec<Frame> = vec![];
    for packet in data.get(start..).unwrap_or_default().as_chunks::<188>().0 {
        let (Some(0x47), Some(b1), Some(b2), Some(b3)) =
            (packet.first(), packet.get(1), packet.get(2), packet.get(3))
        else {
            continue;
        };
        let pid = (u16::from(b1 & 0x1f) << 8) | u16::from(*b2);
        let starts = b1 & 0x40 != 0;
        let adaptation = (b3 >> 4) & 3;
        let offset = match adaptation & 2 {
            0 => 4,
            _ => 5 + usize::from(packet.get(4).copied().unwrap_or(0)),
        };
        let payload = match adaptation & 1 {
            0 => continue,
            _ => packet.get(offset..).unwrap_or_default(),
        };
        match (pid, pmt, video) {
            (0, _, _) if starts => pmt = pat(payload).or(pmt),
            (p, Some(m), None) if p == m && starts => {
                let (found, h265) = pmt_video(payload);
                video = found;
                hevc |= h265;
            }
            (p, _, Some(v)) if p == v => {
                if starts {
                    push_pes(&pes, &mut frames);
                    pes.clear();
                }
                pes.extend_from_slice(payload);
            }
            _ => {}
        }
    }
    push_pes(&pes, &mut frames);
    match (video, hevc) {
        (Some(_), _) => Ok(frames),
        (None, true) => Err(Problem::Hevc),
        (None, false) => Err(Problem::NoVideo(String::new())),
    }
}

/// A section of a table: from its pointer, the bytes after its header
/// up to its CRC.
fn section(payload: &[u8], header: usize) -> Option<&[u8]> {
    let pointer = usize::from(*payload.first()?);
    let table = payload.get(1 + pointer..)?;
    let length = (usize::from(*table.get(1)? & 0x0f) << 8) | usize::from(*table.get(2)?);
    table.get(3 + header..(3 + length).checked_sub(4)?)
}

/// The PID of the first program's table.
fn pat(payload: &[u8]) -> Option<u16> {
    section(payload, 5)?
        .as_chunks::<4>()
        .0
        .iter()
        .find(|[a, b, _, _]| (*a, *b) != (0, 0))
        .map(|[_, _, high, low]| (u16::from(high & 0x1f) << 8) | u16::from(*low))
}

/// The PID of the H.264 video of a program; whether it has H.265.
fn pmt_video(payload: &[u8]) -> (Option<u16>, bool) {
    let Some(body) = section(payload, 5) else {
        return (None, false);
    };
    let info = match body.get(2..4) {
        Some(&[high, low]) => (usize::from(high & 0x0f) << 8) | usize::from(low),
        _ => 0,
    };
    let mut at = 4 + info;
    let mut hevc = false;
    while let Some(&[kind, pid_high, pid_low, info_high, info_low]) = body.get(at..at + 5) {
        let pid = (u16::from(pid_high & 0x1f) << 8) | u16::from(pid_low);
        let es_info = (usize::from(info_high & 0x0f) << 8) | usize::from(info_low);
        match kind {
            0x1b => return (Some(pid), hevc),
            0x24 => hevc = true,
            _ => {}
        }
        at += 5 + es_info;
    }
    (None, hevc)
}

fn ts_time(b: &[u8]) -> Option<u64> {
    let b: &[u8; 5] = b.get(..5)?.try_into().ok()?;
    Some(
        (u64::from((b[0] >> 1) & 7) << 30)
            | (u64::from(b[1]) << 22)
            | (u64::from(b[2] >> 1) << 15)
            | (u64::from(b[3]) << 7)
            | u64::from(b[4] >> 1),
    )
}

/// A packet of the video made a frame; without times, the rest of the
/// frame before.
fn push_pes(pes: &[u8], frames: &mut Vec<Frame>) {
    let (Some([0, 0, 1]), Some(flags), Some(header)) = (pes.get(..3), pes.get(7), pes.get(8))
    else {
        return;
    };
    let data = pes.get(9 + usize::from(*header)..).unwrap_or_default();
    let pts = match flags & 0x80 {
        0 => None,
        _ => pes.get(9..).and_then(ts_time),
    };
    let dts = match flags & 0x40 {
        0 => pts,
        _ => pes.get(14..).and_then(ts_time),
    };
    match (pts, dts, frames.last_mut()) {
        (Some(pts), Some(dts), _) => {
            let (annex_b, key) = cleaned(data);
            frames.push(Frame {
                annex_b,
                key,
                dts,
                pts,
            });
        }
        (_, _, Some(last)) => {
            let (more, key) = cleaned(data);
            last.annex_b.extend_from_slice(&more);
            last.key |= key;
        }
        _ => {}
    }
}

/// The video of a fragmented MP4, from its initialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Init {
    track: u32,
    pub timescale: u32,
    parameters: Vec<u8>,
    length_size: usize,
    default_duration: u32,
    default_size: u32,
}

/// The boxes in `data`: their kind, their content and where they start.
fn boxes(data: &[u8]) -> Vec<([u8; 4], &[u8], usize)> {
    let mut out = vec![];
    let mut at = 0;
    while let (Some(size), Some(kind)) = (u32_at(data, at), data.get(at + 4..at + 8)) {
        let size = u64::from(size);
        let (header, size) = match size {
            1 => match data.get(at + 8..at + 16).and_then(|b| b.try_into().ok()) {
                Some(b) => (16, u64::from_be_bytes(b)),
                None => break,
            },
            0 => (8, (data.len() - at) as u64),
            s => (8, s),
        };
        let end = at.saturating_add(usize::try_from(size).unwrap_or(usize::MAX));
        let (Some(body), Ok(kind)) = (data.get(at + header..end), <[u8; 4]>::try_from(kind)) else {
            break;
        };
        out.push((kind, body, at));
        at = end;
    }
    out
}

fn child<'a>(data: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(data)
        .into_iter()
        .find(|(k, _, _)| k == kind)
        .map(|(_, b, _)| b)
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(data: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(data.get(at..at + 8)?.try_into().ok()?))
}

/// The video track of an initialization segment.
pub fn init(data: &[u8]) -> Result<Init, Problem> {
    let moov = child(data, b"moov").ok_or_else(|| Problem::NoVideo(String::new()))?;
    let mut hevc = false;
    for (kind, trak, _) in boxes(moov) {
        let mdia = (&kind == b"trak").then(|| child(trak, b"mdia")).flatten();
        let Some(mdia) = mdia else { continue };
        if child(mdia, b"hdlr").and_then(|h| h.get(8..12)) != Some(b"vide") {
            continue;
        }
        let tkhd = child(trak, b"tkhd").unwrap_or_default();
        let track = match tkhd.first() {
            Some(1) => u32_at(tkhd, 20),
            _ => u32_at(tkhd, 12),
        };
        let mdhd = child(mdia, b"mdhd").unwrap_or_default();
        let timescale = match mdhd.first() {
            Some(1) => u32_at(mdhd, 20),
            _ => u32_at(mdhd, 12),
        };
        let stsd = child(mdia, b"minf")
            .and_then(|m| child(m, b"stbl"))
            .and_then(|s| child(s, b"stsd"))
            .and_then(|s| s.get(8..))
            .unwrap_or_default();
        for (entry, body, _) in boxes(stsd) {
            match &entry {
                b"avc1" | b"avc3" => {
                    let avcc = body.get(78..).and_then(|b| child(b, b"avcC"));
                    let (Some(track), Some(timescale), Some(avcc)) = (track, timescale, avcc)
                    else {
                        continue;
                    };
                    let trex = child(moov, b"mvex")
                        .map(boxes)
                        .unwrap_or_default()
                        .into_iter()
                        .find(|(k, b, _)| k == b"trex" && u32_at(b, 4) == Some(track))
                        .map(|(_, b, _)| b)
                        .unwrap_or_default();
                    return Ok(Init {
                        track,
                        timescale: timescale.max(1),
                        parameters: parameters_annex_b(avcc).unwrap_or_default(),
                        length_size: length_size(avcc).unwrap_or(4),
                        default_duration: u32_at(trex, 12).unwrap_or(0),
                        default_size: u32_at(trex, 16).unwrap_or(0),
                    });
                }
                b"hvc1" | b"hev1" => hevc = true,
                _ => {}
            }
        }
    }
    match hevc {
        true => Err(Problem::Hevc),
        false => Err(Problem::NoVideo(String::new())),
    }
}

/// The frames of the video in a segment of fragmented MP4, times on the
/// clock of its track.
pub fn fmp4_frames(data: &[u8], init: &Init) -> Vec<Frame> {
    let mut frames = vec![];
    for (kind, moof, moof_at) in boxes(data) {
        if &kind != b"moof" {
            continue;
        }
        for (kind, traf, _) in boxes(moof) {
            if &kind != b"traf" {
                continue;
            }
            traf_frames(data, moof_at, traf, init, &mut frames);
        }
    }
    frames
}

fn traf_frames(data: &[u8], moof_at: usize, traf: &[u8], init: &Init, frames: &mut Vec<Frame>) {
    let Some(tfhd) = child(traf, b"tfhd") else {
        return;
    };
    let flags = u32_at(tfhd, 0).unwrap_or(0) & 0xff_ffff;
    if u32_at(tfhd, 4) != Some(init.track) {
        return;
    }
    let mut at = 8;
    let mut take = |present: bool, wide: bool| -> Option<u64> {
        match (present, wide) {
            (false, _) => None,
            (true, true) => {
                at += 8;
                u64_at(tfhd, at - 8)
            }
            (true, false) => {
                at += 4;
                u32_at(tfhd, at - 4).map(u64::from)
            }
        }
    };
    let base = take(flags & 0x1 != 0, true);
    take(flags & 0x2 != 0, false);
    let duration = take(flags & 0x8 != 0, false).map_or(init.default_duration, |d| d as u32);
    let size = take(flags & 0x10 != 0, false).map_or(init.default_size, |s| s as u32);
    let base = base.map_or(moof_at, |b| usize::try_from(b).unwrap_or(usize::MAX));
    let mut decoded = child(traf, b"tfdt").map_or(0, |t| match t.first() {
        Some(1) => u64_at(t, 4).unwrap_or(0),
        _ => u32_at(t, 4).map_or(0, u64::from),
    });
    let mut next_data = base;
    for (kind, trun, _) in boxes(traf) {
        if &kind != b"trun" {
            continue;
        }
        let flags = u32_at(trun, 0).unwrap_or(0) & 0xff_ffff;
        let count = u32_at(trun, 4).unwrap_or(0);
        let mut at = 8;
        if flags & 0x1 != 0 {
            let offset = u32_at(trun, at).unwrap_or(0) as i32;
            next_data = base.saturating_add_signed(offset as isize);
            at += 4;
        }
        if flags & 0x4 != 0 {
            at += 4;
        }
        for _ in 0..count {
            let mut field = |present: bool, default: u32| match present {
                true => {
                    at += 4;
                    u32_at(trun, at - 4).unwrap_or(default)
                }
                false => default,
            };
            let d = field(flags & 0x100 != 0, duration);
            let s = field(flags & 0x200 != 0, size);
            field(flags & 0x400 != 0, 0);
            let shown_after = field(flags & 0x800 != 0, 0) as i32;
            let s = usize::try_from(s).unwrap_or(0);
            let sample = data.get(next_data..next_data.saturating_add(s));
            next_data = next_data.saturating_add(s);
            let annex_b = sample.and_then(|b| frame_annex_b(b, init.length_size, None));
            if let Some(annex_b) = annex_b {
                let (mut annex_b, key) = cleaned(&annex_b);
                if key {
                    annex_b.splice(0..0, init.parameters.iter().copied());
                }
                frames.push(Frame {
                    annex_b,
                    key,
                    dts: decoded,
                    pts: decoded.saturating_add_signed(i64::from(shown_after)),
                });
            }
            decoded = decoded.saturating_add(u64::from(d));
        }
    }
}

/// The times of MPEG-TS counted on past 2³³.
#[derive(Default)]
struct Unwrap {
    last: Option<u64>,
    wraps: u64,
}

impl Unwrap {
    fn of(&mut self, t: u64) -> u64 {
        if let Some(last) = self.last
            && t + TS_WRAP / 2 < last
        {
            self.wraps += 1;
        }
        self.last = Some(t);
        t + self.wraps * TS_WRAP
    }

    /// The presentation time, near the decoding one already unwrapped.
    fn near(dts: u64, pts: u64) -> u64 {
        let base = dts - dts % TS_WRAP;
        let p = base + pts % TS_WRAP;
        match () {
            () if p + TS_WRAP / 2 < dts => p + TS_WRAP,
            () if p > dts + TS_WRAP / 2 => p.saturating_sub(TS_WRAP),
            () => p,
        }
    }
}

/// A frame waiting for its turn, its times in ms.
struct Queued {
    annex_b: Vec<u8>,
    key: bool,
    dts_ms: u64,
    shown_after_ms: i32,
    /// when it was taken, as far as known
    taken: Instant,
    /// the first after a jump in the times
    discontinuity: bool,
}

/// When the frames of a segment were taken: from its time, trusted when
/// the clocks look alike; else at the latest, the segment just made when
/// listed (`seen`).
fn taken_at(
    segment: &Segment,
    offset_ms: u64,
    seen: Instant,
    now: DateTime<Utc>,
    now_instant: Instant,
) -> Instant {
    let told = segment.date.and_then(|d| {
        let at = d + chrono::TimeDelta::milliseconds(i64::try_from(offset_ms).unwrap_or(0));
        let late = (now - at.with_timezone(&Utc)).to_std().ok()?;
        (late <= LATE_AT_MOST).then(|| now_instant.checked_sub(late))?
    });
    let made = Duration::from_millis(segment.duration_ms.saturating_sub(offset_ms));
    told.unwrap_or_else(|| seen.checked_sub(made).unwrap_or(seen))
}

/// The pace the frames are given at: as they play, for the preview.
#[derive(Default)]
struct Pacer {
    /// a frame's time and when it was given
    anchor: Option<(u64, Instant)>,
}

impl Pacer {
    /// When the frame at `dts_ms` is due; taken anew after a jump, or
    /// a second behind.
    fn due(&mut self, dts_ms: u64, now: Instant) -> Instant {
        let at = self.anchor.and_then(|(ms, at)| {
            let due = at + Duration::from_millis(dts_ms.checked_sub(ms)?);
            (due + Duration::from_secs(1) >= now).then_some(due)
        });
        at.unwrap_or_else(|| {
            self.anchor = Some((dts_ms, now));
            now
        })
    }
}

struct Getter {
    client: reqwest::Client,
    user: Option<(String, String)>,
}

impl Getter {
    async fn bytes(&self, url: &url::Url) -> Result<bytes::Bytes, Problem> {
        self.fetched(url).await.map(|(bytes, _)| bytes)
    }

    /// The bytes at `url`, and where they were in the end (redirected):
    /// what's written in them is from there.
    async fn fetched(&self, url: &url::Url) -> Result<(bytes::Bytes, url::Url), Problem> {
        let request = self.client.get(url.clone());
        let request = match &self.user {
            Some((name, password)) => request.basic_auth(name, Some(password)),
            None => request,
        };
        let response = request.send().await.map_err(|e| http_problem(&e))?;
        let at = response.url().clone();
        match response.status().as_u16() {
            200..=299 => Ok((response.bytes().await.map_err(|e| http_problem(&e))?, at)),
            401 | 403 => Err(Problem::Refused),
            404 => Err(Problem::NotFound),
            code => Err(Problem::Other(format!("HTTP {code}"))),
        }
    }

    async fn playlist(&self, url: &url::Url) -> Result<(Playlist, url::Url), Problem> {
        let (bytes, at) = self.fetched(url).await?;
        let read = playlist(&String::from_utf8_lossy(&bytes));
        read.map(|p| (p, at))
            .ok_or_else(|| Problem::NoVideo(String::new()))
    }
}

fn http_problem(e: &reqwest::Error) -> Problem {
    match () {
        () if e.is_timeout() => Problem::Silent,
        () if e.is_connect() => Problem::Unreachable,
        () => super::network::problem_of(&e.to_string()),
    }
}

/// The qualities of the address, the best first: a media playlist alone,
/// or the H.264 ones of a master playlist.
async fn qualities(get: &Getter, url: &url::Url) -> Result<Vec<url::Url>, Problem> {
    let (read, url) = get.playlist(url).await?;
    match read {
        Playlist::Media { .. } => Ok(vec![url]),
        Playlist::Master(variants) => {
            let playable: Vec<url::Url> = variants
                .iter()
                .filter(|v| !v.hevc)
                .filter_map(|v| url.join(&v.uri).ok())
                .collect();
            match (playable.is_empty(), variants.is_empty()) {
                (false, _) => Ok(playable),
                (true, false) => Err(Problem::Hevc),
                (true, true) => Err(Problem::NoVideo(String::new())),
            }
        }
    }
}

/// What's read of the segments, from one to the next.
#[derive(Default)]
struct Reading {
    /// the last segment fetched
    last: Option<u64>,
    init: Option<(url::Url, Init)>,
    unwrap: Unwrap,
    queue: VecDeque<Queued>,
    /// the next segment after a jump (a new quality, segments missed)
    jumps: bool,
    slow: u32,
}

/// Connected over HLS and read until cut off or asked to quit.
pub(super) async fn pull(
    url: url::Url,
    source: &NetworkSource,
    in_url: Option<(String, String)>,
    io: &mut Io<'_>,
) -> Result<Asked, Problem> {
    let client = reqwest::Client::builder()
        .user_agent("scout4all")
        .timeout(ANSWER_WITHIN)
        .build()
        .map_err(|e| Problem::Other(e.to_string()))?;
    let get = Getter {
        client,
        user: credentials(source, in_url),
    };
    let qualities = qualities(&get, &url).await?;
    let mut quality = 0;
    let mut reading = Reading::default();
    let mut pacer = Pacer::default();
    let mut reload = Instant::now();
    let mut new_since = Instant::now();
    let mut target = Duration::from_secs(2);
    io.connected();
    loop {
        if let Asked::Quit = io.look() {
            return Ok(Asked::Quit);
        }
        let now = Instant::now();
        if now >= reload {
            let Some(media) = qualities.get(quality) else {
                return Err(Problem::NoVideo(String::new()));
            };
            let (read, media) = get.playlist(media).await?;
            let media = &media;
            let (target_ms, segments, ended) = match read {
                Playlist::Media {
                    target_ms,
                    segments,
                    ended,
                } => (target_ms, segments, ended),
                Playlist::Master(_) => return Err(Problem::NoVideo(String::new())),
            };
            target = Duration::from_millis(target_ms.max(1000));
            // not live: nothing to record
            if ended && reading.last.is_none() {
                return Err(Problem::Ended);
            }
            // from the newest segment, to be as little late as can be
            let first = reading
                .last
                .unwrap_or_else(|| segments.last().map_or(0, |s| s.sequence.saturating_sub(1)));
            reading.last.get_or_insert(first);
            let fresh: Vec<&Segment> = segments.iter().filter(|s| s.sequence > first).collect();
            if fresh.first().is_some_and(|s| s.sequence > first + 1) {
                reading.jumps = true;
            }
            for segment in &fresh {
                fetch(&get, media, segment, &mut reading).await?;
                if let Asked::Quit = io.look() {
                    return Ok(Asked::Quit);
                }
            }
            if reading.slow >= SLOW_IN_A_ROW && quality + 1 < qualities.len() {
                quality += 1;
                reading.slow = 0;
                reading.jumps = true;
                // the same segments in the other quality, from the next
            }
            // over, once its last frames are given
            if ended && fresh.is_empty() && reading.queue.is_empty() {
                return Err(Problem::Ended);
            }
            if !fresh.is_empty() {
                new_since = Instant::now();
            }
            if new_since.elapsed() >= ANSWER_WITHIN.max(target * 3) {
                return Err(Problem::Silent);
            }
            reload = Instant::now()
                + match fresh.is_empty() {
                    true => target / 2,
                    false => target,
                };
        }
        // the frames due, at the pace they play; far behind (a slow
        // network), all at once down to a segment
        let behind = match (reading.queue.front(), reading.queue.back()) {
            (Some(f), Some(b)) => {
                Duration::from_millis(b.dts_ms.saturating_sub(f.dts_ms)) > target * 3
            }
            _ => false,
        };
        let mut wait = LOOK_EVERY;
        while let Some(front) = reading.queue.front() {
            let now = Instant::now();
            let due = pacer.due(front.dts_ms, now);
            let span = match reading.queue.back() {
                Some(b) => Duration::from_millis(b.dts_ms.saturating_sub(front.dts_ms)),
                None => Duration::ZERO,
            };
            if due > now && !(behind && span > target) {
                wait = wait.min(due - now);
                break;
            }
            let Some(q) = reading.queue.pop_front() else {
                break;
            };
            if q.discontinuity {
                io.connected();
                pacer.anchor = None;
            }
            let late = u64::try_from(now.saturating_duration_since(q.taken).as_millis())
                .unwrap_or(u64::MAX);
            io.late(late);
            io.frame(&q.annex_b, q.key, q.dts_ms, q.shown_after_ms);
        }
        let until_reload = reload.saturating_duration_since(Instant::now());
        tokio::time::sleep(wait.min(until_reload).max(Duration::from_millis(1))).await;
    }
}

/// A segment fetched and its frames queued; how fast it came, counted.
async fn fetch(
    get: &Getter,
    media: &url::Url,
    segment: &Segment,
    reading: &mut Reading,
) -> Result<(), Problem> {
    let started = Instant::now();
    let url = media.join(&segment.uri).map_err(|_| Problem::Invalid)?;
    let init_url = match &segment.map {
        Some(map) => Some(media.join(map).map_err(|_| Problem::Invalid)?),
        None => None,
    };
    let known = reading.init.as_ref().map(|(u, _)| u);
    if let Some(init_url) = init_url.as_ref().filter(|u| Some(*u) != known) {
        let bytes = get.bytes(init_url).await?;
        reading.init = Some((init_url.clone(), init(&bytes)?));
    }
    let data = get.bytes(&url).await?;
    let seen = Instant::now();
    let took = seen.duration_since(started);
    reading.slow = match took.as_secs_f64() > segment.duration_ms as f64 / 1000.0 * SLOW_SHARE {
        true => reading.slow + 1,
        false => 0,
    };
    let (frames, clock) = match (&init_url, &reading.init) {
        (Some(_), Some((_, init))) => (fmp4_frames(&data, init), u64::from(init.timescale)),
        _ => {
            let frames = ts_frames(&data)?
                .into_iter()
                .map(|f| {
                    let dts = reading.unwrap.of(f.dts);
                    Frame {
                        pts: Unwrap::near(dts, f.pts),
                        dts,
                        ..f
                    }
                })
                .collect();
            (frames, TS_CLOCK)
        }
    };
    let ms = |t: u64| u64::try_from(u128::from(t) * 1000 / u128::from(clock.max(1))).unwrap_or(0);
    let first_dts = frames.first().map_or(0, |f| ms(f.dts));
    let (now, now_instant) = (Utc::now(), Instant::now());
    let mut jumps = std::mem::take(&mut reading.jumps) || segment.discontinuity;
    for f in frames {
        let dts_ms = ms(f.dts);
        let pts_ms = ms(f.pts);
        let offset = pts_ms.saturating_sub(first_dts);
        reading.queue.push_back(Queued {
            annex_b: f.annex_b,
            key: f.key,
            dts_ms,
            shown_after_ms: i32::try_from(pts_ms.saturating_sub(dts_ms)).unwrap_or(0),
            taken: taken_at(segment, offset, seen, now, now_instant),
            discontinuity: std::mem::take(&mut jumps),
        });
    }
    reading.last = Some(segment.sequence);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MASTER: &str = "#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360,CODECS=\"avc1.4d401e,mp4a.40.2\"
low/index.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=5000000,RESOLUTION=1920x1080,CODECS=\"hvc1.1.6.L120.90\"
hevc/index.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=2500000,RESOLUTION=1280x720,CODECS=\"avc1.64001f,mp4a.40.2\"
high/index.m3u8
";

    const MEDIA: &str = "#EXTM3U
#EXT-X-VERSION:9
#EXT-X-TARGETDURATION:2
#EXT-X-MEDIA-SEQUENCE:41
#EXT-X-MAP:URI=\"init.mp4\"
#EXT-X-PROGRAM-DATE-TIME:2026-11-10T14:00:00.000Z
#EXTINF:2.00000,
seg41.mp4
#EXT-X-PART:DURATION=0.2,URI=\"part.mp4\"
#EXTINF:1.5,
seg42.mp4
#EXT-X-DISCONTINUITY
#EXT-X-PROGRAM-DATE-TIME:2026-11-10T14:00:10.000+00:00
#EXTINF:2,
seg43.mp4
";

    /// The qualities of a master playlist, the best first, H.265 known;
    /// the segments of a media one with their times and initialization.
    #[test]
    fn the_playlists_read() {
        let Some(Playlist::Master(variants)) = playlist(MASTER) else {
            panic!("not a master playlist");
        };
        let order: Vec<(&str, bool)> = variants.iter().map(|v| (v.uri.as_str(), v.hevc)).collect();
        assert_eq!(
            order,
            [
                ("hevc/index.m3u8", true),
                ("high/index.m3u8", false),
                ("low/index.m3u8", false)
            ]
        );
        let Some(Playlist::Media {
            target_ms,
            segments,
            ended,
        }) = playlist(MEDIA)
        else {
            panic!("not a media playlist");
        };
        assert_eq!((target_ms, ended), (2000, false));
        let seen: Vec<(u64, &str, u64, bool)> = segments
            .iter()
            .map(|s| (s.sequence, s.uri.as_str(), s.duration_ms, s.discontinuity))
            .collect();
        assert_eq!(
            seen,
            [
                (41, "seg41.mp4", 2000, false),
                (42, "seg42.mp4", 1500, false),
                (43, "seg43.mp4", 2000, true)
            ]
        );
        let dates: Vec<String> = segments
            .iter()
            .map(|s| {
                s.date
                    .unwrap()
                    .with_timezone(&Utc)
                    .format("%H:%M:%S%.3f")
                    .to_string()
            })
            .collect();
        assert_eq!(dates, ["14:00:00.000", "14:00:02.000", "14:00:10.000"]);
        assert!(
            segments
                .iter()
                .all(|s| s.map.as_deref() == Some("init.mp4"))
        );
        assert!(matches!(
            playlist("#EXTM3U\n#EXTINF:2,\na.ts\n#EXT-X-ENDLIST\n"),
            Some(Playlist::Media { ended: true, .. })
        ));
        assert_eq!(playlist("<html>"), None);
    }

    /// Checks the frames of the two seconds of test video (64×48, 30
    /// fps, B-frames, a keyframe each second), times in ms from `ms`.
    fn two_seconds(frames: &[Frame], ms: impl Fn(u64) -> u64) {
        assert_eq!(frames.len(), 60);
        assert_eq!(frames.iter().filter(|f| f.key).count(), 2);
        assert!(frames[0].key && frames[30].key);
        // the keyframes with their parameters, no delimiters
        for f in frames {
            let kinds: Vec<u8> = nals(&f.annex_b).iter().map(|n| n[0] & 0x1f).collect();
            assert!(!kinds.contains(&9), "{kinds:?}");
            assert_eq!(kinds.contains(&7), f.key, "{kinds:?}");
        }
        // decoded in order, a frame every 33 ms; shown out of it
        let dts: Vec<u64> = frames.iter().map(|f| ms(f.dts)).collect();
        assert!(
            dts.windows(2).all(|w| (33..=34).contains(&(w[1] - w[0]))),
            "{dts:?}"
        );
        assert!(frames.windows(2).any(|w| w[1].pts < w[0].pts));
        let mut shown: Vec<u64> = frames.iter().map(|f| ms(f.pts)).collect();
        shown.sort_unstable();
        assert!(
            shown.windows(2).all(|w| (33..=34).contains(&(w[1] - w[0]))),
            "{shown:?}"
        );
        // each one a picture, as the preview decodes them
        let mut decoder = crate::live::network::preview_decoder().unwrap();
        let pictures = frames
            .iter()
            .filter(|f| decoder.decode(&f.annex_b).ok().flatten().is_some())
            .count();
        assert!(pictures >= 57, "{pictures}");
    }

    #[test]
    fn the_frames_of_an_mpeg_ts_segment() {
        let data = include_bytes!("testdata/two_seconds.ts");
        let frames = ts_frames(data).unwrap();
        two_seconds(&frames, |t| t * 1000 / TS_CLOCK);
        // the stream started 10 s in
        assert!((frames[0].dts * 1000 / TS_CLOCK).abs_diff(10_000) < 1500);
        assert_eq!(
            ts_frames(&[0x47; 188 * 3]),
            Err(Problem::NoVideo(String::new()))
        );
    }

    #[test]
    fn the_frames_of_a_fragmented_mp4_segment() {
        let init = init(include_bytes!("testdata/two_seconds_init.mp4")).unwrap();
        assert_eq!(init.timescale, 15360);
        let frames = fmp4_frames(include_bytes!("testdata/two_seconds.m4s"), &init);
        two_seconds(&frames, |t| t * 1000 / 15360);
        assert_eq!(fmp4_frames(b"nothing", &init), vec![]);
        assert!(super::init(b"nothing").is_err());
    }

    /// The times of MPEG-TS counted on when they start again from 0, the
    /// presentation ones next to them.
    #[test]
    fn the_times_past_their_wrap() {
        let mut unwrap = Unwrap::default();
        assert_eq!(unwrap.of(TS_WRAP - 3000), TS_WRAP - 3000);
        assert_eq!(unwrap.of(0), TS_WRAP);
        assert_eq!(unwrap.of(3000), TS_WRAP + 3000);
        assert_eq!(Unwrap::near(TS_WRAP - 3000, 3000), TS_WRAP + 3000);
        assert_eq!(Unwrap::near(TS_WRAP + 1000, TS_WRAP - 2000), TS_WRAP - 2000);
        assert_eq!(Unwrap::near(5000, 8000), 8000);
    }

    /// Taken when the segment's time says, if near; else as soon as the
    /// segment could have been made: its last frame when listed.
    #[test]
    fn when_the_frames_were_taken() {
        let now = Utc::now();
        let at = Instant::now();
        let segment = |date: Option<DateTime<Utc>>| Segment {
            sequence: 0,
            uri: String::new(),
            duration_ms: 2000,
            date: date.map(|d| d.fixed_offset()),
            map: None,
            discontinuity: false,
        };
        let eight_ago = segment(Some(now - chrono::TimeDelta::seconds(8)));
        let taken = taken_at(&eight_ago, 500, at, now, at);
        assert_eq!(at.duration_since(taken).as_millis(), 7500);
        for far in [
            None,
            Some(now - chrono::TimeDelta::hours(3)),
            Some(now + chrono::TimeDelta::seconds(5)),
        ] {
            let taken = taken_at(&segment(far), 500, at, now, at);
            assert_eq!(at.duration_since(taken).as_millis(), 1500, "{far:?}");
        }
    }

    /// Recorded from the test server (mediamtx in Docker serving HLS on
    /// 18888, `scout`/`segreta`, ffmpeg sending H.264 with B-frames on
    /// `/campo`; `S4A_HLS` for another playlist). The video as long as the
    /// frames recorded, started as much before as they came late.
    /// `cargo test -p scout4all-desktop -- --ignored hls`.
    #[test]
    #[ignore]
    fn hls_recorded_from_the_test_server() {
        use crate::live::capture::{Capture, Source, Status};
        let url = std::env::var("S4A_HLS")
            .unwrap_or_else(|_| "http://127.0.0.1:18888/campo/index.m3u8".to_string());
        let source = |password: &str| {
            Capture::open(Source::Network(NetworkSource {
                url: url.clone(),
                username: "scout".into(),
                password: password.into(),
            }))
        };
        let wait = |capture: &Capture, until: &dyn Fn(&Status) -> bool| {
            (0..300).any(|_| {
                std::thread::sleep(Duration::from_millis(100));
                capture.status().is_some_and(|s| until(&s))
            })
        };
        let wrong = source("sbagliata");
        assert!(wait(&wrong, &|s| s.problem == Some(Problem::Refused)));
        let capture = source("segreta");
        assert!(wait(&capture, &|s| s.frame.is_some()));
        let status = capture.status().unwrap();
        assert_eq!(status.size, Some((1280, 720)));
        let late = status.late_ms.unwrap();
        assert!((500..30_000).contains(&late), "{late}");
        let dir = tempfile::tempdir().unwrap();
        let started = chrono::Local::now();
        capture.record(dir.path().join("hls"), started, Default::default());
        capture.preview(false);
        std::thread::sleep(Duration::from_secs(25));
        capture.stop();
        assert!(wait(&capture, &|s| !s.done.is_empty()));
        let done = capture.take_done();
        let made = done[0].result.clone().unwrap();
        let video = crate::video::source::Source::open(&done[0].mp4).unwrap();
        let frames = video.samples.len() as u64;
        // a frame each 33 ms, the video as long
        assert!(
            made.length_ms.abs_diff(frames * 1000 / 30) < 200,
            "{made:?} {frames}"
        );
        assert!(
            frames * 1000 / 30 + made.skipped_ms >= 22_000,
            "{made:?} {frames}"
        );
        // started before it was asked to: as late as the frames came
        let before = (started - done[0].started).num_milliseconds();
        let late = i64::try_from(late).unwrap();
        let skipped = i64::try_from(made.skipped_ms).unwrap();
        eprintln!("late {late} ms, {made:?}, {frames} frames, started {before} ms before");
        assert!(
            (before - (late - skipped)).abs() < 1500,
            "{before} {late} {made:?}"
        );
        let kept = std::env::temp_dir().join("s4a-hls-test.mp4");
        std::fs::copy(&done[0].mp4, &kept).unwrap();
    }

    /// The frames given as they play: on time after the first, again
    /// from the frame after a jump or once a second behind.
    #[test]
    fn the_pace_of_the_frames() {
        let mut pacer = Pacer::default();
        let t0 = Instant::now();
        assert_eq!(pacer.due(1000, t0), t0);
        assert_eq!(pacer.due(1033, t0), t0 + Duration::from_millis(33));
        // back in time: anew
        assert_eq!(pacer.due(10, t0), t0);
        // two seconds late: anew
        let later = t0 + Duration::from_secs(3);
        assert_eq!(pacer.due(1010, later), later);
    }
}
