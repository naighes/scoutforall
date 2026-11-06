//! The player: a thread decodes the files of the video, one after the
//! other as a single video, and keeps the frame to show at its time; the
//! screen reads it (`status`) at each frame of the window while playing.
//!
//! openh264 gives the frames in the order they're shown, without their
//! time: each takes the earliest time of the frames given to it and not
//! yet out. It needs `Flush::NoFlush`: flushing after each frame (the
//! default of the crate) breaks the order of the B-frames.

use {
    super::source::{Source, frame_ms},
    bytes::Bytes,
    openh264::{
        OpenH264API,
        decoder::{Decoder, DecoderConfig, Flush},
        formats::YUVSource,
    },
    std::{
        cmp::Reverse,
        collections::{BinaryHeap, VecDeque},
        path::PathBuf,
        sync::{
            Arc, Mutex,
            mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError},
        },
        thread,
        time::{Duration, Instant},
    },
};

/// A frame to show, at its time in the video.
#[derive(Debug)]
pub struct Frame {
    /// from 1, in the order the frames were published: the screen shows
    /// the newest
    pub serial: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Bytes,
}

/// What the player is doing, for the screen.
#[derive(Debug, Clone)]
pub struct Status {
    pub frame: Option<Arc<Frame>>,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub playing: bool,
    pub speed: f32,
    /// a file that can't be read or decoded
    pub error: Option<String>,
}

enum Command {
    Play,
    Pause,
    Seek(u64),
    Speed(f32),
}

/// The thread playing: it ends with the player.
pub struct Player {
    commands: Sender<Command>,
    status: Arc<Mutex<Status>>,
    /// of each file, in the order played
    durations_ms: Vec<u64>,
}

impl Player {
    /// The files, one after the other: the first frame on screen.
    pub fn open(paths: &[PathBuf]) -> Result<Player, String> {
        let sources = paths
            .iter()
            .map(|p| Source::open(p).map_err(|e| format!("{}: {e}", p.display())))
            .collect::<Result<Vec<_>, _>>()?;
        let segments = segments(sources);
        let durations_ms = segments.iter().map(|s| s.source.duration_ms).collect();
        let status = Arc::new(Mutex::new(Status {
            frame: None,
            position_ms: 0,
            duration_ms: segments
                .last()
                .map_or(0, |s| s.start_ms + s.source.duration_ms),
            playing: false,
            speed: 1.0,
            error: None,
        }));
        let (commands, received) = mpsc::channel();
        let worker = Worker::new(segments, received, status.clone());
        thread::Builder::new()
            .name("video".into())
            .spawn(move || worker.run())
            .map_err(|e| e.to_string())?;
        let player = Player {
            commands,
            status,
            durations_ms,
        };
        player.seek(0);
        Ok(player)
    }

    pub fn durations_ms(&self) -> &[u64] {
        &self.durations_ms
    }

    pub fn status(&self) -> Option<Status> {
        self.status.lock().ok().map(|s| s.clone())
    }

    pub fn play(&self) {
        let _ = self.commands.send(Command::Play);
    }

    pub fn pause(&self) {
        let _ = self.commands.send(Command::Pause);
    }

    /// The frame shown at `ms` from the start of the video.
    pub fn seek(&self, ms: u64) {
        let _ = self.commands.send(Command::Seek(ms));
    }

    pub fn speed(&self, speed: f32) {
        let _ = self.commands.send(Command::Speed(speed));
    }
}

/// The files one after the other.
fn segments(sources: Vec<Source>) -> Vec<Segment> {
    let mut start_ms = 0;
    sources
        .into_iter()
        .map(|source| {
            let segment = Segment { source, start_ms };
            start_ms += segment.source.duration_ms;
            segment
        })
        .collect()
}

struct Segment {
    source: Source,
    /// where it starts in the video: the files before it, one after the
    /// other
    start_ms: u64,
}

