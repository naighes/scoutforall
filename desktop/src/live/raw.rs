//! A recording on disk while it's made: the H.264 of the encoder as it
//! comes (`.h264`) and a line a frame with its time (`.idx`), both written
//! at once, so that a recording cut short (the app closed badly) is still
//! there up to its last second. Stopped, it becomes an MP4 (`remux`); one
//! left over is made an MP4 the next time (`recover`).

use {
    bytes::Bytes,
    std::{
        fs::{self, File, OpenOptions},
        io::{BufWriter, Read, Write},
        path::{Path, PathBuf},
    },
};

pub const RAW_EXTENSION: &str = "h264";
const INDEX_EXTENSION: &str = "idx";

/// The frame time when only one frame is known.
const FRAME_MS: u32 = 33;

/// The raw files of a recording, from its name without extension.
pub fn raw_paths(base: &Path) -> (PathBuf, PathBuf) {
    (
        base.with_extension(RAW_EXTENSION),
        base.with_extension(INDEX_EXTENSION),
    )
}

pub struct RawWriter {
    stream: BufWriter<File>,
    index: BufWriter<File>,
}

impl RawWriter {
    pub fn create(base: &Path) -> Result<RawWriter, String> {
        let (stream, index) = raw_paths(base);
        let open = |p: &Path| {
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(p)
                .map_err(|e| format!("{}: {e}", p.display()))
        };
        Ok(RawWriter {
            stream: BufWriter::new(open(&stream)?),
            index: BufWriter::new(open(&index)?),
        })
    }

    /// A frame of the encoder (Annex B), `pts_ms` from the start.
    pub fn write(&mut self, pts_ms: u64, annex_b: &[u8]) -> Result<(), String> {
        let key = nals(annex_b).iter().any(|n| nal_type(n) == Some(5));
        self.stream.write_all(annex_b).map_err(|e| e.to_string())?;
        writeln!(self.index, "{pts_ms} {} {}", u8::from(key), annex_b.len())
            .map_err(|e| e.to_string())
    }

    /// On disk: what a crash would leave.
    pub fn flush(&mut self) -> Result<(), String> {
        self.stream.flush().map_err(|e| e.to_string())?;
        self.index.flush().map_err(|e| e.to_string())
    }
}

/// The NAL units of an Annex B stream, without their start codes.
pub fn nals(data: &[u8]) -> Vec<&[u8]> {
    let mut starts = vec![];
    let mut i = 0;
    while i + 3 <= data.len() {
        match (&data[i..i + 3], data.get(i + 3)) {
            ([0, 0, 1], _) => {
                starts.push((i, i + 3));
                i += 3;
            }
            ([0, 0, 0], Some(1)) => {
                starts.push((i, i + 4));
                i += 4;
            }
            _ => i += 1,
        }
    }
    starts
        .iter()
        .enumerate()
        .map(|(k, (_, from))| {
            let to = starts.get(k + 1).map_or(data.len(), |(next, _)| *next);
            &data[*from..to]
        })
        .filter(|n| !n.is_empty())
        .collect()
}

fn nal_type(nal: &[u8]) -> Option<u8> {
    nal.first().map(|b| b & 0x1f)
}

struct Entry {
    pts_ms: u64,
    key: bool,
    len: usize,
}

// the lines of the index: a line cut by a crash, or past the stream, and
// the ones after it, left out
fn entries(index: &str, stream_len: usize) -> Vec<Entry> {
    let mut total = 0usize;
    let mut out = vec![];
    for line in index.lines() {
        let fields: Vec<&str> = line.split(' ').collect();
        let entry = match fields.as_slice() {
            [pts, key, len] => match (pts.parse(), key.parse::<u8>(), len.parse::<usize>()) {
                (Ok(pts_ms), Ok(key), Ok(len)) => Some(Entry {
                    pts_ms,
                    key: key == 1,
                    len,
                }),
                _ => None,
            },
            _ => None,
        };
        match entry {
            Some(e) if total + e.len <= stream_len => {
                total += e.len;
                out.push(e);
            }
            _ => break,
        }
    }
    out
}

