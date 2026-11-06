//! RTMP: a stream read from a server (`rtmp://server/app/key`), or sent to
//! this computer by an app (Larix, OBS, a camera that streams): then the
//! computer is the server, the app given its address and a key. The H.264
//! in it (FLV video tags) recorded as it is, as RTSP's; the audio left.

use {
    super::network::{
        ANSWER_WITHIN, Asked, Io, LOOK_EVERY, Problem, frame_annex_b, length_size,
        parameters_annex_b, problem_of,
    },
    rml_rtmp::{
        handshake::{Handshake, HandshakeProcessResult, PeerType},
        sessions::{
            ClientSession, ClientSessionConfig, ClientSessionEvent, ClientSessionResult,
            ServerSession, ServerSessionConfig, ServerSessionEvent, ServerSessionResult,
        },
    },
    std::{
        net::{IpAddr, Ipv4Addr, UdpSocket},
        time::Duration,
    },
    tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    },
};

/// The port of RTMP, when not written.
pub const RTMP_PORT: u16 = 1935;

/// The application the apps send to: `rtmp://computer:port/live/key`.
pub const APP: &str = "live";

/// This computer waiting for an app to send it the video, on `port`,
/// taking it only with `key`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receiver {
    pub port: u16,
    pub key: String,
}

impl Receiver {
    /// The address the app is given: the server, then the key.
    pub fn server(&self, host: &str) -> String {
        format!("rtmp://{host}:{}/{APP}", self.port)
    }

    pub fn address(&self, host: &str) -> String {
        format!("{}/{}", self.server(host), self.key)
    }
}

/// A key not guessed: eight letters and digits.
pub fn new_key() -> String {
    uuid::Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(8)
        .collect()
}

/// The address of this computer on its network (the one the way out
/// goes from: nothing is sent), for the app to send to.
pub fn this_computer() -> Option<IpAddr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    socket
        .local_addr()
        .ok()
        .map(|a| a.ip())
        .filter(|ip| !ip.is_unspecified())
}

/// What a video tag of FLV says.
#[derive(Debug, PartialEq, Eq)]
pub enum Tag<'a> {
    /// the parameters of the H.264 (`avcC`)
    Parameters(&'a [u8]),
    /// a frame, each NAL after its length; `key` starts a picture anew;
    /// shown `composition_ms` after it's decoded (B-frames)
    Frame {
        data: &'a [u8],
        key: bool,
        composition_ms: i32,
    },
    /// nothing to show: the end of the stream, a command
    Nothing,
}

/// The time after the decoding a frame is shown: 24 bits, signed.
fn composition(bytes: Option<&[u8]>) -> i32 {
    match bytes {
        Some([a, b, c]) => {
            let v = (i32::from(*a) << 16) | (i32::from(*b) << 8) | i32::from(*c);
            match v & 0x80_0000 {
                0 => v,
                _ => v - 0x100_0000,
            }
        }
        _ => 0,
    }
}

/// The video tag of FLV `data` (as RTMP gives it): H.264 only, the old
/// way or the enhanced one (a FourCC); H.265 and the others said.
pub fn video_tag(data: &[u8]) -> Result<Tag<'_>, Problem> {
    let Some(&first) = data.first() else {
        return Ok(Tag::Nothing);
    };
    let frame_type = (first >> 4) & 0x07;
    let key = frame_type == 1;
    let from = |at: usize| data.get(at..).unwrap_or_default();
    match (first & 0x80 != 0, frame_type) {
        // a command, not a picture
        (_, 5) => Ok(Tag::Nothing),
        // enhanced RTMP: the codec by its FourCC, the kind of packet below
        (true, _) => match data.get(1..5) {
            Some(b"avc1") => match first & 0x0f {
                0 => Ok(Tag::Parameters(from(5))),
                1 => Ok(Tag::Frame {
                    data: from(8),
                    key,
                    composition_ms: composition(data.get(5..8)),
                }),
                3 => Ok(Tag::Frame {
                    data: from(5),
                    key,
                    composition_ms: 0,
                }),
                _ => Ok(Tag::Nothing),
            },
            Some(b"hvc1") => Err(Problem::Hevc),
            Some(other) => Err(Problem::NoVideo(
                String::from_utf8_lossy(other).to_uppercase(),
            )),
            None => Ok(Tag::Nothing),
        },
        (false, _) => match (first & 0x0f, data.get(1)) {
            (7, Some(0)) => Ok(Tag::Parameters(from(5))),
            (7, Some(1)) => Ok(Tag::Frame {
                data: from(5),
                key,
                composition_ms: composition(data.get(2..5)),
            }),
            (7, _) => Ok(Tag::Nothing),
            (12, _) => Err(Problem::Hevc),
            (2, _) => Err(Problem::NoVideo("H.263".into())),
            (4 | 5, _) => Err(Problem::NoVideo("VP6".into())),
            (codec, _) => Err(Problem::NoVideo(format!("FLV {codec}"))),
        },
    }
}