/// Where the decoding is.
struct Cursor {
    segment: usize,
    /// the next frame to give to the decoder
    next: usize,
    decoder: Decoder,
    /// the times of the frames given and not yet out, the earliest first
    pending: BinaryHeap<Reverse<u64>>,
    /// out of the decoder, to give: at their time in the video, without
    /// the pixels when not wanted
    ready: VecDeque<(u64, Option<Frame>)>,
    /// the last file decoded to its end
    done: bool,
}

impl Cursor {
    fn new(segments: &mut [Segment], segment: usize, next: usize) -> Result<Cursor, String> {
        let config = DecoderConfig::new().flush_after_decode(Flush::NoFlush);
        let mut decoder = Decoder::with_api_config(OpenH264API::from_source(), config)
            .map_err(|e| e.to_string())?;
        if let Some(s) = segments.get(segment) {
            // the parameter sets give no frame
            let _ = decoder.decode(&s.source.parameters);
        }
        Ok(Cursor {
            segment,
            next,
            decoder,
            pending: BinaryHeap::new(),
            ready: VecDeque::new(),
            done: false,
        })
    }
}

struct Worker {
    segments: Vec<Segment>,
    frame_ms: u64,
    commands: Receiver<Command>,
    status: Arc<Mutex<Status>>,
    cursor: Option<Cursor>,
    playing: bool,
    speed: f32,
    /// when the frame at the time was shown, to show the next ones at theirs
    clock: (Instant, u64),
    position_ms: u64,
    /// the next frame, decoded, waiting for its time
    held: Option<(u64, Option<Frame>)>,
    /// the frames published
    published: u64,
}

/// Whether the thread goes on: not once the player is gone.
enum Flow {
    On,
    Over,
}

impl Worker {
    fn new(
        segments: Vec<Segment>,
        commands: Receiver<Command>,
        status: Arc<Mutex<Status>>,
    ) -> Worker {
        Worker {
            frame_ms: segments.first().map_or(33, |s| frame_ms(&s.source.samples)),
            segments,
            commands,
            status,
            cursor: None,
            playing: false,
            speed: 1.0,
            clock: (Instant::now(), 0),
            position_ms: 0,
            held: None,
            published: 0,
        }
    }

    fn run(mut self) {
        loop {
            let command = match self.playing {
                true => match self.commands.try_recv() {
                    Ok(c) => Some(c),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => return,
                },
                false => match self.commands.recv() {
                    Ok(c) => Some(c),
                    Err(_) => return,
                },
            };
            let flow = match command {
                Some(c) => {
                    self.command(c);
                    Flow::On
                }
                None => self.step(),
            };
            if let Flow::Over = flow {
                return;
            }
        }
    }

    fn command(&mut self, command: Command) {
        match command {
            Command::Play => {
                if self.position_ms + self.frame_ms >= self.duration_ms() {
                    self.seek(0);
                }
                self.playing = true;
                self.clock = (Instant::now(), self.position_ms);
            }
            Command::Pause => self.playing = false,
            Command::Speed(speed) => {
                self.speed = speed.clamp(0.1, 8.0);
                self.clock = (Instant::now(), self.position_ms);
            }
            Command::Seek(ms) => {
                // dragging the bar: only the last place counts
                let mut target = ms;
                let mut after = vec![];
                while let Ok(c) = self.commands.try_recv() {
                    match c {
                        Command::Seek(ms) => target = ms,
                        other => after.push(other),
                    }
                }
                self.seek(target);
                after.into_iter().for_each(|c| self.command(c));
            }
        }
        self.publish(None);
    }

    fn duration_ms(&self) -> u64 {
        self.segments
            .last()
            .map_or(0, |s| s.start_ms + s.source.duration_ms)
    }

