//! The screenshots of the app for the site, taken by the app itself
//! (`scout4all-desktop --shots plan.json`, `scripts/generate_screenshots.py`):
//! the steps of the plan one after the other (a screen opened, keys, a wait,
//! a capture of the window saved as PNG), then the app closes. The window
//! is captured by the app, not by the system: no keys sent to other windows,
//! the same size every time.
//!
//! Between `record` and `stop` the window is captured over and over while
//! the steps go on, the frames given to ffmpeg at 30 a second (the last one
//! again while the next is taken): `<name>.mkv`, for the video of the site.

use {
    iced::window::Screenshot,
    scout4all_keys::KeyCombination,
    serde::Deserialize,
    std::{
        collections::VecDeque,
        io::Write,
        path::PathBuf,
        process::{Child, Command, Stdio},
        sync::mpsc,
        thread::JoinHandle,
        time::{Duration, Instant},
    },
};

/// The frames a second of the recordings.
const FPS: u32 = 30;

/// A screen to open, by the ids of its team and match.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Open {
    Home,
    Team(String),
    Match(String, String),
    /// the set of the match in progress
    Scouting(String, String),
    Stats(String, String),
    Analysis(String),
    Video(String, String),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Go(Open),
    Key(KeyCombination),
    Keys(Vec<KeyCombination>),
    /// milliseconds, for what loads (the frames of the video, a screen)
    Wait(u64),
    /// the window saved as `<name>.png` in the folder of the plan
    Shot(String),
    /// the window recorded from here as `<name>.mkv`, up to `stop`
    Record(String),
    Stop,
}

#[derive(Debug, Deserialize)]
pub struct Plan {
    /// the size of the window, in points
    pub window: (f32, f32),
    /// where the captures go
    pub out: PathBuf,
    pub steps: Vec<Step>,
}

/// What the plan asks next.
pub enum Next {
    /// a screen opened or keys pressed: on with the steps after them
    Do(Step),
    /// the steps after it once the time passed
    Wait(Duration),
    /// the window captured, saved as the name
    Shot(String),
    /// the recording started: frames to take, the steps going on
    Recording,
    /// the recording over (`Shots::stop`)
    Stopped(Result<String, String>),
    Done,
}

pub struct Shots {
    pub plan_window: (f32, f32),
    out: PathBuf,
    steps: VecDeque<Step>,
    pub started: bool,
    recorder: Option<Recorder>,
}

/// A frame of the window and when it was taken.
type Frame = (Instant, u32, u32, bytes::Bytes);

/// The frames to ffmpeg, on a thread of their own.
struct Recorder {
    frames: mpsc::Sender<Frame>,
    thread: JoinHandle<Result<usize, String>>,
    started: Instant,
    taken: usize,
}

/// How long the screen is left to draw before a capture.
const SETTLE: Duration = Duration::from_millis(300);

impl Shots {
    /// The plan given with `--shots`, if any; unreadable, said and the app
    /// not started.
    pub fn from_args() -> Option<Result<Shots, String>> {
        let args: Vec<String> = std::env::args().collect();
        let at = args.iter().position(|a| a == "--shots")?;
        Some(
            args.get(at + 1)
                .ok_or_else(|| "--shots needs the path of the plan".to_string())
                .and_then(|path| Shots::read(path)),
        )
    }

    fn read(path: &str) -> Result<Shots, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let plan: Plan = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
        std::fs::create_dir_all(&plan.out).map_err(|e| format!("{}: {e}", plan.out.display()))?;
        Ok(Shots {
            plan_window: plan.window,
            out: plan.out,
            steps: plan.steps.into(),
            started: false,
            recorder: None,
        })
    }

    /// The window recorded as `<name>.mkv` from now on.
    pub fn record(&mut self, name: &str) {
        let path = self.out.join(format!("{name}.mkv"));
        let (frames, rx) = mpsc::channel::<Frame>();
        let thread = std::thread::spawn(move || encode(&rx, &path));
        self.recorder = Some(Recorder {
            frames,
            thread,
            started: Instant::now(),
            taken: 0,
        });
    }

    pub fn recording(&self) -> bool {
        self.recorder.is_some()
    }

    /// A frame of the recording.
    pub fn frame(&mut self, shot: &Screenshot) {
        if let Some(recorder) = &mut self.recorder {
            recorder.taken += 1;
            let _ = recorder.frames.send((
                Instant::now(),
                shot.size.width,
                shot.size.height,
                shot.rgba.clone(),
            ));
        }
    }

    /// The recording over: the file written; how many frames were taken
    /// and written, or why not.
    pub fn stop(&mut self) -> Result<String, String> {
        let Some(recorder) = self.recorder.take() else {
            return Err("stop without record".to_string());
        };
        let seconds = recorder.started.elapsed().as_secs_f64();
        drop(recorder.frames);
        let written = recorder
            .thread
            .join()
            .map_err(|_| "the recording stopped".to_string())??;
        Ok(format!(
            "{} frames taken in {seconds:.1} s ({:.1} a second), {written} written",
            recorder.taken,
            recorder.taken as f64 / seconds.max(0.001)
        ))
    }

    pub fn next(&mut self) -> Next {
        match self.steps.pop_front() {
            None => Next::Done,
            Some(Step::Wait(ms)) => Next::Wait(Duration::from_millis(ms)),
            Some(Step::Shot(name)) => Next::Shot(name),
            Some(Step::Record(name)) => {
                self.record(&name);
                Next::Recording
            }
            Some(Step::Stop) => Next::Stopped(self.stop()),
            Some(step) => Next::Do(step),
        }
    }

    /// The capture saved; the path, or why not.
    pub fn save(&self, name: &str, shot: &Screenshot) -> Result<PathBuf, String> {
        let path = self.out.join(format!("{name}.png"));
        let size = tiny_skia::IntSize::from_wh(shot.size.width, shot.size.height)
            .ok_or_else(|| format!("{name}: empty capture"))?;
        // the window is opaque: its pixels are premultiplied as they are
        let pixmap = tiny_skia::Pixmap::from_vec(shot.rgba.to_vec(), size)
            .ok_or_else(|| format!("{name}: {} bytes for {size:?}", shot.rgba.len()))?;
        pixmap
            .save_png(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }
}

