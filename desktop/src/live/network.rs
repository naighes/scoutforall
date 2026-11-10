//! A source of video on the network: RTSP (an IP camera, a phone app, a
//! streamer), with its user and password; RTMP read from a server, or
//! received from an app (`rtmp`). The H.264 that comes is recorded as it
//! is (no encoding again) and decoded only for the preview. Cut off while
//! recording, it connects again and goes on in the same recording. HLS is
//! known, not read yet.

use {
    super::{
        capture::{Command, Status, Take, finish, set},
        picture::{self, I420},
    },
    crate::video::Picture as Frame,
    bytes::Bytes,
    iced::futures::StreamExt,
    openh264::{decoder::Decoder, formats::YUVSource},
    retina::{
        client::{
            Credentials, PlayOptions, Session, SessionOptions, SetupOptions, TcpTransportOptions,
            Transport,
        },
        codec::{CodecItem, ParametersRef},
    },
    std::{
        sync::{
            Arc, Mutex,
            mpsc::{Receiver, TryRecvError},
        },
        time::{Duration, Instant},
    },
};

/// How wide the preview is.
const PREVIEW_WIDTH: usize = 960;

/// How long to wait for the source to answer.
pub(super) const ANSWER_WITHIN: Duration = Duration::from_secs(10);

/// How long to wait before connecting again, once cut off.
const AGAIN_AFTER: Duration = Duration::from_secs(2);

/// How often the commands are looked at while no frame comes.
pub(super) const LOOK_EVERY: Duration = Duration::from_millis(100);

/// The ports of the protocols, when not written.
const RTSP_PORT: u16 = 554;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Rtsp,
    Rtmp,
    Hls,
}

impl Protocol {
    pub fn name(self) -> &'static str {
        match self {
            Protocol::Rtsp => "RTSP",
            Protocol::Rtmp => "RTMP",
            Protocol::Hls => "HLS",
        }
    }
}

/// What went wrong, to say it to the user in their words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// not an address
    Invalid,
    /// known, not read yet
    NotYet(Protocol),
    /// nothing there: the address or the port
    Unreachable,
    /// the user or the password
    Refused,
    /// the server there, not the path
    NotFound,
    /// no answer in time
    Silent,
    /// H.265 (HEVC): not played; the camera can send H.264
    Hevc,
    /// no video, or a video that isn't H.264
    NoVideo(String),
    /// closed by the server before the video: the key, the user
    Closed,
    /// `rtmps://`: not read yet
    Encrypted,
    /// the port to receive on taken by another program
    PortTaken(u16),
    /// an app sent with another key: from where
    WrongKey(String),
    /// as it was said
    Other(String),
}

/// A source on the network as the user gave it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NetworkSource {
    pub url: String,
    pub username: String,
    pub password: String,
}

/// The address written made one to connect to: `rtsp://` when there's no
/// scheme, the user and the password in it taken out (given back apart).
pub fn parse(written: &str) -> Result<(url::Url, Protocol, Option<(String, String)>), Problem> {
    let written = written.trim();
    let with_scheme = match written.contains("://") {
        true => written.to_string(),
        false => format!("rtsp://{written}"),
    };
    let mut url = url::Url::parse(&with_scheme).map_err(|_| Problem::Invalid)?;
    if url.host_str().is_none_or(str::is_empty) {
        return Err(Problem::Invalid);
    }
    let protocol = match (url.scheme(), url.path().to_lowercase()) {
        ("rtsp" | "rtsps", _) => Protocol::Rtsp,
        ("rtmp" | "rtmps", _) => Protocol::Rtmp,
        ("http" | "https", path) if path.ends_with(".m3u8") => Protocol::Hls,
        _ => return Err(Problem::Invalid),
    };
    let user = match url.username() {
        "" => None,
        name => Some((decode(name), url.password().map(decode).unwrap_or_default())),
    };
    // taken out: they're given apart, the password not kept in the address
    let _ = url.set_username("");
    let _ = url.set_password(None);
    Ok((url, protocol, user))
}

fn decode(part: &str) -> String {
    url::form_urlencoded::parse(format!("x={part}").as_bytes())
        .next()
        .map(|(_, v)| v.into_owned())
        .unwrap_or_else(|| part.to_string())
}