    /// The frame at `target`: decoded from the keyframe before it.
    fn seek(&mut self, target: u64) {
        let target = target.min(self.duration_ms().saturating_sub(self.frame_ms));
        self.held = None;
        self.cursor = match cursor_at(&mut self.segments, target) {
            Ok(cursor) => Some(cursor),
            Err(e) => return self.failed(e),
        };
        let half = self.frame_ms / 2;
        loop {
            match self.next_frame(|at| at + half >= target) {
                Ok(Some((at, Some(frame)))) => {
                    self.position_ms = at;
                    self.clock = (Instant::now(), at);
                    return self.publish(Some(frame));
                }
                Ok(Some((_, None))) => {}
                Ok(None) => return,
                Err(e) => return self.failed(e),
            }
        }
    }

    /// Playing: the next frame, at its time; the ones late skipped.
    fn step(&mut self) -> Flow {
        let (clock, speed, frame_ms) = (self.clock, self.speed, self.frame_ms);
        let due = move |at: u64| {
            let after = at.saturating_sub(clock.1) as f64 / f64::from(speed);
            clock.0 + Duration::from_millis(after as u64)
        };
        let next = match self.held.take() {
            Some(held) => Ok(Some(held)),
            // the pixels only for a frame in time
            None => self.next_frame(|at| Instant::now() < due(at + frame_ms)),
        };
        let (at, frame) = match next {
            Ok(Some(next)) => next,
            Ok(None) => {
                self.playing = false;
                self.publish(None);
                return Flow::On;
            }
            Err(e) => {
                self.failed(e);
                return Flow::On;
            }
        };
        let Some(frame) = frame else {
            self.position_ms = at;
            return Flow::On;
        };
        let wait = due(at).saturating_duration_since(Instant::now());
        match self.commands.recv_timeout(wait) {
            Ok(command) => {
                self.held = Some((at, Some(frame)));
                self.command(command);
                Flow::On
            }
            Err(RecvTimeoutError::Timeout) => {
                self.position_ms = at;
                self.publish(Some(frame));
                Flow::On
            }
            Err(RecvTimeoutError::Disconnected) => Flow::Over,
        }
    }

    /// The next frame shown, at its time in the video, with its pixels when
    /// `wanted`; none at the end of the video.
    fn next_frame(
        &mut self,
        wanted: impl Fn(u64) -> bool,
    ) -> Result<Option<(u64, Option<Frame>)>, String> {
        next_frame(&mut self.segments, &mut self.cursor, wanted)
    }

    fn publish(&mut self, frame: Option<Frame>) {
        let frame = frame.map(|f| {
            self.published += 1;
            Frame {
                serial: self.published,
                ..f
            }
        });
        if let Ok(mut status) = self.status.lock() {
            status.position_ms = self.position_ms;
            status.playing = self.playing;
            status.speed = self.speed;
            if let Some(frame) = frame {
                status.frame = Some(Arc::new(frame));
            }
        }
    }

    fn failed(&mut self, error: String) {
        self.playing = false;
        if let Ok(mut status) = self.status.lock() {
            status.error = Some(error);
            status.playing = false;
        }
    }
}