/// An MP4 made of a raw recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Remuxed {
    pub length_ms: u64,
    /// left out before the first keyframe (a stream joined between two):
    /// the video starts this much after the recording
    pub skipped_ms: u64,
}

/// The raw recording `base` made the MP4 `mp4`; the raw files removed once
/// it's written. A frame at a time: a match is gigabytes.
pub fn remux(base: &Path, mp4: &Path) -> Result<Remuxed, String> {
    let (stream_path, index_path) = raw_paths(base);
    let open = |p: &Path| File::open(p).map_err(|e| format!("{}: {e}", p.display()));
    let stream_len = open(&stream_path)?
        .metadata()
        .map_err(|e| e.to_string())?
        .len();
    let index =
        fs::read_to_string(&index_path).map_err(|e| format!("{}: {e}", index_path.display()))?;
    let entries = entries(&index, usize::try_from(stream_len).unwrap_or(usize::MAX));
    let mut stream = std::io::BufReader::new(open(&stream_path)?);
    let mut read = |len: usize| -> Result<Vec<u8>, String> {
        let mut data = vec![0; len];
        stream.read_exact(&mut data).map_err(|e| e.to_string())?;
        Ok(data)
    };
    // from the first keyframe with its parameters: before it, nothing to
    // show
    let mut entries = entries.iter().peekable();
    let (first, data) = loop {
        let e = entries
            .next()
            .ok_or_else(|| "no frame to start the video from".to_string())?;
        let data = read(e.len)?;
        if e.key && nals(&data).iter().any(|n| nal_type(n) == Some(7)) {
            break (e, data);
        }
    };
    let parameters = |kind: u8| {
        nals(&data)
            .into_iter()
            .find(|n| nal_type(n) == Some(kind))
            .map(<[u8]>::to_vec)
            .unwrap_or_default()
    };
    let (sps, pps) = (parameters(7), parameters(8));
    let (width, height) = sps_size(&sps).ok_or("no size in the stream")?;
    let file = File::create(mp4).map_err(|e| format!("{}: {e}", mp4.display()))?;
    let config = mp4::Mp4Config {
        major_brand: str::parse("isom").map_err(|e: mp4::Error| e.to_string())?,
        minor_version: 512,
        compatible_brands: ["isom", "iso2", "avc1", "mp41"]
            .iter()
            .map(|b| str::parse(b))
            .collect::<Result<_, mp4::Error>>()
            .map_err(|e| e.to_string())?,
        timescale: 1000,
    };
    let mut writer = mp4::Mp4Writer::write_start(std::io::BufWriter::new(file), &config)
        .map_err(|e| e.to_string())?;
    writer
        .add_track(&mp4::TrackConfig {
            track_type: mp4::TrackType::Video,
            timescale: 1000,
            language: "und".into(),
            media_conf: mp4::MediaConfig::AvcConfig(mp4::AvcConfig {
                width,
                height,
                seq_param_set: sps,
                pic_param_set: pps,
            }),
        })
        .map_err(|e| e.to_string())?;
    let start = first.pts_ms;
    let skipped_ms = start;
    let mut end = 0;
    let mut current = Some((first, data));
    while let Some((e, data)) = current.take() {
        let next = match entries.next() {
            Some(n) => Some((n, read(n.len)?)),
            None => None,
        };
        let duration = next.as_ref().map_or(FRAME_MS, |(n, _)| {
            u32::try_from(n.pts_ms.saturating_sub(e.pts_ms))
                .unwrap_or(FRAME_MS)
                .max(1)
        });
        // the parameters are in the track: the frames without them
        let mut sample = vec![];
        for n in nals(&data)
            .into_iter()
            .filter(|n| !matches!(nal_type(n), Some(7 | 8)))
        {
            let len = u32::try_from(n.len()).map_err(|e| e.to_string())?;
            sample.extend_from_slice(&len.to_be_bytes());
            sample.extend_from_slice(n);
        }
        let start_time = e.pts_ms.saturating_sub(start);
        writer
            .write_sample(
                1,
                &mp4::Mp4Sample {
                    start_time,
                    duration,
                    rendering_offset: 0,
                    is_sync: e.key,
                    bytes: Bytes::from(sample),
                },
            )
            .map_err(|e| e.to_string())?;
        end = start_time + u64::from(duration);
        current = next;
    }
    writer.write_end().map_err(|e| e.to_string())?;
    let mut file = writer.into_writer();
    file.flush().map_err(|e| e.to_string())?;
    // the MP4 there: the raw files no longer needed
    let _ = fs::remove_file(&stream_path);
    let _ = fs::remove_file(&index_path);
    Ok(Remuxed {
        length_ms: end,
        skipped_ms,
    })
}