/// The problem in an error of the client, as far as its words tell.
pub fn problem_of(error: &str) -> Problem {
    let e = error.to_lowercase();
    match () {
        () if e.contains("401") || e.contains("unauthorized") => Problem::Refused,
        () if e.contains("404") || e.contains("not found") => Problem::NotFound,
        () if e.contains("timed out") || e.contains("timeout") => Problem::Silent,
        () if e.contains("refused")
            || e.contains("unreachable")
            || e.contains("no route")
            || e.contains("lookup")
            || e.contains("resolve")
            || e.contains("unable to connect") =>
        {
            Problem::Unreachable
        }
        () => Problem::Other(error.to_string()),
    }
}

/// The parameters of H.264 in an `avcC` box (as RTSP gives them): the
/// sequence and picture parameter sets, as Annex B.
pub fn parameters_annex_b(avcc: &[u8]) -> Option<Vec<u8>> {
    let mut out = vec![];
    let sps_count = usize::from(*avcc.get(5)? & 0x1f);
    let mut at = 6;
    let take = |at: &mut usize, out: &mut Vec<u8>| -> Option<()> {
        let len = usize::from(u16::from_be_bytes([*avcc.get(*at)?, *avcc.get(*at + 1)?]));
        let nal = avcc.get(*at + 2..*at + 2 + len)?;
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(nal);
        *at += 2 + len;
        Some(())
    };
    for _ in 0..sps_count {
        take(&mut at, &mut out)?;
    }
    let pps_count = usize::from(*avcc.get(at)?);
    at += 1;
    for _ in 0..pps_count {
        take(&mut at, &mut out)?;
    }
    Some(out)
}

/// How many bytes say the length of each NAL, as an `avcC` box says.
pub fn length_size(avcc: &[u8]) -> Option<usize> {
    avcc.get(4).map(|b| usize::from(b & 3) + 1)
}

/// A frame as RTSP and RTMP give it (each NAL after its length, in
/// `size` bytes: four, mostly) as Annex B, its parameters before it when
/// it starts a picture anew.
pub fn frame_annex_b(data: &[u8], size: usize, parameters: Option<&[u8]>) -> Option<Vec<u8>> {
    let mut out = parameters.map(<[u8]>::to_vec).unwrap_or_default();
    let mut at = 0;
    while at < data.len() {
        let len = data
            .get(at..at + size)?
            .iter()
            .fold(0usize, |len, b| (len << 8) | usize::from(*b));
        let nal = data.get(at + size..at + size + len)?;
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(nal);
        at += size + len;
    }
    Some(out)
}

/// The preview of a frame decoded.
fn shown_of(yuv: &openh264::decoder::DecodedYUV) -> Option<(I420, (usize, usize, Vec<u8>))> {
    let (w, h) = yuv.dimensions();
    let (sy, su, sv) = yuv.strides();
    let (w, h) = (w & !1, h & !1);
    let mut data = Vec::with_capacity(w * h * 3 / 2);
    for row in 0..h {
        data.extend_from_slice(yuv.y().get(row * sy..row * sy + w)?);
    }
    for (plane, stride) in [(yuv.u(), su), (yuv.v(), sv)] {
        for row in 0..h / 2 {
            data.extend_from_slice(plane.get(row * stride..row * stride + w / 2)?);
        }
    }
    let picture = I420 {
        width: w,
        height: h,
        data,
    };
    let shown = picture::preview(&picture, PREVIEW_WIDTH);
    Some((picture, shown))
}

/// The frames of a source on the network as they come: recorded on the
/// clock of the recording, decoded for the preview while it's shown. One
/// for the source, over its connections: the preview counted on.
pub(super) struct Feed {
    decoder: Decoder,
    /// where the stream was (ms on its clock) and the recording, at the
    /// first frame recorded since connected
    first: Option<(u64, u64)>,
    serial: u64,
    decoding: bool,
    /// the bytes come since `counting`, for the bitrate of the source
    counted: u64,
    counting: Option<Instant>,
}

