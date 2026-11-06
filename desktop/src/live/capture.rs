//! A camera on a thread of its own: its pictures shown (the preview) and,
//! once asked, recorded (H.264, `raw`) until stopped, then made an MP4.

use {
    super::{
        picture::{self, I420},
        raw::{self, RawWriter},
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
        time::Instant,
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

/// The recording being made.
#[derive(Debug, Clone)]
pub struct Recording {
    pub elapsed_ms: u64,
}

/// A recording stopped: its MP4 and when it started, or why it couldn't be
/// made one.
#[derive(Debug, Clone)]
pub struct Done {
    pub mp4: PathBuf,
    pub base: PathBuf,
    pub started: DateTime<Local>,
    pub result: Result<u64, String>,
}

#[derive(Debug, Clone, Default)]
pub struct Status {
    pub frame: Option<Arc<Frame>>,
    /// as the camera gives it
    pub size: Option<(u32, u32)>,
    pub recording: Option<Recording>,
    /// the MP4 being made from the recording stopped
    pub saving: bool,
    pub done: Vec<Done>,
    pub error: Option<String>,
}

enum Command {
    Record {
        base: PathBuf,
        started: DateTime<Local>,
        quality: Quality,
    },
    Stop,
    Quit,
}

pub struct Capture {
    commands: Sender<Command>,
    status: Arc<Mutex<Status>>,
    pub camera: CameraEntry,
}

impl Capture {
    /// The camera opened on its thread, its preview at once.
    pub fn open(camera: CameraEntry) -> Capture {
        let (commands, received) = mpsc::channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let shared = status.clone();
        let index = camera.index.clone();
        std::thread::spawn(move || run(index, received, &shared));
        Capture {
            commands,
            status,
            camera,
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
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Quit);
    }
}

fn set(status: &Mutex<Status>, change: impl FnOnce(&mut Status)) {
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

struct Recorder {
    raw: RawWriter,
    encoder: Encoder,
    base: PathBuf,
    started: DateTime<Local>,
    clock: Instant,
    quality: Quality,
    frames: u64,
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
        if let Some(folder) = base.parent() {
            std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        }
        Ok(Recorder {
            raw: RawWriter::create(&base)?,
            encoder,
            base,
            started,
            clock: Instant::now(),
            quality,
            frames: 0,
        })
    }

    fn write(&mut self, picture: I420) -> Result<(), String> {
        let picture = picture::fit(picture, self.quality.height());
        let pts = u64::try_from(self.clock.elapsed().as_millis()).unwrap_or(u64::MAX);
        let yuv = YUVBuffer::from_vec(picture.data, picture.width, picture.height);
        let bits = self
            .encoder
            .encode(&yuv)
            .map_err(|e| e.to_string())?
            .to_vec();
        if !bits.is_empty() {
            self.raw.write(pts, &bits)?;
        }
        self.frames += 1;
        match self.frames % FLUSH_EVERY {
            0 => self.raw.flush(),
            _ => Ok(()),
        }
    }

    fn recording(&self) -> Recording {
        Recording {
            elapsed_ms: u64::try_from(self.clock.elapsed().as_millis()).unwrap_or(u64::MAX),
        }
    }

    /// Made an MP4 next to the raw files.
    fn finish(mut self) -> Done {
        let flushed = self.raw.flush();
        drop(self.raw);
        let mp4 = self.base.with_extension("mp4");
        Done {
            result: flushed.and_then(|()| raw::remux(&self.base, &mp4)),
            mp4,
            base: self.base,
            started: self.started,
        }
    }
}

fn finish(recorder: Option<Recorder>, status: &Mutex<Status>) {
    if let Some(recorder) = recorder {
        set(status, |s| {
            s.recording = None;
            s.saving = true;
        });
        let done = recorder.finish();
        set(status, |s| {
            s.saving = false;
            s.done.push(done);
        });
    }
}

fn run(index: CameraIndex, commands: Receiver<Command>, status: &Mutex<Status>) {
    let mut camera = match open_camera(index) {
        Ok(camera) => camera,
        Err(e) => return set(status, |s| s.error = Some(e)),
    };
    let mut recorder: Option<Recorder> = None;
    let mut serial = 0u64;
    loop {
        match commands.try_recv() {
            Ok(Command::Record {
                base,
                started,
                quality,
            }) => {
                finish(recorder.take(), status);
                match Recorder::start(base, started, quality) {
                    Ok(r) => recorder = Some(r),
                    Err(e) => set(status, |s| s.error = Some(e)),
                }
            }
            Ok(Command::Stop) => finish(recorder.take(), status),
            Ok(Command::Quit) | Err(TryRecvError::Disconnected) => {
                finish(recorder.take(), status);
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
        serial += 1;
        let (w, h, rgba) = picture::preview(&picture, PREVIEW_WIDTH);
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
        let written = recorder.as_mut().map(|r| r.write(picture));
        if let Some(Err(e)) = written {
            set(status, |s| s.error = Some(e));
            finish(recorder.take(), status);
        }
        let recording = recorder.as_ref().map(Recorder::recording);
        set(status, |s| {
            s.frame = Some(frame);
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