/// The H.264 of a stream as its tags come.
#[derive(Default)]
struct Video {
    /// as Annex B, and the length of the length of each NAL
    parameters: Option<(Vec<u8>, usize)>,
}

impl Video {
    fn tag(&mut self, data: &[u8], timestamp_ms: u32, io: &mut Io<'_>) -> Result<(), Problem> {
        match video_tag(data)? {
            Tag::Parameters(avcc) => {
                self.parameters = parameters_annex_b(avcc).zip(length_size(avcc));
            }
            // without its parameters, a frame can't be made a picture
            Tag::Frame {
                data,
                key,
                composition_ms,
            } => {
                if let Some((parameters, size)) = &self.parameters {
                    let annex_b = frame_annex_b(data, *size, key.then_some(parameters.as_slice()));
                    if let Some(annex_b) = annex_b {
                        io.frame(&annex_b, key, u64::from(timestamp_ms), composition_ms);
                    }
                }
            }
            Tag::Nothing => {}
        }
        Ok(())
    }
}

/// The problem in what a server said, refusing.
fn refused(said: &str) -> Problem {
    let s = said.to_lowercase();
    match () {
        () if s.contains("notfound") || s.contains("not found") => Problem::NotFound,
        () if s.contains("auth")
            || s.contains("denied")
            || s.contains("reject")
            || s.contains("badname")
            || s.contains("forbidden")
            || s.contains("failed") =>
        {
            Problem::Refused
        }
        () => Problem::Other(said.to_string()),
    }
}

async fn send(stream: &mut TcpStream, bytes: &[u8]) -> Result<(), Problem> {
    stream
        .write_all(bytes)
        .await
        .map_err(|e| problem_of(&e.to_string()))
}

/// The handshake of RTMP, as the client or the server: what came after it.
async fn handshake(stream: &mut TcpStream, client: bool) -> Result<Vec<u8>, Problem> {
    let mut handshake = Handshake::new(match client {
        true => PeerType::Client,
        false => PeerType::Server,
    });
    let other = |e: rml_rtmp::handshake::HandshakeError| Problem::Other(e.to_string());
    if client {
        let first = handshake.generate_outbound_p0_and_p1().map_err(other)?;
        send(stream, &first).await?;
    }
    let mut buffer = vec![0; 4096];
    loop {
        let n = tokio::time::timeout(ANSWER_WITHIN, stream.read(&mut buffer))
            .await
            .map_err(|_| Problem::Silent)?
            .map_err(|e| problem_of(&e.to_string()))?;
        if n == 0 {
            return Err(Problem::Closed);
        }
        let read = buffer.get(..n).unwrap_or_default();
        match handshake.process_bytes(read).map_err(other)? {
            HandshakeProcessResult::InProgress { response_bytes } => {
                send(stream, &response_bytes).await?;
            }
            HandshakeProcessResult::Completed {
                response_bytes,
                remaining_bytes,
            } => {
                send(stream, &response_bytes).await?;
                return Ok(remaining_bytes);
            }
        }
    }
}