impl Feed {
    pub(super) fn new() -> Result<Feed, Problem> {
        Ok(Feed {
            decoder: Decoder::new().map_err(|e| Problem::Other(e.to_string()))?,
            first: None,
            serial: 0,
            decoding: false,
            counted: 0,
            counting: None,
        })
    }

    /// Connected (again): a clock of its own, a picture that starts anew.
    pub(super) fn connected(&mut self, status: &Mutex<Status>) {
        self.first = None;
        self.decoding = false;
        self.counted = 0;
        self.counting = None;
        set(status, |s| {
            s.problem = None;
            s.error = None;
        });
    }

    /// A frame (Annex B), `at_ms` on the clock of the stream, shown
    /// `shown_after_ms` later (B-frames); `starts` a picture anew (a
    /// keyframe).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn frame(
        &mut self,
        annex_b: &[u8],
        starts: bool,
        at_ms: u64,
        shown_after_ms: i32,
        take: &mut Option<Take>,
        preview: bool,
        status: &Mutex<Status>,
    ) {
        self.count(annex_b.len(), status);
        let stops = match take.as_mut() {
            Some(t) => {
                let (at0, ms0) = *self.first.get_or_insert((at_ms, t.elapsed_ms()));
                let written =
                    t.write_shown(ms0 + at_ms.saturating_sub(at0), shown_after_ms, annex_b);
                t.stops(written, status)
            }
            None => {
                self.first = None;
                false
            }
        };
        if stops {
            finish(take.take(), status);
        }
        // decoded only for the preview: not shown, not decoded, from the
        // next picture that starts anew when shown again
        let shown = match (preview, starts, self.decoding) {
            (false, _, _) => {
                self.decoding = false;
                None
            }
            (true, false, false) => None,
            (true, _, _) => {
                self.decoding = true;
                let decoded = self.decoder.decode(annex_b).ok().flatten();
                decoded.as_ref().and_then(shown_of)
            }
        };
        let recording = take.as_ref().map(Take::recording);
        match shown {
            Some((picture, (w, h, rgba))) => {
                self.serial += 1;
                let frame = Arc::new(Frame {
                    serial: self.serial,
                    width: u32::try_from(w).unwrap_or(0),
                    height: u32::try_from(h).unwrap_or(0),
                    rgba: Bytes::from(rgba),
                });
                let size = (
                    u32::try_from(picture.width).unwrap_or(0),
                    u32::try_from(picture.height).unwrap_or(0),
                );
                set(status, |s| {
                    s.frame = Some(frame);
                    s.size = Some(size);
                    s.recording = recording;
                });
            }
            None => set(status, |s| s.recording = recording),
        }
    }
}

impl Feed {
    /// `bytes` more come: the bitrate of the source said once it's been
    /// counted long enough, for the space a match needs.
    fn count(&mut self, bytes: usize, status: &Mutex<Status>) {
        let since = *self.counting.get_or_insert_with(Instant::now);
        self.counted = self
            .counted
            .saturating_add(u64::try_from(bytes).unwrap_or(u64::MAX));
        let ms = u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX);
        if ms >= COUNTED_FOR_MS {
            let bits = self.counted.saturating_mul(8_000) / ms;
            set(status, |s| s.bits_per_second = Some(bits));
        }
    }
}

/// How long the bytes of a source are counted before its bitrate is said:
/// a few keyframes in.
const COUNTED_FOR_MS: u64 = 3_000;

/// What the loop is asked between frames.
pub(super) enum Asked {
    Nothing,
    Quit,
}

pub(super) fn look(
    commands: &Receiver<Command>,
    take: &mut Option<Take>,
    preview: &mut bool,
    status: &Mutex<Status>,
) -> Asked {
    loop {
        match commands.try_recv() {
            Ok(Command::Record { base, started, .. }) => {
                finish(take.take(), status);
                match Take::start(base, started) {
                    Ok(t) => *take = Some(t),
                    Err(e) => set(status, |s| s.error = Some(e)),
                }
            }
            Ok(Command::Stop) => finish(take.take(), status),
            Ok(Command::Preview(on)) => *preview = on,
            Ok(Command::Quit) | Err(TryRecvError::Disconnected) => {
                finish(take.take(), status);
                return Asked::Quit;
            }
            Err(TryRecvError::Empty) => return Asked::Nothing,
        }
    }
}

