//! A source of video on a thread of its own: its pictures shown (the
//! preview) and, once asked, recorded (H.264, `raw`) until stopped, then
//! made an MP4. A camera of the computer here; one on the network in
//! `network`.

use {
    super::{
        picture::{self, I420},
        raw::{self, RawWriter},
        space,
    },
    crate::video::Picture as Frame,
    bytes::Bytes,
    chrono::{DateTime, Local},
    nokhwa::{
        Camera,
        pixel_format::RgbFormat,
        utils::{
            ApiBackend, CameraFormat, CameraIndex, FrameFormat, RequestedFormat,
            RequestedFormatType, Resolution,
        },
    },
    openh264::{
        OpenH264API,
        encoder::{BitRate, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, UsageType},
        formats::YUVBuffer,
    },
    std::{
        path::{Path, PathBuf},
        sync::{
            Arc, Mutex,
            mpsc::{self, Receiver, Sender, TryRecvError},
        },
        time::{Duration, Instant},
    },
};

/// How wide the preview is: enough for the screen, cheap to make.
const PREVIEW_WIDTH: usize = 960;

/// The recording on disk every this many frames: a crash loses less.
const FLUSH_EVERY: u64 = 30;

/// A keyframe every two seconds at 30 fps: the player goes anywhere fast.
const KEYFRAME_EVERY: u32 = 60;

/// A camera as the system names it.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraEntry {
    pub name: String,
    pub index: CameraIndex,
}

/// The cameras the system has; the Mac asks first whether the app may use
/// them (`allowed`).
pub fn cameras() -> Result<Vec<CameraEntry>, String> {
    nokhwa::query(ApiBackend::Auto)
        .map(|found| {
            found
                .into_iter()
                .map(|c| CameraEntry {
                    name: c.human_name(),
                    index: c.index().clone(),
                })
                .collect()
        })
        .map_err(|e| e.to_string())
}

/// Asks the system for the cameras (the Mac shows its dialog the first
/// time); `done` gets whether they may be used.
pub fn ask_permission(done: impl Fn(bool) + Send + Sync + 'static) {
    nokhwa::nokhwa_initialize(done);
}

/// The size and the bits of a recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    Full,
    #[default]
    High,
    Low,
}

impl Quality {
    pub const ALL: [Quality; 3] = [Quality::Full, Quality::High, Quality::Low];

    pub fn height(self) -> usize {
        match self {
            Quality::Full => 1080,
            Quality::High => 720,
            Quality::Low => 480,
        }
    }

    pub fn bits_per_second(self) -> u32 {
        match self {
            Quality::Full => 6_000_000,
            Quality::High => 3_500_000,
            Quality::Low => 1_500_000,
        }
    }

    /// About how much an hour of it takes, in GB.
    pub fn gb_per_hour(self) -> f64 {
        f64::from(self.bits_per_second()) * 3600.0 / 8.0 / 1e9
    }

    pub fn key(self) -> &'static str {
        match self {
            Quality::Full => "1080p",
            Quality::High => "720p",
            Quality::Low => "480p",
        }
    }

    pub fn from_key(key: &str) -> Option<Quality> {
        Quality::ALL.into_iter().find(|q| q.key() == key)
    }
}

/// The recording being made: since when, so that how long it's been going
/// on is told by the clock, frames or not (a source on the network cut
/// off: the recording goes on).
#[derive(Debug, Clone)]
pub struct Recording {
    pub since: Instant,
}

impl Recording {
    /// The milliseconds it's been going on.
    pub fn elapsed_ms(&self) -> u64 {
        self.elapsed_ms_at(Instant::now())
    }

    /// The milliseconds it's been going on at `now`.
    pub fn elapsed_ms_at(&self, now: Instant) -> u64 {
        u64::try_from(now.saturating_duration_since(self.since).as_millis()).unwrap_or(u64::MAX)
    }
}

/// A recording stopped: its MP4 and when it started, or why it couldn't be
/// made one.
#[derive(Debug, Clone)]
pub struct Done {
    pub mp4: PathBuf,
    pub base: PathBuf,
    pub started: DateTime<Local>,
    pub result: Result<raw::Remuxed, String>,
    /// stopped as the disk was almost full
    pub out_of_space: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Status {
    pub frame: Option<Arc<Frame>>,
    /// as the source gives it
    pub size: Option<(u32, u32)>,
    pub recording: Option<Recording>,
    /// the MP4 being made from the recording stopped
    pub saving: bool,
    pub done: Vec<Done>,
    pub error: Option<String>,
    /// what went wrong with a source on the network, to say it plainly
    pub problem: Option<super::network::Problem>,
    /// where the app sending the video is (RTMP received), while it sends
    pub sender: Option<String>,
    /// as it comes from a source on the network, once counted
    pub bits_per_second: Option<u64>,
    /// how long before they come its frames were taken (HLS)
    pub late_ms: Option<u64>,
}

/// Where the video comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    Camera(CameraEntry),
    Network(super::network::NetworkSource),
    /// an app sending it to this computer
    Receiver(super::rtmp::Receiver),
}

