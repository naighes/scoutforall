# Data of the older versions

What the released versions saved and exported, for the migration to the
database and for the import of the old archives (TODO/001, steps 10–13).
Checked by `legacy_fixtures_tests.rs`: every data folder loads and every
archive imports with the current code, and every set replays to the score
the version scouted it with.

## Layout

```
<version>/data/     the data folder (~/.scout4all) the version left
<version>/export/   the archives exported by the version: <team id>.zip,
                    <match id>.zip
generator/          the generator of each version, as it was run
```

One version where the saved data changed (`core/src/shapes`, providers,
`archive.rs`, flags of the events); v0.10.0, v0.14.0 and v0.17.0 save as
the version before them.

| version | what it adds to the data |
|---|---|
| v0.9.0 | team archive without the id in `team.json` (only in the file name), exported by the TUI screens; `lineup_rules`; technique column |
| v0.10.1 | player details (birth date, height, notes), match notes; team id in the archive (`archive.rs`) |
| v0.11.0 | match roster; flags column of the events (inverted P1, reception) |
| v0.12.0 | team country; match `source: dvw` (DataVolley import); `upload_queue.json` |
| v0.13.0 | flags: target zone/cell/pair, blockers |
| v0.15.0 | `video_clock` match, `video.json` profile (not exported); `LF` event |
| v0.16.0 | flags: serve start; `.index.json` (derived) |
| v0.18.0 | same players (ids) in two teams |

## The matches

Every version has the team `Fixture Volley` (13 players, two liberos,
number 13 removed) and, scouted by its generator:

- `2025-10-04_rivali_pisa`: finished, at home, 8 substitutions (from
  v0.11.0 its roster leaves out number 11; in v0.11.0+ the second set has
  the inverted P1);
- `2025-10-11_polisportiva_lucca`: in progress (one set, 40 events of the
  second), away, 6 substitutions;
- `2025-10-18_volley_siena`: no sets (from v0.11.0 without 10 and 12 in the
  roster);
- from v0.12.0, `2025-10-25_visiting`: imported from DataVolley into a new
  team (`Home`) with the players of the file;
- from v0.15.0, `2025-11-08_pallavolo_prato`: scouted on the video
  (`video_clock`), with the profile of two files, an anchor and a
  correction;
- in v0.18.0, `2025-11-15_rivali_pisa_u19` of the team `Fixture Volley U19`,
  which has players 1–8 of `Fixture Volley` with their ids (and a 14th of
  its own): the match has only those players, so both teams fit its
  archive (`teams_of_match`). The core never copies players by itself; the
  case is there for the migration: one person with a membership in each
  team (`legacy::rows::team_rows`).

The finished matches of v0.12.0+ are in `upload_queue.json`, as with the
statistics allowed.

## How they were made

Each match is played by a random scout (seeded, so the same each run) that
picks among the events the version offers (`Snapshot::get_available_options`),
the players who can play them (`can_play`), the evaluations allowed, a
serve type or set type, and the flags the scouting screen of the version
sets (reception on the reception, inverted P1 for the whole set, target,
blockers, serve start: each only when the version has it). Every event goes
through the version's own `Snapshot::add_event`, which validates it as the
scouting does, and is saved through its `SetWriter`. The archives come from
the version's export code: the TUI screens in v0.9.0
(`ExportTeamAction`, `ExportMatchAction`), `archive::export_team` and
`export_match` later, which the apps call.

The DataVolley match is the synthetic `three_sets.dvw` of the tests (3 sets
25-0): the importers of v0.12.0–v0.18.0 refuse all the public files of
`fixtures/dvw` (the first error, home side):

| file | error |
|---|---|
| `example_data.dvw` | set 1, line 310: score 14-9 in the file, 15-8 replayed |
| `PM06.dvw` | set 2, line 372: their touch after the end of the rally |
| `DCup-7.dvw` | set 4, line 1099: unknown code `*~3EH+` |
| `example-click-scout.dvw` | set 1, line 64: our touch without a player |

To make them again (the ids, and so the files, change every time):

```sh
git worktree add --detach /tmp/wt/v0.18.0 v0.18.0
cp generator/v0.18.0.rs /tmp/wt/v0.18.0/core/src/tests/gen_legacy.rs
echo "mod gen_legacy;" >> /tmp/wt/v0.18.0/core/src/tests/mod.rs
cd /tmp/wt/v0.18.0
LEGACY_OUT=/tmp/out/v0.18.0 cargo test -p scout4all-core gen_legacy -- --ignored --nocapture
```

For v0.9.0 the generator is a test of the TUI (it calls its export
screens): copy it to `src/tests/gen_legacy.rs`, add `mod gen_legacy;` to
`src/tests/mod.rs` and run `cargo test --bin scout4all gen_legacy --
--ignored --nocapture`. The generator prints the scores of the sets, which
go in `legacy_fixtures_tests.rs`.