/// How a source on the network is read: connected to, or waited for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reading {
    /// at an address
    Pull(NetworkSource),
    /// an app sending it to this computer (RTMP), on `port` with `key`
    Receive(super::rtmp::Receiver),
}

pub(super) fn run(reading: Reading, commands: Receiver<Command>, status: &Mutex<Status>) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let runtime = match runtime {
        Ok(r) => r,
        Err(e) => return set(status, |s| s.error = Some(e.to_string())),
    };
    runtime.block_on(async {
        let mut take: Option<Take> = None;
        let mut preview = true;
        let mut feed = match Feed::new() {
            Ok(f) => f,
            Err(problem) => return set(status, |s| s.problem = Some(problem)),
        };
        let mut io = Io {
            commands: &commands,
            take: &mut take,
            preview: &mut preview,
            status,
            feed: &mut feed,
        };
        loop {
            let ended = match &reading {
                Reading::Pull(source) => match parse(&source.url) {
                    Ok((url, Protocol::Rtsp, in_url)) => stream(url, source, in_url, &mut io).await,
                    Ok((url, Protocol::Rtmp, _)) => super::rtmp::pull(url, &mut io).await,
                    Ok((_, protocol, _)) => Err(Problem::NotYet(protocol)),
                    Err(problem) => Err(problem),
                },
                Reading::Receive(receiver) => super::rtmp::receive(receiver, &mut io).await,
            };
            match ended {
                Ok(Asked::Quit) => return,
                Ok(Asked::Nothing) => {}
                Err(problem) => set(status, |s| s.problem = Some(problem)),
            }
            // cut off, or never there: again in a while, the recording
            // going on (the frames missed, missing)
            let mut waited = Duration::ZERO;
            while waited < AGAIN_AFTER {
                if let Asked::Quit = io.look() {
                    return;
                }
                tokio::time::sleep(LOOK_EVERY).await;
                waited += LOOK_EVERY;
            }
        }
    });
}

/// What a connection reads into: the commands of the app, the recording,
/// the preview.
pub(super) struct Io<'a> {
    commands: &'a Receiver<Command>,
    pub(super) take: &'a mut Option<Take>,
    pub(super) preview: &'a mut bool,
    pub(super) status: &'a Mutex<Status>,
    pub(super) feed: &'a mut Feed,
}

impl Io<'_> {
    pub(super) fn look(&mut self) -> Asked {
        look(self.commands, self.take, self.preview, self.status)
    }

    pub(super) fn connected(&mut self) {
        self.feed.connected(self.status);
    }

    pub(super) fn frame(&mut self, annex_b: &[u8], starts: bool, at_ms: u64, shown_after_ms: i32) {
        self.feed.frame(
            annex_b,
            starts,
            at_ms,
            shown_after_ms,
            self.take,
            *self.preview,
            self.status,
        );
    }
}