pub(super) enum Command {
    Record {
        base: PathBuf,
        started: DateTime<Local>,
        quality: Quality,
    },
    Stop,
    /// the preview made at most this often (zero, each frame), or not
    /// (`None`): not while nothing shows it, for the recording to keep up
    Preview(Option<Duration>),
    Quit,
}

pub struct Capture {
    commands: Sender<Command>,
    status: Arc<Mutex<Status>>,
    pub source: Source,
}

impl Capture {
    /// The source opened on its thread, its preview at once.
    pub fn open(source: Source) -> Capture {
        let (commands, received) = mpsc::channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let shared = status.clone();
        match source.clone() {
            Source::Camera(camera) => {
                std::thread::spawn(move || run(camera.index, received, &shared));
            }
            Source::Network(network) => {
                let reading = super::network::Reading::Pull(network);
                std::thread::spawn(move || super::network::run(reading, received, &shared));
            }
            Source::Receiver(receiver) => {
                let reading = super::network::Reading::Receive(receiver);
                std::thread::spawn(move || super::network::run(reading, received, &shared));
            }
        }
        Capture {
            commands,
            status,
            source,
        }
    }

    pub fn status(&self) -> Option<Status> {
        self.status.lock().ok().map(|s| s.clone())
    }

    /// The recordings stopped since last asked.
    pub fn take_done(&self) -> Vec<Done> {
        self.status
            .lock()
            .map(|mut s| std::mem::take(&mut s.done))
            .unwrap_or_default()
    }

    pub fn record(&self, base: PathBuf, started: DateTime<Local>, quality: Quality) {
        let _ = self.commands.send(Command::Record {
            base,
            started,
            quality,
        });
    }

    pub fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
    }

    /// The preview of each frame made or not: off once the app keeps the
    /// recording.
    pub fn preview(&self, on: bool) {
        let _ = self
            .commands
            .send(Command::Preview(on.then_some(Duration::ZERO)));
    }

    /// The preview made at most every `gap`: for a window looking now and
    /// then, the frames between not made pictures.
    pub fn preview_every(&self, gap: Duration) {
        let _ = self.commands.send(Command::Preview(Some(gap)));
    }
}

/// How often a picture of the preview is made: never, each frame, or at
/// most every so often.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Pace {
    every: Option<Duration>,
    last: Option<Instant>,
}

impl Pace {
    pub(super) fn new(every: Option<Duration>) -> Pace {
        Pace { every, last: None }
    }

    /// Each frame: as a source opens, its screen shows it.
    pub(super) fn each_frame() -> Pace {
        Pace::new(Some(Duration::ZERO))
    }

    pub(super) fn is_on(&self) -> bool {
        self.every.is_some()
    }

    /// Whether a picture is made `now`: on, and long enough since the
    /// last one.
    pub(super) fn due(&mut self, now: Instant) -> bool {
        let due = match (self.every, self.last) {
            (None, _) => false,
            (Some(_), None) => true,
            (Some(every), Some(last)) => now.saturating_duration_since(last) >= every,
        };
        if due {
            self.last = Some(now);
        }
        due
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Quit);
    }
}

pub(super) fn set(status: &Mutex<Status>, change: impl FnOnce(&mut Status)) {
    if let Ok(mut s) = status.lock() {
        change(&mut s);
    }
}

fn open_camera(index: CameraIndex) -> Result<Camera, String> {
    // 720p at 30 fps, as close as the camera has; any, if it doesn't say
    let wanted = CameraFormat::new(Resolution::new(1280, 720), FrameFormat::YUYV, 30);
    let mut camera = Camera::new(
        index.clone(),
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(wanted)),
    )
    .or_else(|_| {
        Camera::new(
            index,
            RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate),
        )
    })
    .map_err(|e| e.to_string())?;
    camera.open_stream().map_err(|e| e.to_string())?;
    Ok(camera)
}

fn to_i420(buffer: &nokhwa::Buffer) -> Option<I420> {
    let size = buffer.resolution();
    let (w, h) = (size.width() as usize, size.height() as usize);
    match buffer.source_frame_format() {
        FrameFormat::YUYV => picture::from_yuyv(buffer.buffer(), w, h),
        FrameFormat::NV12 => picture::from_nv12(buffer.buffer(), w, h),
        _ => buffer
            .decode_image::<RgbFormat>()
            .ok()
            .and_then(|rgb| picture::from_rgb(rgb.as_raw(), w, h)),
    }
}