/// The next frame shown, at its time in the video, with its pixels when
/// `wanted`; none at the end of the video.
fn next_frame(
    segments: &mut [Segment],
    cursor: &mut Option<Cursor>,
    wanted: impl Fn(u64) -> bool,
) -> Result<Option<(u64, Option<Frame>)>, String> {
    let Some(cursor) = cursor else {
        return Ok(None);
    };
    loop {
        if let Some(ready) = cursor.ready.pop_front() {
            return Ok(Some(ready));
        }
        if cursor.done {
            return Ok(None);
        }
        let Some(segment) = segments.get_mut(cursor.segment) else {
            return Ok(None);
        };
        let start = segment.start_ms;
        match segment.source.samples.get(cursor.next).copied() {
            Some(sample) => {
                let data = segment.source.annex_b(cursor.next)?;
                cursor.next += 1;
                cursor.pending.push(Reverse(sample.pts_ms));
                match cursor.decoder.decode(&data) {
                    Ok(Some(yuv)) => {
                        let at = cursor.pending.pop().map_or(sample.pts_ms, |r| r.0) + start;
                        let frame = wanted(at).then(|| frame(&yuv));
                        cursor.ready.push_back((at, frame));
                    }
                    Ok(None) => {}
                    // a frame broken: none out for it
                    Err(_) => cursor.pending.retain(|p| p.0 != sample.pts_ms),
                }
            }
            None => {
                // the end of the file: what the decoder still has
                let flushed = cursor.decoder.flush_remaining().unwrap_or_default();
                for yuv in flushed {
                    if let Some(Reverse(pts)) = cursor.pending.pop() {
                        let at = pts + start;
                        let frame = wanted(at).then(|| frame(&yuv));
                        cursor.ready.push_back((at, frame));
                    }
                }
                let next = cursor.segment + 1;
                match next < segments.len() {
                    true => {
                        let ready = std::mem::take(&mut cursor.ready);
                        *cursor = Cursor::new(segments, next, 0)?;
                        cursor.ready = ready;
                    }
                    false => cursor.done = true,
                }
            }
        }
    }
}

/// A cursor at the keyframe before `target`, in the file it's in.
fn cursor_at(segments: &mut [Segment], target: u64) -> Result<Cursor, String> {
    let segment = segments
        .iter()
        .rposition(|s| s.start_ms <= target)
        .unwrap_or(0);
    let keyframe = segments.get(segment).map_or(0, |s| {
        s.source.keyframe_before(target.saturating_sub(s.start_ms))
    });
    Cursor::new(segments, segment, keyframe)
}

/// The frames of the files one after the other, read in order from a
/// moment: for a video made of some pieces of them (`montage`), not shown.
pub struct Frames {
    segments: Vec<Segment>,
    cursor: Option<Cursor>,
    /// how long a frame is shown
    pub frame_ms: u64,
}

impl Frames {
    pub fn open(paths: &[PathBuf]) -> Result<Frames, String> {
        let sources = paths
            .iter()
            .map(|p| Source::open(p).map_err(|e| format!("{}: {e}", p.display())))
            .collect::<Result<Vec<_>, _>>()?;
        let segments = segments(sources);
        Ok(Frames {
            frame_ms: segments.first().map_or(33, |s| frame_ms(&s.source.samples)),
            segments,
            cursor: None,
        })
    }

    /// The frames from the one at `ms` on: the ones before it decoded,
    /// not given.
    pub fn seek(&mut self, ms: u64) -> Result<(), String> {
        self.cursor = Some(cursor_at(&mut self.segments, ms)?);
        let half = self.frame_ms / 2;
        // the ones before, without their pixels, until the one at `ms`
        loop {
            match next_frame(&mut self.segments, &mut self.cursor, |at| at + half >= ms)? {
                Some((at, Some(frame))) => {
                    if let Some(cursor) = &mut self.cursor {
                        cursor.ready.push_front((at, Some(frame)));
                    }
                    return Ok(());
                }
                Some((_, None)) => {}
                None => return Ok(()),
            }
        }
    }

    /// The next frame, at its time in the video; none at its end.
    pub fn next(&mut self) -> Result<Option<(u64, Frame)>, String> {
        loop {
            match next_frame(&mut self.segments, &mut self.cursor, |_| true)? {
                Some((at, Some(frame))) => return Ok(Some((at, frame))),
                Some((_, None)) => {}
                None => return Ok(None),
            }
        }
    }
}