/// The application and the key of a stream in its address: the key the
/// last part of the path, its query (a user, a password) with it.
pub fn app_and_key(url: &url::Url) -> (String, String) {
    let path = url.path().trim_start_matches('/');
    let query = url.query().map(|q| format!("?{q}")).unwrap_or_default();
    match path.rsplit_once('/') {
        Some((app, key)) => (app.to_string(), format!("{key}{query}")),
        None => (format!("{path}{query}"), String::new()),
    }
}

/// Read from a server until cut off (`Ok(Nothing)`) or asked to quit.
pub(super) async fn pull(url: url::Url, io: &mut Io<'_>) -> Result<Asked, Problem> {
    if url.scheme() == "rtmps" {
        return Err(Problem::Encrypted);
    }
    let host = url.host_str().unwrap_or_default().to_string();
    let port = url.port().unwrap_or(RTMP_PORT);
    let (app, key) = app_and_key(&url);
    let mut stream = tokio::time::timeout(ANSWER_WITHIN, TcpStream::connect((host.as_str(), port)))
        .await
        .map_err(|_| Problem::Silent)?
        .map_err(|e| problem_of(&format!("unable to connect: {e}")))?;
    let _ = stream.set_nodelay(true);
    let remaining = handshake(&mut stream, true).await?;
    let other = |e: rml_rtmp::sessions::ClientSessionError| Problem::Other(e.to_string());
    let mut config = ClientSessionConfig::new();
    config.tc_url = Some(format!("rtmp://{host}:{port}/{app}"));
    let (mut session, first) = ClientSession::new(config).map_err(other)?;
    let mut results = first;
    results.extend(session.handle_input(&remaining).map_err(other)?);
    results.push(session.request_connection(app).map_err(other)?);
    let mut video = Video::default();
    let mut playing = false;
    let mut silent = Duration::ZERO;
    let mut buffer = vec![0; 64 * 1024];
    loop {
        // what to send, what was said: one after the other, in order
        let mut said = vec![];
        for result in results.drain(..) {
            match result {
                ClientSessionResult::OutboundResponse(packet) => {
                    send(&mut stream, &packet.bytes).await?;
                }
                ClientSessionResult::RaisedEvent(event) => said.push(event),
                ClientSessionResult::UnhandleableMessageReceived(_) => {}
            }
        }
        for event in said {
            match event {
                ClientSessionEvent::ConnectionRequestAccepted => {
                    results.push(session.request_playback(key.clone()).map_err(other)?);
                }
                ClientSessionEvent::ConnectionRequestRejected { description } => {
                    return Err(refused(&description));
                }
                // taken once the video comes: some servers say yes, then
                // close for the user
                ClientSessionEvent::VideoDataReceived { timestamp, data } => {
                    if !playing {
                        playing = true;
                        io.connected();
                    }
                    video.tag(&data, timestamp.value, io)?;
                }
                ClientSessionEvent::UnhandleableOnStatusCode { code } => match code.as_str() {
                    c if c.contains("Stop")
                        || c.contains("Unpublish")
                        || c.contains("Complete") =>
                    {
                        return Ok(Asked::Nothing);
                    }
                    c if c.contains("NotFound")
                        || c.contains("Failed")
                        || c.contains("Rejected")
                        || c.contains("BadName") =>
                    {
                        return Err(refused(c));
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        if !results.is_empty() {
            continue;
        }
        if let Asked::Quit = io.look() {
            return Ok(Asked::Quit);
        }
        match tokio::time::timeout(LOOK_EVERY, stream.read(&mut buffer)).await {
            Err(_) => {
                silent += LOOK_EVERY;
                if silent >= ANSWER_WITHIN {
                    return Err(Problem::Silent);
                }
            }
            // closed by the server before the video: a key, a user not
            // taken, mostly
            Ok(Ok(0)) if !playing => return Err(Problem::Closed),
            Ok(Ok(0)) => return Ok(Asked::Nothing),
            Ok(Ok(n)) => {
                silent = Duration::ZERO;
                let read = buffer.get(..n).unwrap_or_default();
                results = session.handle_input(read).map_err(other)?;
            }
            Ok(Err(e)) if !playing => return Err(problem_of(&e.to_string())),
            Ok(Err(_)) => return Ok(Asked::Nothing),
        }
    }
}

/// Waiting for the app, taking its video while it sends it, then waiting
/// again: until asked to quit, or the port is taken.
pub(super) async fn receive(receiver: &Receiver, io: &mut Io<'_>) -> Result<Asked, Problem> {
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, receiver.port))
        .await
        .map_err(|_| Problem::PortTaken(receiver.port))?;
    super::capture::set(io.status, |s| {
        s.problem = None;
        s.sender = None;
    });
    loop {
        if let Asked::Quit = io.look() {
            return Ok(Asked::Quit);
        }
        let Ok(accepted) = tokio::time::timeout(LOOK_EVERY, listener.accept()).await else {
            continue;
        };
        let Ok((stream, from)) = accepted else {
            continue;
        };
        let from = from.ip().to_string();
        let ended = publisher(stream, &from, receiver, io).await;
        super::capture::set(io.status, |s| s.sender = None);
        match ended {
            Ok(Asked::Quit) => return Ok(Asked::Quit),
            Ok(Asked::Nothing) => {}
            Err(problem) => super::capture::set(io.status, |s| s.problem = Some(problem)),
        }
    }
}

/// An app sending to this computer, until it stops.
async fn publisher(
    mut stream: TcpStream,
    from: &str,
    receiver: &Receiver,
    io: &mut Io<'_>,
) -> Result<Asked, Problem> {
    let _ = stream.set_nodelay(true);
    let remaining = handshake(&mut stream, false).await?;
    let other = |e: rml_rtmp::sessions::ServerSessionError| Problem::Other(e.to_string());
    let (mut session, first) = ServerSession::new(ServerSessionConfig::new()).map_err(other)?;
    let mut results = first;
    results.extend(session.handle_input(&remaining).map_err(other)?);
    let mut video = Video::default();
    let mut silent = Duration::ZERO;
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let mut said = vec![];
        for result in results.drain(..) {
            match result {
                ServerSessionResult::OutboundResponse(packet) => {
                    send(&mut stream, &packet.bytes).await?;
                }
                ServerSessionResult::RaisedEvent(event) => said.push(event),
                ServerSessionResult::UnhandleableMessageReceived(_) => {}
            }
        }
        for event in said {
            match event {
                ServerSessionEvent::ConnectionRequested { request_id, .. } => {
                    results.extend(session.accept_request(request_id).map_err(other)?);
                }
                ServerSessionEvent::PublishStreamRequested {
                    request_id,
                    app_name,
                    stream_key,
                    ..
                } => {
                    // the key after the app, or in it (`live/key`); the
                    // query of some apps left
                    let given = format!("{app_name}/{stream_key}");
                    let given = given.split('?').next().unwrap_or_default();
                    match given
                        .trim_end_matches('/')
                        .ends_with(&format!("/{}", receiver.key))
                    {
                        true => {
                            results.extend(session.accept_request(request_id).map_err(other)?);
                            io.connected();
                            super::capture::set(io.status, |s| s.sender = Some(from.to_string()));
                        }
                        false => {
                            let no = session
                                .reject_request(
                                    request_id,
                                    "NetStream.Publish.BadName",
                                    "wrong stream key",
                                )
                                .map_err(other)?;
                            for result in no {
                                if let ServerSessionResult::OutboundResponse(packet) = result {
                                    let _ = send(&mut stream, &packet.bytes).await;
                                }
                            }
                            return Err(Problem::WrongKey(from.to_string()));
                        }
                    }
                }
                ServerSessionEvent::VideoDataReceived {
                    data, timestamp, ..
                } => video.tag(&data, timestamp.value, io)?,
                ServerSessionEvent::PublishStreamFinished { .. } => return Ok(Asked::Nothing),
                _ => {}
            }
        }
        if !results.is_empty() {
            continue;
        }
        if let Asked::Quit = io.look() {
            return Ok(Asked::Quit);
        }
        match tokio::time::timeout(LOOK_EVERY, stream.read(&mut buffer)).await {
            Err(_) => {
                silent += LOOK_EVERY;
                if silent >= ANSWER_WITHIN {
                    return Ok(Asked::Nothing);
                }
            }
            Ok(Ok(0) | Err(_)) => return Ok(Asked::Nothing),
            Ok(Ok(n)) => {
                silent = Duration::ZERO;
                let read = buffer.get(..n).unwrap_or_default();
                results = session.handle_input(read).map_err(other)?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::live::capture::{Capture, Source, Status},
    };

    fn wait(capture: &Capture, until: &dyn Fn(&Status) -> bool) -> bool {
        for _ in 0..150 {
            if capture.status().is_some_and(|s| until(&s)) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    }

    /// Three seconds recorded: about as long, from the first keyframe.
    fn recorded(capture: &Capture) -> std::path::PathBuf {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("20261106 120000");
        capture.record(base, chrono::Local::now(), Default::default());
        capture.preview(false);
        std::thread::sleep(Duration::from_secs(3));
        capture.stop();
        assert!(wait(capture, &|s| !s.done.is_empty()));
        let done = capture.take_done().remove(0);
        let made = done.result.clone().unwrap();
        let length = made.length_ms + made.skipped_ms;
        assert!((2500..=3600).contains(&length), "{made:?}");
        let video = crate::video::source::Source::open(&done.mp4).unwrap();
        assert!(video.samples.len() as u64 >= made.length_ms * 25 / 1000 - 3);
        // kept, for a look (ffprobe)
        let kept = std::env::temp_dir().join("s4a-rtmp-test.mp4");
        std::fs::copy(&done.mp4, &kept).unwrap();
        kept
    }

    /// Read from the test server (mediamtx in Docker, RTMP on 19350,
    /// `scout`/`segreta` in the query; `S4A_RTMP` for another address).
    /// `cargo test -p scout4all-desktop -- --ignored rtmp`.
    #[test]
    #[ignore]
    fn rtmp_read_from_the_test_server() {
        let base = std::env::var("S4A_RTMP").unwrap_or_else(|_| "rtmp://127.0.0.1:19350".into());
        let source = |path: &str| {
            Capture::open(Source::Network(crate::live::network::NetworkSource {
                url: format!("{base}/{path}"),
                ..Default::default()
            }))
        };
        let closed = source("campo");
        assert!(wait(&closed, &|s| s.problem == Some(Problem::Closed)));
        let capture = source("campo?user=scout&pass=segreta");
        assert!(wait(&capture, &|s| s.frame.is_some()));
        assert_eq!(capture.status().unwrap().size, Some((1280, 720)));
        recorded(&capture);
    }

    /// An app sending to this computer (ffmpeg as it): taken with the
    /// key, said with another; B-frames as x264 makes them by default
    /// (`S4A_BFRAMES` how many).
    #[test]
    #[ignore]
    fn rtmp_received_from_an_app() {
        let receiver = Receiver {
            port: 19351,
            key: new_key(),
        };
        let bframes = std::env::var("S4A_BFRAMES").unwrap_or_else(|_| "3".into());
        let send_as = |key: &str, codec: &str| {
            std::process::Command::new("ffmpeg")
                .args(["-nostdin", "-loglevel", "error", "-re", "-f", "lavfi", "-i"])
                .arg("testsrc2=size=1280x720:rate=30")
                .args(["-c:v", codec, "-preset", "veryfast", "-g", "60"])
                .args([
                    "-bf", &bframes, "-pix_fmt", "yuv420p", "-t", "20", "-f", "flv",
                ])
                .arg(format!("rtmp://127.0.0.1:{}/live/{key}", receiver.port))
                .spawn()
                .unwrap()
        };
        let send = |key: &str| send_as(key, "libx264");
        let capture = Capture::open(Source::Receiver(receiver.clone()));
        std::thread::sleep(Duration::from_millis(500));
        let mut wrong = send("sbagliata");
        assert!(wait(&capture, &|s| matches!(
            &s.problem,
            Some(Problem::WrongKey(_))
        )));
        let _ = wrong.kill();
        let _ = wrong.wait();
        // H.265, the enhanced way: said
        let mut hevc = send_as(&receiver.key, "libx265");
        assert!(wait(&capture, &|s| s.problem == Some(Problem::Hevc)));
        let _ = hevc.kill();
        let _ = hevc.wait();
        let mut app = send(&receiver.key);
        assert!(wait(&capture, &|s| s.frame.is_some()));
        let status = capture.status().unwrap();
        assert_eq!(status.sender.as_deref(), Some("127.0.0.1"));
        assert_eq!(status.problem, None);
        let mp4 = recorded(&capture);
        let _ = app.kill();
        let _ = app.wait();
        // the B-frames shown after their decoding, as x264 made them
        let video = crate::video::source::Source::open(&mp4).unwrap();
        let reordered = video.samples.windows(2).any(|w| w[1].pts_ms < w[0].pts_ms);
        assert_eq!(reordered, bframes != "0");
    }

    #[test]
    fn the_video_tags() {
        // AVC, a keyframe: the parameters, then a frame shown 40 ms after
        let parameters = [0x17, 0, 0, 0, 0, 1, 0x64];
        assert_eq!(video_tag(&parameters), Ok(Tag::Parameters(&[1, 0x64])));
        let frame = [0x17, 1, 0, 0, 40, 0, 0, 0, 1, 0x65];
        assert_eq!(
            video_tag(&frame),
            Ok(Tag::Frame {
                data: &[0, 0, 0, 1, 0x65],
                key: true,
                composition_ms: 40
            })
        );
        // not a keyframe, shown before (negative)
        let inter = [0x27, 1, 0xff, 0xff, 0xd8, 9];
        assert_eq!(
            video_tag(&inter),
            Ok(Tag::Frame {
                data: &[9],
                key: false,
                composition_ms: -40
            })
        );
        // the end of the stream, a command, nothing
        assert_eq!(video_tag(&[0x17, 2, 0, 0, 0]), Ok(Tag::Nothing));
        assert_eq!(video_tag(&[0x57, 0]), Ok(Tag::Nothing));
        assert_eq!(video_tag(&[]), Ok(Tag::Nothing));
        // enhanced: H.264 by its FourCC, H.265 said
        assert_eq!(
            video_tag(b"\x90avc1\x01\x64"),
            Ok(Tag::Parameters(&[1, 0x64]))
        );
        assert_eq!(
            video_tag(b"\x93avc1\x00\x00\x00\x01\x65"),
            Ok(Tag::Frame {
                data: &[0, 0, 0, 1, 0x65],
                key: true,
                composition_ms: 0
            })
        );
        assert_eq!(video_tag(b"\x90hvc1\x01"), Err(Problem::Hevc));
        assert_eq!(
            video_tag(b"\x90av01\x01"),
            Err(Problem::NoVideo("AV01".into()))
        );
        // the old way: H.265 (as some send it), Sorenson
        assert_eq!(video_tag(&[0x1c, 0]), Err(Problem::Hevc));
        assert_eq!(video_tag(&[0x12, 0]), Err(Problem::NoVideo("H.263".into())));
    }

    #[test]
    fn the_app_and_the_key() {
        let of = |u: &str| app_and_key(&url::Url::parse(u).unwrap());
        assert_eq!(of("rtmp://s/live/abc"), ("live".into(), "abc".into()));
        assert_eq!(
            of("rtmp://s:1936/app/inst/abc?user=u&pass=p"),
            ("app/inst".into(), "abc?user=u&pass=p".into())
        );
        assert_eq!(
            of("rtmp://s/campo?user=u"),
            ("campo?user=u".into(), String::new())
        );
    }

    #[test]
    fn the_receiver_said() {
        let r = Receiver {
            port: 1935,
            key: new_key(),
        };
        assert_eq!(r.key.len(), 8);
        assert_ne!(r.key, new_key());
        assert_eq!(
            r.address("192.168.1.5"),
            format!("rtmp://192.168.1.5:1935/live/{}", r.key)
        );
        assert_eq!(refused("NetStream.Play.StreamNotFound"), Problem::NotFound);
        assert_eq!(refused("Authentication failed"), Problem::Refused);
    }
}