/// A recording on disk, its frames already H.264: from the encoder (a
/// camera) or as they came (the network).
pub(super) struct Take {
    raw: RawWriter,
    base: PathBuf,
    started: DateTime<Local>,
    clock: Instant,
    frames: u64,
    /// the bytes of the frames so far
    written: u64,
    /// the space free where it's written
    free: fn(&Path) -> Option<u64>,
    /// stopping: the disk almost full
    pub(super) out_of_space: bool,
    /// its first frame taken this long before it came (HLS): the
    /// recording started as much before
    late_ms: Option<u64>,
}

impl Take {
    pub(super) fn start(base: PathBuf, started: DateTime<Local>) -> Result<Take, String> {
        Take::start_on(base, started, space::free)
    }

    /// As `start`, the free space told by `free`.
    pub(super) fn start_on(
        base: PathBuf,
        started: DateTime<Local>,
        free: fn(&Path) -> Option<u64>,
    ) -> Result<Take, String> {
        if let Some(folder) = base.parent() {
            std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        }
        Ok(Take {
            raw: RawWriter::create(&base)?,
            base,
            started,
            clock: Instant::now(),
            frames: 0,
            written: 0,
            free,
            out_of_space: false,
            late_ms: None,
        })
    }

    /// How late its first frame came: `late`, the first time asked.
    pub(super) fn late_ms(&mut self, late: u64) -> u64 {
        *self.late_ms.get_or_insert(late)
    }

    /// The milliseconds since it started.
    pub(super) fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.clock.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// A frame (Annex B) at `pts_ms`; on disk every `FLUSH_EVERY`.
    pub(super) fn write(&mut self, pts_ms: u64, annex_b: &[u8]) -> Result<(), String> {
        self.write_shown(pts_ms, 0, annex_b)
    }

    /// As `write`, the frame shown `shown_after_ms` after it's decoded.
    pub(super) fn write_shown(
        &mut self,
        pts_ms: u64,
        shown_after_ms: i32,
        annex_b: &[u8],
    ) -> Result<(), String> {
        if !annex_b.is_empty() {
            self.raw.write_shown(pts_ms, shown_after_ms, annex_b)?;
        }
        self.frames += 1;
        self.written = self
            .written
            .saturating_add(u64::try_from(annex_b.len()).unwrap_or(u64::MAX));
        match self.frames % FLUSH_EVERY {
            0 => {
                // its MP4 made next to it at the end: room left for that
                let folder = self.base.parent().unwrap_or(Path::new("."));
                self.out_of_space = !space::may_go_on((self.free)(folder), self.written);
                self.raw.flush()
            }
            _ => Ok(()),
        }
    }

    /// Whether to stop after a frame written: it failed (said in
    /// `status`) or the disk is almost full.
    pub(super) fn stops(&self, written: Result<(), String>, status: &Mutex<Status>) -> bool {
        match written {
            Err(e) => {
                set(status, |s| s.error = Some(e));
                true
            }
            Ok(()) => self.out_of_space,
        }
    }

    pub(super) fn recording(&self) -> Recording {
        Recording { since: self.clock }
    }

    /// Made an MP4 next to the raw files.
    fn finish(mut self) -> Done {
        let flushed = self.raw.flush();
        drop(self.raw);
        let mp4 = self.base.with_extension("mp4");
        let result = flushed.and_then(|()| raw::remux(&self.base, &mp4));
        // the video starts at its first keyframe: when it was taken,
        // the frames late as they came (HLS) taken before
        let skipped = result.as_ref().map_or(0, |r| r.skipped_ms);
        let skipped = i64::try_from(skipped).unwrap_or(0)
            - i64::try_from(self.late_ms.unwrap_or(0)).unwrap_or(0);
        Done {
            result,
            mp4,
            out_of_space: self.out_of_space,
            base: self.base,
            started: self.started + chrono::TimeDelta::milliseconds(skipped),
        }
    }
}

/// The MP4 made of the recording, if any, said in the status.
pub(super) fn finish(take: Option<Take>, status: &Mutex<Status>) {
    if let Some(take) = take {
        set(status, |s| {
            s.recording = None;
            s.saving = true;
        });
        let done = take.finish();
        set(status, |s| {
            s.saving = false;
            s.done.push(done);
        });
    }
}

/// A camera's recording: its pictures encoded at the quality chosen.
struct Recorder {
    take: Take,
    encoder: Encoder,
    quality: Quality,
}