/// Connected over RTSP and read until cut off (`Ok(Nothing)`) or asked to
/// quit.
async fn stream(
    mut url: url::Url,
    source: &NetworkSource,
    in_url: Option<(String, String)>,
    io: &mut Io<'_>,
) -> Result<Asked, Problem> {
    if url.port().is_none() {
        let _ = url.set_port(Some(RTSP_PORT));
    }
    let (username, password) = match (&source.username, in_url) {
        (name, _) if !name.is_empty() => (name.clone(), source.password.clone()),
        (_, Some(user)) => user,
        _ => (String::new(), String::new()),
    };
    let credentials = (!username.is_empty()).then_some(Credentials { username, password });
    let options = SessionOptions::default()
        .creds(credentials)
        .user_agent("scout4all".to_string());
    let mut session = tokio::time::timeout(ANSWER_WITHIN, Session::describe(url, options))
        .await
        .map_err(|_| Problem::Silent)?
        .map_err(|e| problem_of(&e.to_string()))?;
    let video = session
        .streams()
        .iter()
        .position(|s| s.media() == "video")
        .ok_or_else(|| Problem::NoVideo(String::new()))?;
    let encoding = session
        .streams()
        .get(video)
        .map(|s| s.encoding_name().to_lowercase())
        .unwrap_or_default();
    match encoding.as_str() {
        "h264" => {}
        "h265" | "hevc" => return Err(Problem::Hevc),
        other => return Err(Problem::NoVideo(other.to_uppercase())),
    }
    let transport = Transport::Tcp(TcpTransportOptions::default());
    tokio::time::timeout(
        ANSWER_WITHIN,
        session.setup(video, SetupOptions::default().transport(transport)),
    )
    .await
    .map_err(|_| Problem::Silent)?
    .map_err(|e| problem_of(&e.to_string()))?;
    let playing = tokio::time::timeout(ANSWER_WITHIN, session.play(PlayOptions::default()))
        .await
        .map_err(|_| Problem::Silent)?
        .map_err(|e| problem_of(&e.to_string()))?;
    let mut demuxed = playing.demuxed().map_err(|e| problem_of(&e.to_string()))?;
    io.connected();
    let mut parameters: Option<Vec<u8>> = None;
    let mut silent = Duration::ZERO;
    loop {
        if let Asked::Quit = io.look() {
            return Ok(Asked::Quit);
        }
        let item = match tokio::time::timeout(LOOK_EVERY, demuxed.next()).await {
            Err(_) => {
                silent += LOOK_EVERY;
                match silent >= ANSWER_WITHIN {
                    true => return Err(Problem::Silent),
                    false => continue,
                }
            }
            Ok(None) => return Ok(Asked::Nothing),
            Ok(Some(Err(e))) => return Err(problem_of(&e.to_string())),
            Ok(Some(Ok(item))) => item,
        };
        silent = Duration::ZERO;
        let CodecItem::VideoFrame(frame) = item else {
            continue;
        };
        if frame.has_new_parameters() || parameters.is_none() {
            parameters = demuxed
                .streams()
                .get(video)
                .and_then(|s| match s.parameters() {
                    Some(ParametersRef::Video(v)) => parameters_annex_b(v.extra_data()),
                    _ => None,
                });
        }
        let starts = frame.is_random_access_point();
        let annex_b = frame_annex_b(frame.data(), 4, parameters.as_deref().filter(|_| starts));
        let Some(annex_b) = annex_b else {
            continue;
        };
        let clock = i64::from(frame.timestamp().clock_rate().get()).max(1);
        let ticks = frame.timestamp().elapsed().max(0);
        let at_ms = u64::try_from(ticks.saturating_mul(1000) / clock).unwrap_or(0);
        io.frame(&annex_b, starts, at_ms, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Against the test server (`scratchpad`-like: mediamtx in Docker with
    /// `scout`/`segreta`, ffmpeg sending H.264 on `/campo` and H.265 on
    /// `/campo265`; `S4A_RTSP` for another address). `cargo test -p
    /// scout4all-desktop -- --ignored rtsp`.
    #[test]
    #[ignore]
    fn rtsp_recorded_from_the_test_server() {
        use crate::live::capture::{Capture, Source};
        let base_url = std::env::var("S4A_RTSP").unwrap_or_else(|_| "127.0.0.1:8554".to_string());
        let source = |path: &str, password: &str| {
            Source::Network(NetworkSource {
                url: format!("{base_url}/{path}"),
                username: "scout".into(),
                password: password.into(),
            })
        };
        let wait = |capture: &Capture, until: &dyn Fn(&Status) -> bool| {
            for _ in 0..150 {
                if capture.status().is_some_and(|s| until(&s)) {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            false
        };
        let wrong = Capture::open(source("campo", "sbagliata"));
        assert!(wait(&wrong, &|s| s.problem == Some(Problem::Refused)));
        let hevc = Capture::open(source("campo265", "segreta"));
        assert!(wait(&hevc, &|s| s.problem == Some(Problem::Hevc)));
        let missing = Capture::open(source("nessuno", "segreta"));
        assert!(wait(&missing, &|s| s.problem == Some(Problem::NotFound)));
        let dir = tempfile::tempdir().unwrap();
        let capture = Capture::open(source("campo", "segreta"));
        assert!(wait(&capture, &|s| s.frame.is_some()));
        assert_eq!(capture.status().unwrap().size, Some((1280, 720)));
        let base = dir.path().join("20261106 120000");
        capture.record(base.clone(), chrono::Local::now(), Default::default());
        capture.preview(false);
        std::thread::sleep(Duration::from_secs(3));
        capture.stop();
        assert!(wait(&capture, &|s| !s.done.is_empty()));
        let done = capture.take_done();
        let made = done[0].result.clone().unwrap();
        // from the first keyframe (one every 2 s): the start moved as much
        assert!(made.skipped_ms <= 2100, "{made:?}");
        let recorded = made.length_ms + made.skipped_ms;
        assert!((2500..=3600).contains(&recorded), "{made:?}");
        let video = crate::video::source::Source::open(&done[0].mp4).unwrap();
        assert!(video.samples.len() as u64 >= made.length_ms * 30 / 1000 - 3);
    }

    /// Connected again: the preview counted on from where it was, for
    /// the screen (which shows only newer ones) to show it.
    #[test]
    fn the_preview_after_connecting_again() {
        use openh264::{OpenH264API, encoder::Encoder, formats::YUVBuffer};
        let mut encoder = Encoder::with_api_config(
            OpenH264API::from_source(),
            openh264::encoder::EncoderConfig::new(),
        )
        .unwrap();
        let yuv = YUVBuffer::from_vec(vec![90; 64 * 48 * 3 / 2], 64, 48);
        let keyframe = encoder.encode(&yuv).unwrap().to_vec();
        let status = Mutex::new(Status::default());
        let mut feed = Feed::new().unwrap();
        let serial = |status: &Mutex<Status>| status.lock().unwrap().frame.as_ref().unwrap().serial;
        feed.frame(&keyframe, true, 0, 0, &mut None, true, &status);
        let before = serial(&status);
        feed.connected(&status);
        feed.frame(&keyframe, true, 0, 0, &mut None, true, &status);
        assert!(serial(&status) > before);
    }

    /// Frames of an encoder, the first a keyframe with its parameters.
    fn encoded(count: usize) -> Vec<Vec<u8>> {
        use openh264::{OpenH264API, encoder::Encoder, formats::YUVBuffer};
        let mut encoder = Encoder::with_api_config(
            OpenH264API::from_source(),
            openh264::encoder::EncoderConfig::new(),
        )
        .unwrap();
        (0..count)
            .map(|i| {
                let shade = u8::try_from(i % 200).unwrap();
                let yuv = YUVBuffer::from_vec(vec![shade; 64 * 48 * 3 / 2], 64, 48);
                encoder.encode(&yuv).unwrap().to_vec()
            })
            .collect()
    }

    /// The disk almost full: stopped by itself after its frames on disk,
    /// its MP4 made and readable, said; with room, it goes on.
    #[test]
    fn stopped_as_the_disk_fills_up() {
        let dir = tempfile::tempdir().unwrap();
        let frames = encoded(100);
        let status = Mutex::new(Status::default());
        let mut feed = Feed::new().unwrap();
        let base = dir.path().join("20261110 120000");
        let mut take =
            Some(Take::start_on(base.clone(), chrono::Local::now(), |_| Some(0)).unwrap());
        for (i, f) in frames.iter().enumerate() {
            let at = u64::try_from(i).unwrap() * 33;
            feed.frame(f, i == 0, at, 0, &mut take, false, &status);
        }
        assert!(take.is_none());
        let done = std::mem::take(&mut status.lock().unwrap().done);
        let [done] = done.as_slice() else {
            panic!("{done:?}");
        };
        assert!(done.out_of_space);
        // at the first look at the space (every 30 frames)
        let made = done.result.clone().unwrap();
        assert!((900..1000).contains(&made.length_ms), "{made:?}");
        let video = crate::video::source::Source::open(&done.mp4).unwrap();
        assert_eq!(video.samples.len(), 30);
        // with room for its MP4 and the reserve: on
        let base = dir.path().join("20261110 130000");
        let room = |_: &std::path::Path| Some(crate::live::space::RESERVE + 10_000_000);
        let mut take = Some(Take::start_on(base, chrono::Local::now(), room).unwrap());
        for (i, f) in frames.iter().enumerate() {
            let at = u64::try_from(i).unwrap() * 33;
            feed.frame(f, i == 0, at, 0, &mut take, false, &status);
        }
        assert!(take.is_some_and(|t| !t.out_of_space));
    }

    /// The bitrate of a source said once counted for a while.
    #[test]
    fn the_bitrate_counted() {
        let status = Mutex::new(Status::default());
        let mut feed = Feed::new().unwrap();
        feed.count(1_000, &status);
        assert_eq!(status.lock().unwrap().bits_per_second, None);
        feed.counting = Instant::now().checked_sub(Duration::from_secs(4));
        feed.count(499_000, &status);
        // 500 kB in about 4 s: about 1 Mbit/s
        let bits = status.lock().unwrap().bits_per_second.unwrap();
        assert!((900_000..=1_000_000).contains(&bits), "{bits}");
        // connected again: counted anew
        feed.connected(&status);
        assert_eq!((feed.counted, feed.counting), (0, None));
    }

    #[test]
    fn the_addresses_written() {
        let (url, protocol, user) = parse("192.168.1.20/stream1").unwrap();
        assert_eq!(url.as_str(), "rtsp://192.168.1.20/stream1");
        assert_eq!((protocol, user), (Protocol::Rtsp, None));
        let (url, _, user) = parse(" rtsp://admin:p%40ss@cam.local:8554/live ").unwrap();
        assert_eq!(url.as_str(), "rtsp://cam.local:8554/live");
        assert_eq!(user, Some(("admin".into(), "p@ss".into())));
        assert_eq!(parse("rtmp://server/live/key").unwrap().1, Protocol::Rtmp);
        assert_eq!(
            parse("https://cdn.example/match/index.m3u8").unwrap().1,
            Protocol::Hls
        );
        for wrong in ["", "rtsp://", "ftp://x/y", "https://example.com/page"] {
            assert_eq!(parse(wrong).err(), Some(Problem::Invalid), "{wrong:?}");
        }
    }

    #[test]
    fn the_problems_in_the_errors() {
        let said = [
            (
                "401 response to DESCRIBE CSeq=2: Received Unauthorized after trying digest auth",
                Problem::Refused,
            ),
            (
                "404 response to DESCRIBE CSeq=2: Not Found",
                Problem::NotFound,
            ),
            (
                "Unable to connect to RTSP server: Connection refused (os error 61)",
                Problem::Unreachable,
            ),
            (
                "Unable to connect to RTSP server: failed to lookup address information",
                Problem::Unreachable,
            ),
            ("connection timed out", Problem::Silent),
        ];
        for (error, problem) in said {
            assert_eq!(problem_of(error), problem, "{error}");
        }
        assert_eq!(problem_of("odd"), Problem::Other("odd".into()));
    }

    #[test]
    fn the_frames_made_annex_b() {
        // avcC: version, profile, compatibility, level, length size, one
        // SPS of 3 bytes, one PPS of 2
        let avcc = [
            1, 0x64, 0, 0x1f, 0xff, 0xe1, 0, 3, 0x67, 1, 2, 1, 0, 2, 0x68, 9,
        ];
        let parameters = parameters_annex_b(&avcc).unwrap();
        assert_eq!(parameters, [0, 0, 0, 1, 0x67, 1, 2, 0, 0, 0, 1, 0x68, 9]);
        let data = [0, 0, 0, 2, 0x65, 7, 0, 0, 0, 1, 0x06];
        let frame = frame_annex_b(&data, 4, Some(&parameters)).unwrap();
        assert_eq!(frame[..13], parameters[..]);
        assert_eq!(frame[13..], [0, 0, 0, 1, 0x65, 7, 0, 0, 0, 1, 0x06]);
        assert_eq!(frame_annex_b(&data, 4, None).unwrap(), frame[13..]);
        // cut short: not a frame
        assert_eq!(frame_annex_b(&[0, 0, 0, 9, 1], 4, None), None);
        assert_eq!(parameters_annex_b(&[1, 2]), None);
    }
}
