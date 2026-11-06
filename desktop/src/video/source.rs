//! A file of the video: its H.264 track, when each frame is shown, the
//! keyframes to start decoding from, and the frames as openh264 reads them
//! (Annex B: each unit after a start code).

use std::{fs::File, io::BufReader, path::Path};

/// A frame of the file, in the order of decoding.
#[derive(Debug, Clone, Copy)]
pub struct Sample {
    /// When it's shown, from the start of the file.
    pub pts_ms: u64,
    /// A keyframe: decoding can start from it.
    pub sync: bool,
}

pub struct Source {
    reader: mp4::Mp4Reader<BufReader<File>>,
    track: u32,
    /// The parameter sets, to give the decoder before the first frame.
    pub parameters: Vec<u8>,
    /// From the first (sample 1 in the file).
    pub samples: Vec<Sample>,
    pub duration_ms: u64,
}

const START_CODE: [u8; 4] = [0, 0, 0, 1];

impl Source {
    pub fn open(path: &Path) -> Result<Source, String> {
        let file = File::open(path).map_err(|e| e.to_string())?;
        let size = file.metadata().map_err(|e| e.to_string())?.len();
        let reader =
            mp4::Mp4Reader::read_header(BufReader::new(file), size).map_err(|e| e.to_string())?;
        let (track, parameters, samples) = {
            let (id, track) = reader
                .tracks()
                .iter()
                .find(|(_, t)| t.track_type().ok() == Some(mp4::TrackType::Video))
                .ok_or_else(|| "no video in the file".to_string())?;
            let stbl = &track.trak.mdia.minf.stbl;
            let avc = stbl
                .stsd
                .avc1
                .as_ref()
                .ok_or_else(|| "the video is not H.264".to_string())?;
            let parameters: Vec<u8> = avc
                .avcc
                .sequence_parameter_sets
                .iter()
                .chain(&avc.avcc.picture_parameter_sets)
                .flat_map(|unit| START_CODE.iter().chain(&unit.bytes).copied())
                .collect();
            let timescale = u64::from(track.timescale().max(1));
            (
                *id,
                parameters,
                samples(
                    &stbl
                        .stts
                        .entries
                        .iter()
                        .map(|e| (e.sample_count, e.sample_delta))
                        .collect::<Vec<_>>(),
                    &stbl
                        .ctts
                        .as_ref()
                        .map(|ctts| {
                            ctts.entries
                                .iter()
                                .map(|e| (e.sample_count, e.sample_offset))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default(),
                    stbl.stss.as_ref().map(|stss| stss.entries.as_slice()),
                    timescale,
                ),
            )
        };
        let duration_ms = samples
            .iter()
            .map(|s| s.pts_ms)
            .max()
            .map(|last| last + frame_ms(&samples))
            .unwrap_or_else(|| reader.duration().as_millis() as u64);
        Ok(Source {
            reader,
            track,
            parameters,
            samples,
            duration_ms,
        })
    }

    /// The frame at `index` (from 0), its units after start codes.
    pub fn annex_b(&mut self, index: usize) -> Result<Vec<u8>, String> {
        let id = u32::try_from(index + 1).map_err(|e| e.to_string())?;
        let sample = self
            .reader
            .read_sample(self.track, id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("no frame {id}"))?;
        Ok(annex_b(&sample.bytes))
    }

    /// The keyframe to start from to show the frame at `pts_ms`: the last
    /// one shown not after it.
    pub fn keyframe_before(&self, pts_ms: u64) -> usize {
        self.samples
            .iter()
            .enumerate()
            .filter(|(_, s)| s.sync && s.pts_ms <= pts_ms)
            .map(|(i, _)| i)
            .next_back()
            .unwrap_or(0)
    }
}

/// How long a frame is shown: the shortest gap between two.
pub fn frame_ms(samples: &[Sample]) -> u64 {
    let mut shown: Vec<u64> = samples.iter().take(64).map(|s| s.pts_ms).collect();
    shown.sort_unstable();
    shown
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|gap| *gap > 0)
        .min()
        .unwrap_or(33)
}

// the units of the frame, each after its length in 4 bytes (as in MP4),
// each after a start code
fn annex_b(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 16);
    let mut rest = data;
    while let Some((length, after)) = rest.split_first_chunk::<4>() {
        let length = u32::from_be_bytes(*length) as usize;
        let Some(unit) = after.get(..length) else {
            break;
        };
        out.extend_from_slice(&START_CODE);
        out.extend_from_slice(unit);
        rest = after.get(length..).unwrap_or_default();
    }
    out
}

// when each frame is shown, from the tables of the track: the time of
// decoding (stts: how many frames, how long each) plus the delay of showing
// (ctts: how many frames, their delay), and the keyframes (stss, from 1;
// none: all of them)
fn samples(
    durations: &[(u32, u32)],
    delays: &[(u32, i32)],
    sync: Option<&[u32]>,
    timescale: u64,
) -> Vec<Sample> {
    let delays: Vec<i64> = delays
        .iter()
        .flat_map(|(count, delay)| std::iter::repeat_n(i64::from(*delay), *count as usize))
        .collect();
    let sync: Option<std::collections::HashSet<u32>> = sync.map(|s| s.iter().copied().collect());
    durations
        .iter()
        .flat_map(|(count, delta)| std::iter::repeat_n(u64::from(*delta), *count as usize))
        .scan(0u64, |time, delta| {
            let at = *time;
            *time += delta;
            Some(at)
        })
        .enumerate()
        .map(|(i, dts)| {
            let delay = delays.get(i).copied().unwrap_or(0);
            let pts = i64::try_from(dts)
                .unwrap_or(i64::MAX)
                .saturating_add(delay)
                .max(0)
                .unsigned_abs();
            let number = u32::try_from(i + 1).unwrap_or(u32::MAX);
            Sample {
                pts_ms: pts * 1000 / timescale,
                sync: sync.as_ref().is_none_or(|s| s.contains(&number)),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_units_after_start_codes() {
        let data = [0, 0, 0, 2, 0x67, 1, 0, 0, 0, 1, 0x68];
        assert_eq!(annex_b(&data), [0, 0, 0, 1, 0x67, 1, 0, 0, 0, 1, 0x68]);
        // cut short: the units whole only
        assert_eq!(annex_b(&[0, 0, 0, 5, 1, 2]), Vec::<u8>::new());
    }

    /// I P B B, at 30 fps on 90 kHz: the B-frames shown before the P.
    #[test]
    fn when_the_frames_are_shown() {
        let shown = samples(
            &[(4, 3000)],
            &[(1, 3000), (1, 12000), (2, 0)],
            Some(&[1]),
            90_000,
        );
        let at: Vec<u64> = shown.iter().map(|s| s.pts_ms).collect();
        assert_eq!(at, [33, 166, 66, 100]);
        let sync: Vec<bool> = shown.iter().map(|s| s.sync).collect();
        assert_eq!(sync, [true, false, false, false]);
        // no table of keyframes: all of them
        assert!(
            samples(&[(2, 3000)], &[], None, 90_000)
                .iter()
                .all(|s| s.sync)
        );
    }

    #[test]
    fn a_frame_lasts_the_shortest_gap() {
        let at = |ms| Sample {
            pts_ms: ms,
            sync: false,
        };
        // in the order of decoding, B-frames shown before
        assert_eq!(frame_ms(&[at(0), at(100), at(33), at(66), at(200)]), 33);
        assert_eq!(frame_ms(&[]), 33);
    }
}