impl Recorder {
    fn start(
        base: PathBuf,
        started: DateTime<Local>,
        quality: Quality,
    ) -> Result<Recorder, String> {
        let config = EncoderConfig::new()
            .usage_type(UsageType::CameraVideoRealTime)
            .bitrate(BitRate::from_bps(quality.bits_per_second()))
            .max_frame_rate(FrameRate::from_hz(30.0))
            .intra_frame_period(IntraFramePeriod::from_num_frames(KEYFRAME_EVERY));
        let encoder = Encoder::with_api_config(OpenH264API::from_source(), config)
            .map_err(|e| e.to_string())?;
        Ok(Recorder {
            take: Take::start(base, started)?,
            encoder,
            quality,
        })
    }

    fn write(&mut self, picture: I420) -> Result<(), String> {
        let picture = picture::fit(picture, self.quality.height());
        let pts = self.take.elapsed_ms();
        let yuv = YUVBuffer::from_vec(picture.data, picture.width, picture.height);
        let bits = self
            .encoder
            .encode(&yuv)
            .map_err(|e| e.to_string())?
            .to_vec();
        self.take.write(pts, &bits)
    }
}

fn run(index: CameraIndex, commands: Receiver<Command>, status: &Mutex<Status>) {
    let mut camera = match open_camera(index) {
        Ok(camera) => camera,
        Err(e) => return set(status, |s| s.error = Some(e)),
    };
    let mut recorder: Option<Recorder> = None;
    let mut serial = 0u64;
    let mut pace = Pace::each_frame();
    loop {
        match commands.try_recv() {
            Ok(Command::Record {
                base,
                started,
                quality,
            }) => {
                finish(recorder.take().map(|r| r.take), status);
                match Recorder::start(base, started, quality) {
                    Ok(r) => recorder = Some(r),
                    Err(e) => set(status, |s| s.error = Some(e)),
                }
            }
            Ok(Command::Stop) => finish(recorder.take().map(|r| r.take), status),
            Ok(Command::Preview(every)) => pace = Pace::new(every),
            Ok(Command::Quit) | Err(TryRecvError::Disconnected) => {
                finish(recorder.take().map(|r| r.take), status);
                let _ = camera.stop_stream();
                return;
            }
            Err(TryRecvError::Empty) => {}
        }
        let buffer = match camera.frame() {
            Ok(buffer) => buffer,
            Err(e) => {
                set(status, |s| s.error = Some(e.to_string()));
                std::thread::sleep(std::time::Duration::from_millis(200));
                continue;
            }
        };
        let Some(picture) = to_i420(&buffer) else {
            continue;
        };
        let frame = pace.due(Instant::now()).then(|| {
            serial += 1;
            let (w, h, rgba) = picture::preview(&picture, PREVIEW_WIDTH);
            Arc::new(Frame {
                serial,
                width: u32::try_from(w).unwrap_or(0),
                height: u32::try_from(h).unwrap_or(0),
                rgba: Bytes::from(rgba),
            })
        });
        let size = (
            u32::try_from(picture.width).unwrap_or(0),
            u32::try_from(picture.height).unwrap_or(0),
        );
        let stops = recorder.as_mut().is_some_and(|r| {
            let written = r.write(picture);
            r.take.stops(written, status)
        });
        if stops {
            finish(recorder.take().map(|r| r.take), status);
        }
        let recording = recorder.as_ref().map(|r| r.take.recording());
        set(status, |s| {
            if let Some(frame) = frame {
                s.frame = Some(frame);
            }
            s.size = Some(size);
            s.recording = recording;
            s.error = None;
        });
    }
}

/// The name of a recording started at `at`, in `folder`: as the cameras
/// name their files, the start read back from it.
pub fn base_name(folder: &Path, at: DateTime<Local>) -> PathBuf {
    folder.join(at.format("%Y%m%d %H%M%S").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Off, never; each frame, each; every 100 ms, one in the 100 ms and
    /// the next once they're past.
    #[test]
    fn the_pace_of_the_preview() {
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        let mut off = Pace::new(None);
        assert!(!off.is_on());
        assert!(!off.due(at(0)));
        let mut each = Pace::each_frame();
        assert!(each.is_on());
        assert!(each.due(at(0)) && each.due(at(0)) && each.due(at(33)));
        let mut window = Pace::new(Some(Duration::from_millis(100)));
        let made: Vec<u64> = (0..10)
            .map(|i| i * 33)
            .filter(|ms| window.due(at(*ms)))
            .collect();
        assert_eq!(made, vec![0, 132, 264]);
    }
}
