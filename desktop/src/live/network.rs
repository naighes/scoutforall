//! A source of video on the network: RTSP (an IP camera, a phone app, a
//! streamer), with its user and password. The H.264 that comes is recorded
//! as it is (no encoding again) and decoded only for the preview. Cut off
//! while recording, it connects again and goes on in the same recording.
//! RTMP and HLS are known, not read yet.

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
        time::Duration,
    },
};

/// How wide the preview is.
const PREVIEW_WIDTH: usize = 960;

/// How long to wait for the source to answer.
const ANSWER_WITHIN: Duration = Duration::from_secs(10);

/// How long to wait before connecting again, once cut off.
const AGAIN_AFTER: Duration = Duration::from_secs(2);

/// How often the commands are looked at while no frame comes.
const LOOK_EVERY: Duration = Duration::from_millis(100);

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

/// A frame as RTSP gives it (each NAL after its length, four bytes) as
/// Annex B, its parameters before it when it starts a picture anew.
pub fn frame_annex_b(data: &[u8], parameters: Option<&[u8]>) -> Option<Vec<u8>> {
    let mut out = parameters.map(<[u8]>::to_vec).unwrap_or_default();
    let mut at = 0;
    while at < data.len() {
        let len = u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?);
        let len = usize::try_from(len).ok()?;
        let nal = data.get(at + 4..at + 4 + len)?;
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(nal);
        at += 4 + len;
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

/// What the loop is asked between frames.
enum Asked {
    Nothing,
    Quit,
}

fn look(
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

pub(super) fn run(source: NetworkSource, commands: Receiver<Command>, status: &Mutex<Status>) {
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
        loop {
            let ended = stream(&source, &commands, &mut take, &mut preview, status).await;
            match ended {
                Ok(Asked::Quit) => return,
                Ok(Asked::Nothing) => {}
                Err(problem) => set(status, |s| s.problem = Some(problem)),
            }
            // cut off, or never there: again in a while, the recording
            // going on (the frames missed, missing)
            let mut waited = Duration::ZERO;
            while waited < AGAIN_AFTER {
                if let Asked::Quit = look(&commands, &mut take, &mut preview, status) {
                    return;
                }
                tokio::time::sleep(LOOK_EVERY).await;
                waited += LOOK_EVERY;
            }
        }
    });
}

/// Connected and read until cut off (`Ok(Nothing)`) or asked to quit.
async fn stream(
    source: &NetworkSource,
    commands: &Receiver<Command>,
    take: &mut Option<Take>,
    preview: &mut bool,
    status: &Mutex<Status>,
) -> Result<Asked, Problem> {
    let (url, protocol, in_url) = parse(&source.url)?;
    if protocol != Protocol::Rtsp {
        return Err(Problem::NotYet(protocol));
    }
    let mut url = url;
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
    let mut decoder = Decoder::new().map_err(|e| Problem::Other(e.to_string()))?;
    set(status, |s| {
        s.problem = None;
        s.error = None;
    });
    // the frames on the clock of the recording: from where it is when
    // connected, then as the stream counts them
    let mut first: Option<(i64, u64)> = None;
    let mut serial = 0u64;
    let mut parameters: Option<Vec<u8>> = None;
    let mut silent = Duration::ZERO;
    let mut decoding = false;
    loop {
        if let Asked::Quit = look(commands, take, preview, status) {
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
        let Some(annex_b) = frame_annex_b(frame.data(), parameters.as_deref().filter(|_| starts))
        else {
            continue;
        };
        let clock = i64::from(frame.timestamp().clock_rate().get());
        let ticks = frame.timestamp().elapsed();
        if let Some(t) = take.as_mut() {
            let (ticks0, ms0) = *first.get_or_insert((ticks, t.elapsed_ms()));
            let since = u64::try_from((ticks - ticks0).max(0).saturating_mul(1000) / clock).unwrap_or(0);
            if let Err(e) = t.write(ms0 + since, &annex_b) {
                set(status, |s| s.error = Some(e));
                finish(take.take(), status);
            }
        } else {
            first = None;
        }
        // decoded only for the preview: not shown, not decoded, from the
        // next picture that starts anew when shown again
        let shown = match (*preview, starts, decoding) {
            (false, _, _) => {
                decoding = false;
                None
            }
            (true, false, false) => None,
            (true, _, _) => {
                decoding = true;
                let decoded = decoder.decode(&annex_b).ok().flatten();
                decoded.as_ref().and_then(shown_of)
            }
        };
        let recording = take.as_ref().map(Take::recording);
        match shown {
            Some((picture, (w, h, rgba))) => {
                serial += 1;
                let frame = Arc::new(Frame {
                    serial,
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
        let frame = frame_annex_b(&data, Some(&parameters)).unwrap();
        assert_eq!(frame[..13], parameters[..]);
        assert_eq!(frame[13..], [0, 0, 0, 1, 0x65, 7, 0, 0, 0, 1, 0x06]);
        assert_eq!(frame_annex_b(&data, None).unwrap(), frame[13..]);
        // cut short: not a frame
        assert_eq!(frame_annex_b(&[0, 0, 0, 9, 1], None), None);
        assert_eq!(parameters_annex_b(&[1, 2]), None);
    }
}