/// The recordings in `folder` left raw (the app closed while recording):
/// their names without extension.
pub fn left_over(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(folder) else {
        return vec![];
    };
    let mut bases: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == RAW_EXTENSION))
        .map(|p| p.with_extension(""))
        .filter(|base| raw_paths(base).1.is_file())
        .collect();
    bases.sort();
    bases
}

// the width and height in a sequence parameter set (baseline, main and
// high profiles, as openh264 makes them)
fn sps_size(sps: &[u8]) -> Option<(u16, u16)> {
    let rbsp = unescape(sps.get(1..)?);
    let mut bits = Bits::new(&rbsp);
    let profile = bits.read(8)?;
    bits.read(16)?; // constraints, level
    bits.ue()?; // id
    if matches!(profile, 100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128) {
        let chroma = bits.ue()?;
        if chroma == 3 {
            bits.read(1)?;
        }
        bits.ue()?;
        bits.ue()?;
        bits.read(1)?;
        if bits.read(1)? == 1 {
            return None; // scaling matrices: not from openh264
        }
    }
    bits.ue()?; // log2 max frame num
    match bits.ue()? {
        0 => {
            bits.ue()?;
        }
        1 => return None,
        _ => {}
    }
    bits.ue()?; // ref frames
    bits.read(1)?;
    let width_mbs = bits.ue()? + 1;
    let height_units = bits.ue()? + 1;
    let frame_mbs_only = bits.read(1)?;
    if frame_mbs_only == 0 {
        bits.read(1)?;
    }
    bits.read(1)?;
    let (crop_x, crop_y) = match bits.read(1)? {
        1 => {
            let (l, r, t, b) = (bits.ue()?, bits.ue()?, bits.ue()?, bits.ue()?);
            (2 * (l + r), 2 * (t + b) * (2 - frame_mbs_only))
        }
        _ => (0, 0),
    };
    let width = width_mbs * 16 - crop_x;
    let height = height_units * 16 * (2 - frame_mbs_only) - crop_y;
    Some((u16::try_from(width).ok()?, u16::try_from(height).ok()?))
}

fn unescape(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut zeros = 0;
    for &b in data {
        match (zeros >= 2, b) {
            (true, 3) => zeros = 0,
            _ => {
                zeros = match b {
                    0 => zeros + 1,
                    _ => 0,
                };
                out.push(b);
            }
        }
    }
    out
}

