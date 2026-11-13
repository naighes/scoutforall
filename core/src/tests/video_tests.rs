//! The video of a match: the start of its files from their names, the
//! anchor the events line up with, the profile the versions before the
//! database saved next to the match (`video.json`).

#[cfg(test)]
mod tests {
    use crate::{
        legacy::folder::{profile_key, read_profile},
        shapes::{r#match::MatchEntry, team::TeamEntry},
        storage::{Store, memory::MemoryStore},
        video::{
            Anchor, AnchorSource, Correction, EventRef, StoredProfile, TimeEffect,
            VIDEO_PROFILE_VERSION, VideoFile, VideoProfile, anchor, anchor_on, anchor_source,
            in_order, offset_ms, place, resolved_path, start_from_name, stored_path,
        },
    };
    use chrono::{DateTime, FixedOffset, TimeDelta, TimeZone, Utc};
    use std::path::Path;
    use uuid::Uuid;

    fn rome() -> FixedOffset {
        FixedOffset::east_opt(3600).unwrap()
    }

    fn at(h: u32, m: u32, s: u32) -> DateTime<FixedOffset> {
        rome().with_ymd_and_hms(2026, 11, 1, h, m, s).unwrap()
    }

    fn start(name: &str) -> Option<DateTime<FixedOffset>> {
        start_from_name(Path::new(name), rome())
    }

    #[test]
    fn the_start_from_the_name_of_the_file() {
        assert_eq!(start("20261101 175252.mp4"), Some(at(17, 52, 52)));
        assert_eq!(start("/videos/20261101 185442.mp4"), Some(at(18, 54, 42)));
        assert_eq!(start("VID_20261101_175252.mp4"), Some(at(17, 52, 52)));
        assert_eq!(start("20261101-175252.mov"), Some(at(17, 52, 52)));
        assert_eq!(start("20261101175252.mp4"), Some(at(17, 52, 52)));
        assert_eq!(
            start("match 20261101T175252 part 2.mp4"),
            Some(at(17, 52, 52))
        );
    }

    #[test]
    fn no_start_without_a_date_and_a_time() {
        assert_eq!(start("match.mp4"), None);
        assert_eq!(start("20261101.mp4"), None);
        assert_eq!(start("20261399 175252.mp4"), None);
        assert_eq!(start("20261101 255252.mp4"), None);
        // more digits than a date and a time: not one
        assert_eq!(start("120261101 175252.mp4"), None);
        assert_eq!(start("20261101 1752521.mp4"), None);
    }

    fn file(path: &str, start: Option<DateTime<FixedOffset>>) -> VideoFile {
        VideoFile {
            path: path.into(),
            start,
        }
    }

    #[test]
    fn the_files_by_start_the_others_after() {
        let files = vec![
            file("c", None),
            file("b", Some(at(18, 54, 42))),
            file("d", None),
            file("a", Some(at(17, 52, 52))),
        ];
        let paths: Vec<String> = in_order(files).into_iter().map(|f| f.path).collect();
        assert_eq!(paths, ["a", "b", "c", "d"]);
    }

    fn a_match(video_clock: bool) -> MatchEntry {
        MatchEntry {
            source: Default::default(),
            video_clock,
            opponent: "Polisportiva".into(),
            date: at(18, 0, 0),
            id: "2026-11-01_polisportiva".into(),
            team: TeamEntry::default(),
            home: true,
            max_substitutions: 6,
            sets: vec![],
            notes: String::new(),
            roster: vec![],
        }
    }

    fn profile(files: Vec<VideoFile>, anchor: Option<Anchor>) -> VideoProfile {
        VideoProfile {
            files,
            anchor,
            ..Default::default()
        }
    }

    /// Imported from DataVolley with the video: the seconds from midnight
    /// are the seconds in the video, whatever the names of the files.
    #[test]
    fn on_the_video_clock_the_seconds_from_midnight() {
        let p = profile(
            vec![file("20261101 175252.mp4", Some(at(17, 52, 52)))],
            None,
        );
        let m = a_match(true);
        let found = anchor(&p, &m).unwrap();
        assert_eq!(anchor_source(&p, &m), Some(AnchorSource::VideoClock));
        let midnight = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap();
        // the first serve of the file of the test, at 378 s
        assert_eq!(
            offset_ms(&found, midnight + TimeDelta::seconds(378)),
            378_000
        );
        // even without files
        assert_eq!(anchor(&profile(vec![], None), &m), Some(found));
    }

    /// Scouted here, on the clock: from the start of the first file.
    #[test]
    fn on_the_clock_from_the_start_of_the_first_file() {
        let p = profile(
            vec![
                file("20261101 175252.mp4", Some(at(17, 52, 52))),
                file("20261101 185442.mp4", Some(at(18, 54, 42))),
            ],
            None,
        );
        let found = anchor(&p, &a_match(false)).unwrap();
        assert_eq!(
            anchor_source(&p, &a_match(false)),
            Some(AnchorSource::FileStart(at(17, 52, 52)))
        );
        let serve = at(18, 0, 0).to_utc();
        assert_eq!(offset_ms(&found, serve), (7 * 60 + 8) * 1000);
        // before the video
        assert_eq!(offset_ms(&found, at(17, 52, 50).to_utc()), -2000);
    }

    /// The two files of the test: 3708.65 s from 17:52:52, then from
    /// 18:54:42 (a pause of 1.35 s).
    fn two_files() -> VideoProfile {
        profile(
            vec![
                file("20261101 175252.mp4", Some(at(17, 52, 52))),
                file("20261101 185442.mp4", Some(at(18, 54, 42))),
            ],
            None,
        )
    }

    const DURATIONS: [u64; 2] = [3_708_650, 2_050_668];

    /// Each event in the file recording at its time: the pause between the
    /// files doesn't count.
    #[test]
    fn the_events_in_the_file_recording() {
        let p = two_files();
        let m = a_match(false);
        let place = |t: DateTime<FixedOffset>| place(&p, &m, &DURATIONS, t.to_utc());
        assert_eq!(place(at(18, 0, 0)), Some(428_000));
        // 2 minutes into the second file: after the whole first
        assert_eq!(place(at(18, 56, 42)), Some(3_708_650 + 120_000));
        // in the pause: the start of the second file
        assert_eq!(place(at(18, 54, 41)), Some(3_708_650));
        // before the video, past its end
        assert_eq!(place(at(17, 52, 50)), Some(-2000));
        let end = 3_708_650 + 2_050_668;
        assert_eq!(
            place(at(19, 30, 0)),
            Some(3_708_650 + (35 * 60 + 18) * 1000)
        );
        assert!(place(at(19, 30, 0)).unwrap() > end);
    }

    /// On the video clock, by hand, or a file without its start: a single
    /// video from the anchor.
    #[test]
    fn otherwise_a_single_video() {
        let p = two_files();
        let midnight = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap();
        let t = midnight + TimeDelta::seconds(3864);
        assert_eq!(place(&p, &a_match(true), &DURATIONS, t), Some(3_864_000));
        let chosen = profile(
            p.files.clone(),
            Some(anchor_on(
                EventRef {
                    set_number: 1,
                    index: 0,
                },
                at(18, 0, 0).to_utc(),
                1000,
            )),
        );
        let later = at(19, 0, 0).to_utc();
        assert_eq!(
            place(&chosen, &a_match(false), &DURATIONS, later),
            Some(3_601_000)
        );
        let unnamed = profile(vec![p.files[0].clone(), file("second.mp4", None)], None);
        assert_eq!(
            place(
                &unnamed,
                &a_match(false),
                &DURATIONS,
                at(18, 56, 42).to_utc()
            ),
            Some((63 * 60 + 50) * 1000)
        );
        // the durations not known: a single video too
        assert_eq!(
            place(&p, &a_match(false), &[], at(18, 0, 0).to_utc()),
            Some(428_000)
        );
    }

    #[test]
    fn no_anchor_without_a_start() {
        let p = profile(vec![file("match.mp4", None)], None);
        assert_eq!(anchor(&p, &a_match(false)), None);
        assert_eq!(anchor_source(&p, &a_match(false)), None);
    }

    /// The anchor of the user wins, on any clock.
    #[test]
    fn the_anchor_chosen_wins() {
        let serve = at(18, 0, 0).to_utc();
        let event = EventRef {
            set_number: 1,
            index: 3,
        };
        let chosen = anchor_on(event, serve, 95_500);
        let p = profile(
            vec![file("20261101 175252.mp4", Some(at(17, 52, 52)))],
            Some(chosen.clone()),
        );
        for clock in [false, true] {
            let found = anchor(&p, &a_match(clock)).unwrap();
            assert_eq!(
                anchor_source(&p, &a_match(clock)),
                Some(AnchorSource::Chosen)
            );
            assert_eq!(found, chosen);
            assert_eq!(offset_ms(&found, serve), 95_500);
            assert_eq!(offset_ms(&found, serve + TimeDelta::seconds(10)), 105_500);
        }
    }

    fn event(index: usize) -> EventRef {
        EventRef {
            set_number: 1,
            index,
        }
    }

    /// The first time written ties the video to the event; then a time
    /// written for another event puts it alone there, the others where
    /// they were; for the event tied, all move again.
    #[test]
    fn a_time_written_ties_the_video_then_puts_the_event_alone() {
        let serve = at(18, 0, 0).to_utc();
        let block = serve + TimeDelta::seconds(15);
        let start = profile(vec![file("/media/match.mp4", None)], None);
        assert_eq!(start.time_effect(event(0)), TimeEffect::AlignAll);
        // the serve at 0:00: the block 15 s after it, as scouted
        let tied = start.with_time(event(0), serve, None, 0);
        assert_eq!(tied.anchor, Some(anchor_on(event(0), serve, 0)));
        let placed = |p: &VideoProfile, e: EventRef, t: DateTime<Utc>| {
            anchor(p, &a_match(false)).map(|a| offset_ms(&a, t) + p.correction(e).unwrap_or(0))
        };
        assert_eq!(placed(&tied, event(1), block), Some(15_000));
        // the block at 0:06: it alone, the serve still at 0:00
        assert_eq!(tied.time_effect(event(1)), TimeEffect::OnlyThis);
        let corrected = tied.with_time(event(1), block, Some(15_000), 6_000);
        assert_eq!(corrected.anchor, tied.anchor);
        assert_eq!(corrected.correction(event(1)), Some(-9_000));
        assert_eq!(placed(&corrected, event(1), block), Some(6_000));
        assert_eq!(placed(&corrected, event(0), serve), Some(0));
        // written again: in place of the first correction
        let again = corrected.with_time(event(1), block, Some(15_000), 7_000);
        assert_eq!(
            again.corrections,
            vec![Correction {
                event: event(1),
                by_ms: -8_000
            }]
        );
        // at where the anchor puts it: no correction
        assert!(again.corrected(event(1), 0).corrections.is_empty());
        // the video shifted: the event corrected moves with it
        let shifted = VideoProfile {
            anchor: Some(anchor_on(event(0), serve, 2_000)),
            ..again.clone()
        };
        assert_eq!(placed(&shifted, event(1), block), Some(9_000));
        // the time of the event tied: all move
        assert_eq!(again.time_effect(event(0)), TimeEffect::AlignAll);
        let moved = again.with_time(event(0), serve, Some(0), 1_000);
        assert_eq!(placed(&moved, event(0), serve), Some(1_000));
        assert_eq!(placed(&moved, event(1), block), Some(8_000));
        // the video tied to the corrected event: its correction gone, the
        // others moved with it
        let retied = again.tied_to(event(1), block, 7_000);
        assert_eq!(retied.correction(event(1)), None);
        assert_eq!(placed(&retied, event(0), serve), Some(-8_000));
        // back to automatic: no anchor, no correction
        let automatic = again.automatic();
        assert_eq!((automatic.anchor, automatic.corrections), (None, vec![]));
    }

    /// Shifted by hand from the sync found by itself, the video is tied to
    /// no event: the first time written ties it.
    #[test]
    fn shifted_by_hand_the_first_time_still_ties_the_video() {
        let shifted = profile(
            vec![file("/media/match.mp4", None)],
            Some(Anchor {
                at: at(18, 0, 0).to_utc(),
                offset_ms: 500,
                event: None,
            }),
        );
        assert_eq!(shifted.time_effect(event(4)), TimeEffect::AlignAll);
    }

    #[test]
    fn the_paths_in_the_folder_of_the_data_are_relative() {
        let data = Path::new("/home/me/.scout4all");
        let inside = Path::new("/home/me/.scout4all/videos/a.mp4");
        let outside = Path::new("/media/videos/b.mp4");
        assert_eq!(stored_path(data, inside), "videos/a.mp4");
        assert_eq!(stored_path(data, outside), "/media/videos/b.mp4");
        assert_eq!(resolved_path(data, "videos/a.mp4"), inside);
        assert_eq!(resolved_path(data, "/media/videos/b.mp4"), outside);
        // the folder moved: the video moves with it
        let moved = Path::new("/new/place");
        assert_eq!(
            resolved_path(moved, &stored_path(data, inside)),
            Path::new("/new/place/videos/a.mp4")
        );
    }

    const MATCH_ID: &str = "2026-11-01_polisportiva";

    /// `video.json` as the versions before the database saved it.
    #[tokio::test]
    async fn the_profile_saved_is_read() {
        let store = MemoryStore::new();
        let team = Uuid::new_v4();
        assert_eq!(
            read_profile(&store, &team, MATCH_ID).await.unwrap(),
            StoredProfile::Missing
        );
        let saved = profile(
            vec![
                file("20261101 175252.mp4", Some(at(17, 52, 52))),
                file("/media/second.mp4", None),
            ],
            Some(anchor_on(
                EventRef {
                    set_number: 2,
                    index: 0,
                },
                at(19, 0, 0).to_utc(),
                3_600_000,
            )),
        )
        .corrected(event(7), -2_500);
        let json = serde_json::to_vec_pretty(&saved).unwrap();
        store
            .write(&profile_key(&team, MATCH_ID), &json)
            .await
            .unwrap();
        assert_eq!(
            read_profile(&store, &team, MATCH_ID).await.unwrap(),
            StoredProfile::Profile(saved)
        );
    }

    /// A profile of a newer version: not read.
    #[tokio::test]
    async fn a_newer_profile_is_left_as_it_is() {
        let store = MemoryStore::new();
        let team = Uuid::new_v4();
        let key = profile_key(&team, MATCH_ID);
        let newer = VIDEO_PROFILE_VERSION + 1;
        let json = format!(r#"{{"version": {newer}, "streams": [{{"url": "rtsp://cam"}}]}}"#);
        store.write(&key, json.as_bytes()).await.unwrap();
        assert_eq!(
            read_profile(&store, &team, MATCH_ID).await.unwrap(),
            StoredProfile::Newer(newer)
        );
    }
}