fn frame(yuv: &openh264::decoder::DecodedYUV) -> Frame {
    let (width, height) = yuv.dimensions();
    let mut rgba = vec![0; width * height * 4];
    yuv.write_rgba8(&mut rgba);
    Frame {
        serial: 0,
        width: u32::try_from(width).unwrap_or(0),
        height: u32::try_from(height).unwrap_or(0),
        rgba: Bytes::from(rgba),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2 s at 30 fps, I-frames at 0 and 1 s, two B-frames between the
    /// others; each frame brighter than the one before (ffmpeg, `geq`,
    /// `lum = 16 + 3n`).
    const CLIP: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/video/testdata/two_seconds.mp4"
    );

    fn worker(files: usize) -> Worker {
        let sources = (0..files)
            .map(|_| Source::open(std::path::Path::new(CLIP)).unwrap())
            .collect();
        let (_, received) = mpsc::channel();
        Worker::new(
            segments(sources),
            received,
            Arc::new(Mutex::new(Status {
                frame: None,
                position_ms: 0,
                duration_ms: 0,
                playing: false,
                speed: 1.0,
                error: None,
            })),
        )
    }

    fn brightness(frame: &Frame) -> u8 {
        frame.rgba[0]
    }

    /// Every frame, at its time, with its brightness.
    fn played(worker: &mut Worker) -> Vec<(u64, u8)> {
        worker.cursor = Some(Cursor::new(&mut worker.segments, 0, 0).unwrap());
        std::iter::from_fn(|| worker.next_frame(|_| true).unwrap())
            .map(|(at, frame)| (at, frame.map_or(0, |f| brightness(&f))))
            .collect()
    }

    fn shown(worker: &Worker) -> (u64, u8) {
        let status = worker.status.lock().unwrap();
        (
            status.position_ms,
            status.frame.as_ref().map_or(0, |f| brightness(f)),
        )
    }

    /// The B-frames in their place: the times and the brightness only go up.
    #[test]
    fn the_frames_in_the_order_they_are_shown() {
        let frames = played(&mut worker(1));
        assert_eq!(frames.len(), 60);
        assert!(frames.windows(2).all(|w| w[0].0 < w[1].0), "{frames:?}");
        assert!(frames.windows(2).all(|w| w[0].1 < w[1].1), "{frames:?}");
    }

    /// From the keyframe before it, the frame asked for, as when played.
    #[test]
    fn a_seek_shows_the_frame_at_its_time() {
        let mut w = worker(1);
        let frames = played(&mut w);
        for target in [0, 500, 1000, 1500, 1990] {
            w.seek(target);
            let expected = frames
                .iter()
                .find(|(at, _)| at + 16 >= target)
                .copied()
                .unwrap();
            assert_eq!(shown(&w), expected, "at {target} ms");
        }
    }

    /// The frames read from a moment on, for a montage: the same as played,
    /// from the one at that moment, across the files.
    #[test]
    fn the_frames_read_from_a_moment() {
        let mut w = worker(2);
        let played = played(&mut w);
        let path = PathBuf::from(CLIP);
        let mut frames = Frames::open(&[path.clone(), path]).unwrap();
        frames.seek(1500).unwrap();
        let read: Vec<(u64, u8)> = std::iter::from_fn(|| frames.next().unwrap())
            .map(|(at, f)| (at, brightness(&f)))
            .collect();
        let from = played.iter().position(|(at, _)| at + 16 >= 1500).unwrap();
        assert_eq!(read, played[from..]);
    }

    /// Two files, one after the other: a single video.
    #[test]
    fn two_files_one_after_the_other() {
        let mut w = worker(2);
        let one = played(&mut worker(1));
        let both = played(&mut w);
        assert_eq!(both.len(), 120);
        assert!(both.windows(2).all(|w| w[0].0 < w[1].0), "{both:?}");
        let duration = w.segments[0].source.duration_ms;
        assert_eq!(both[60], (one[0].0 + duration, one[0].1));
        w.seek(duration + 500);
        let expected = one.iter().find(|(at, _)| at + 16 >= 500).copied().unwrap();
        assert_eq!(shown(&w), (expected.0 + duration, expected.1));
    }
}