/// The frames at `FPS` into ffmpeg, each written for as long as it was on
/// screen: the time of the recording as it went. The number written.
fn encode(frames: &mpsc::Receiver<Frame>, path: &std::path::Path) -> Result<usize, String> {
    let slot = Duration::from_secs(1) / FPS;
    let mut ffmpeg: Option<(Child, Instant)> = None;
    let mut last: Option<bytes::Bytes> = None;
    let mut written = 0usize;
    let write = |ffmpeg: &mut Child, bytes: &[u8]| -> Result<(), String> {
        ffmpeg
            .stdin
            .as_mut()
            .ok_or("ffmpeg without input")?
            .write_all(bytes)
            .map_err(|e| format!("ffmpeg: {e}"))
    };
    for (at, width, height, rgba) in frames {
        let (child, start) = match &mut ffmpeg {
            Some(running) => running,
            None => ffmpeg.insert((spawn(path, width, height)?, at)),
        };
        // the frames the time up to this one asks for: the last one again
        let due = usize::try_from(at.duration_since(*start).as_nanos() / slot.as_nanos())
            .unwrap_or(usize::MAX);
        while written < due {
            if let Some(bytes) = &last {
                write(child, bytes)?;
            }
            written += 1;
        }
        last = Some(rgba);
    }
    if let (Some((mut child, _)), Some(bytes)) = (ffmpeg, last) {
        write(&mut child, &bytes)?;
        written += 1;
        drop(child.stdin.take());
        let status = child.wait().map_err(|e| format!("ffmpeg: {e}"))?;
        if !status.success() {
            return Err(format!("ffmpeg: {status}"));
        }
    }
    Ok(written)
}

fn spawn(path: &std::path::Path, width: u32, height: u32) -> Result<Child, String> {
    Command::new("ffmpeg")
        .args([
            "-y",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
        ])
        .args([
            "-s",
            &format!("{width}x{height}"),
            "-r",
            &FPS.to_string(),
            "-i",
            "-",
        ])
        .args([
            "-c:v", "libx264", "-preset", "veryfast", "-crf", "12", "-pix_fmt", "yuv420p",
        ])
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg: {e}"))
}

/// Waits `time` off the thread of the app.
pub async fn after(time: Duration) {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        std::thread::sleep(time);
        let _ = tx.send(());
    });
    let _ = rx.await;
}

pub const fn settle() -> Duration {
    SETTLE
}

#[cfg(test)]
mod tests {
    use {super::*, scout4all_keys::key};

    #[test]
    fn a_plan_read() {
        let plan: Plan = serde_json::from_str(
            r#"{
                "window": [1440, 900],
                "out": "/tmp/shots",
                "steps": [
                    {"go": "home"},
                    {"go": {"video": ["team", "match"]}},
                    {"keys": ["down", "enter"]},
                    {"key": "shift-s"},
                    {"wait": 1500},
                    {"shot": "video"}
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(plan.window, (1440.0, 900.0));
        assert_eq!(plan.steps.len(), 6);
        assert!(
            matches!(&plan.steps[1], Step::Go(Open::Video(t, m)) if t == "team" && m == "match")
        );
        assert!(matches!(&plan.steps[2], Step::Keys(k) if *k == vec![key!(down), key!(enter)]));
        assert!(matches!(&plan.steps[3], Step::Key(k) if *k == key!(shift-s)));
        let mut shots = Shots {
            plan_window: plan.window,
            out: plan.out,
            steps: plan.steps.into(),
            started: false,
            recorder: None,
        };
        assert!(matches!(shots.next(), Next::Do(Step::Go(Open::Home))));
        let _ = (shots.next(), shots.next(), shots.next());
        assert!(matches!(shots.next(), Next::Wait(d) if d == Duration::from_millis(1500)));
        assert!(matches!(shots.next(), Next::Shot(n) if n == "video"));
        assert!(matches!(shots.next(), Next::Done));
    }
}