struct Bits<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Bits<'a> {
        Bits { data, at: 0 }
    }

    fn bit(&mut self) -> Option<u32> {
        let byte = self.data.get(self.at / 8)?;
        let bit = (byte >> (7 - self.at % 8)) & 1;
        self.at += 1;
        Some(u32::from(bit))
    }

    fn read(&mut self, n: u32) -> Option<u32> {
        (0..n).try_fold(0, |v, _| Some((v << 1) | self.bit()?))
    }

    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        Some((1u32 << zeros) - 1 + self.read(zeros)?)
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::video::source::Source,
        openh264::{
            OpenH264API,
            encoder::{Encoder, EncoderConfig, IntraFramePeriod},
            formats::YUVBuffer,
        },
    };

    /// `frames` frames of 64×48 at 30 fps, a keyframe a second.
    fn record(base: &Path, frames: u64) {
        let config = EncoderConfig::new().intra_frame_period(IntraFramePeriod::from_num_frames(30));
        let mut encoder = Encoder::with_api_config(OpenH264API::from_source(), config).unwrap();
        let mut raw = RawWriter::create(base).unwrap();
        for i in 0..frames {
            let shade = u8::try_from(i * 2 % 256).unwrap();
            let yuv = YUVBuffer::from_vec(vec![shade; 64 * 48 * 3 / 2], 64, 48);
            let bits = encoder.encode(&yuv).unwrap().to_vec();
            raw.write(i * 1000 / 30, &bits).unwrap();
        }
        raw.flush().unwrap();
    }

    #[test]
    fn a_recording_made_an_mp4() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("20261106 111400");
        record(&base, 90);
        let mp4 = base.with_extension("mp4");
        let made = remux(&base, &mp4).unwrap();
        assert!((2990..=3010).contains(&made.length_ms), "{made:?}");
        assert_eq!(made.skipped_ms, 0);
        let source = Source::open(&mp4).unwrap();
        assert_eq!(source.samples.len(), 90);
        assert_eq!(source.samples.iter().filter(|s| s.sync).count(), 3);
        assert!(!raw_paths(&base).0.exists() && !raw_paths(&base).1.exists());
        assert!(left_over(dir.path()).is_empty());
    }

    /// Cut short as by a crash: the stream and the index half written. The
    /// MP4 up to the last whole frame.
    #[test]
    fn a_recording_cut_short() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("20261106 111400");
        record(&base, 60);
        let (stream, index) = raw_paths(&base);
        let bytes = fs::read(&stream).unwrap();
        fs::write(&stream, &bytes[..bytes.len() - 10]).unwrap();
        let text = fs::read_to_string(&index).unwrap();
        fs::write(&index, format!("{text}2000 0 12")).unwrap();
        assert_eq!(left_over(dir.path()), vec![base.clone()]);
        let mp4 = base.with_extension("mp4");
        remux(&base, &mp4).unwrap();
        assert_eq!(Source::open(&mp4).unwrap().samples.len(), 59);
    }

    /// Joined between two keyframes (a stream): the frames before the
    /// first left out, said how long they were.
    #[test]
    fn a_recording_from_its_first_keyframe() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("joined");
        record(&base, 90);
        // the first 10 frames as if not a picture to start from
        let (stream, index) = raw_paths(&base);
        let text = fs::read_to_string(&index).unwrap();
        let bytes = fs::read(&stream).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        let cut: usize = lines[..10]
            .iter()
            .map(|l| l.split(' ').nth(2).unwrap().parse::<usize>().unwrap())
            .sum();
        // the first keyframe's bytes made a frame of no parameters
        let p_frame = bytes[cut..].to_vec();
        fs::write(&stream, [&bytes[cut..cut], &p_frame[..]].concat()).unwrap();
        fs::write(&index, lines[10..].join("\n") + "\n").unwrap();
        let made = remux(&base, &base.with_extension("mp4")).unwrap();
        // from the keyframe at 1 s
        assert_eq!(made.skipped_ms, 1000);
        assert!((1990..=2010).contains(&made.length_ms), "{made:?}");
    }

    #[test]
    fn nothing_to_make_a_video_from() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("empty");
        RawWriter::create(&base).unwrap().flush().unwrap();
        assert!(remux(&base, &base.with_extension("mp4")).is_err());
    }

    #[test]
    fn the_size_in_the_parameters() {
        let config = EncoderConfig::new();
        let mut encoder = Encoder::with_api_config(OpenH264API::from_source(), config).unwrap();
        let yuv = YUVBuffer::from_vec(vec![0; 1920 * 1080 * 3 / 2], 1920, 1080);
        let bits = encoder.encode(&yuv).unwrap().to_vec();
        let sps = nals(&bits)
            .into_iter()
            .find(|n| nal_type(n) == Some(7))
            .unwrap();
        assert_eq!(sps_size(sps), Some((1920, 1080)));
    }
}
