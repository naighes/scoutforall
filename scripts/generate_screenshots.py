#!/usr/bin/env python3
"""The screenshots of the app for the site, in every language.

The desktop app takes them itself (`scout4all-desktop --shots plan.json`):
each language on a copy of the example matches (never `~/.scout4all`), the
screens opened and the keys pressed by the plan, the window saved as PNG,
then converted to the WebP the site uses (`web/src/assets/screens/<lang>/`).

The video screen is taken on the match given with `--video-match`: its
folder in the data (`~/.scout4all/<team>/<match>`), which keeps the video
and how it lines up with the events (the shared archives leave them out).

    scripts/generate_screenshots.py \\
        --video-match ~/.scout4all/<team>/2026-11-06_japan

With `--video-match` it also makes the video at the top of the site
(`web/public/media/<lang>/scouting.mp4`, `.webm` and the poster): the
video of the match playing with its events, then a rally scouted on the
example match in progress, recorded by the app itself.

Options: `--out` the WebP (and the videos) in another folder, to look at them before they
replace the site's; `--video` another file for the video of the match, `--langs en,it`
fewer languages, `--examples` another folder of example matches (one
`<lang>/scout4all` each), `--keep` the temporary folders kept to look at.
`--no-hero` no video for the site.
Needs: cargo, ffmpeg (libx264, libvpx-vp9), node with the packages of `web`
(sharp converts to WebP).
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LANGS = ["en", "it", "pt", "pl", "ja"]
# the window of a 13" Mac at full screen, in points: the captures (twice as
# large on a Retina screen) are made 2000 x 1256 for the site
WINDOW = [1470, 923]
SITE_SIZE = (2000, 1256)
# the example matches: the one in progress, a finished one for the
# statistics (their folders have the name of the opponent in each language,
# the dates are the same)
IN_PROGRESS = "2026-11-05"
FINISHED = "2026-10-19"


def fail(message):
    sys.exit(f"generate_screenshots: {message}")


def run(command, **kwargs):
    print("+", " ".join(str(c) for c in command), flush=True)
    subprocess.run(command, check=True, **kwargs)


def build():
    run(["cargo", "build", "--release", "-p", "scout4all-desktop"], cwd=ROOT)
    app = ROOT / "target" / "release" / "scout4all-desktop"
    if not app.exists():
        fail(f"{app} not built")
    return app


def the_team(data):
    teams = [d for d in data.iterdir() if (d / "team.json").exists()]
    if len(teams) != 1:
        fail(f"{data}: {len(teams)} teams, one expected")
    return teams[0]


def match_on(team, day):
    found = sorted(d.name for d in team.iterdir() if d.is_dir() and d.name.startswith(day))
    if not found:
        fail(f"{team}: no match on {day}")
    return found[0]


def set_language(data, lang):
    config_path = data / "config.json"
    config = json.loads(config_path.read_text()) if config_path.exists() else {}
    # no terms over the screens, nothing sent
    config.update({"language": lang, "analytics_enabled": False, "terms_version": 99})
    config_path.write_text(json.dumps(config, indent=2))


def examples_plan(data, out):
    """Home, scouting, statistics and analysis on the example matches."""
    team = the_team(data)
    t = team.name
    live = match_on(team, IN_PROGRESS)
    over = match_on(team, FINISHED)
    return {
        "window": WINDOW,
        "out": str(out),
        "steps": [
            {"go": "home"},
            {"wait": 600},
            {"shot": "home"},
            {"go": {"scouting": [t, live]}},
            {"wait": 800},
            {"shot": "scouting"},
            # a reception of Sylla, positive, then an attack of Egonu on a
            # super: the evaluation and the zones (the copy is scouted on)
            {"keys": ["p", "2", "+", "a", "4", "s"]},
            {"wait": 400},
            {"shot": "scouting-attack"},
            {"go": {"stats": [t, over]}},
            {"wait": 800},
            {"shot": "stats-summary"},
            {"keys": ["tab", "tab"]},
            {"wait": 600},
            {"shot": "stats-detail"},
            {"go": {"analysis": t}},
            {"wait": 800},
            # the last 4 matches, not the last month: the same whatever the
            # day it's taken
            {"key": "right"},
            {"wait": 1200},
            {"shot": "team-analysis"},
        ],
    }


def video_data(match_dir, video, lang, into):
    """A folder of data with the match of the video alone, in `lang`."""
    match_dir = match_dir.expanduser().resolve()
    team_dir = match_dir.parent
    if not (match_dir / "match.json").exists() or not (team_dir / "team.json").exists():
        fail(f"{match_dir}: not the folder of a match in the data")
    team = into / team_dir.name
    team.mkdir(parents=True)
    shutil.copy2(team_dir / "team.json", team / "team.json")
    shutil.copytree(match_dir, team / match_dir.name)
    profile_path = team / match_dir.name / "video.json"
    if not profile_path.exists():
        fail(f"{match_dir}: no video.json (link the video to the match in the app first)")
    profile = json.loads(profile_path.read_text())
    # the paths saved relative to the data: from the data the match is in
    files = []
    for f in profile.get("files", []):
        path = Path(f["path"])
        path = path if path.is_absolute() else match_dir.parent.parent / path
        files.append({**f, "path": str(path)})
    if video:
        files = [{"path": str(Path(video).expanduser().resolve())}]
    for f in files:
        if not Path(f["path"]).exists():
            fail(f"{f['path']}: the video isn't there")
    profile["files"] = files
    profile_path.write_text(json.dumps(profile, indent=2))
    set_language(into, lang)
    return team_dir.name, match_dir.name


def video_plan(team, match, out):
    """The video with the events beside it, the match over it."""
    return {
        "window": WINDOW,
        "out": str(out),
        "steps": [
            {"go": {"video": [team, match]}},
            {"wait": 2500},
            # an action of the rally, gone to (2 s before it), then played up
            # to it: the match over the video counts it
            {"keys": ["down", "down", "enter"]},
            {"wait": 1500},
            {"key": "space"},
            {"wait": 2300},
            {"key": "space"},
            {"wait": 800},
            {"shot": "video"},
        ],
    }


def video_hero_plan(team, match, out):
    """The video of the match played from its first event: the events lit
    in the list as they come, the score over the video."""
    return {
        "window": WINDOW,
        "out": str(out),
        "steps": [
            {"go": {"video": [team, match]}},
            {"wait": 2500},
            {"key": "enter"},
            {"wait": 1500},
            {"record": "hero-video"},
            {"key": "space"},
            {"wait": 11500},
            {"key": "space"},
            {"wait": 300},
            {"stop": None},
        ],
    }


def scouting_hero_plan(data, out):
    """A rally scouted on the match in progress: a reception with its zone,
    an attack on a super with its zone, a kill, then the serve."""
    team = the_team(data)
    steps = [{"go": {"scouting": [team.name, match_on(team, IN_PROGRESS)]}}, {"wait": 1000}]
    steps += [{"record": "hero-scouting"}, {"wait": 900}]

    def key(k, wait=700):
        steps.extend([{"key": k}, {"wait": wait}])

    def zone(arrows):
        for a in arrows:
            key(a, 180)
        steps.append({"wait": 350})
        key("space", 400)

    key("p"), key("2"), zone(["left", "down"]), key("+", 900)
    key("a"), key("4"), key("s"), zone(["up", "up", "left", "left"]), key("#", 1200)
    key("s"), key("f"), zone(["down", "right", "down"]), key("+", 1500)
    steps.append({"stop": None})
    return {"window": WINDOW, "out": str(out), "steps": steps}


def hero(recordings, dest):
    """The two recordings one after the other, as the site's video: MP4 and
    WebM at the size of the old one, the poster from the first frame."""
    dest.mkdir(parents=True, exist_ok=True)
    video, scouting = recordings / "hero-video.mkv", recordings / "hero-scouting.mkv"
    joined = recordings / "hero.mkv"
    run([
        "ffmpeg", "-v", "error", "-y", "-i", str(video), "-i", str(scouting),
        "-filter_complex",
        "[0:v]scale=1600:-2,setsar=1,fps=30[a];[1:v]scale=1600:-2,setsar=1,fps=30[b];"
        "[a][b]xfade=transition=fade:duration=0.5:offset="
        f"{max(duration(video) - 0.5, 0):.2f}[v]",
        "-map", "[v]", "-c:v", "libx264", "-crf", "10", "-preset", "veryfast", str(joined),
    ])
    run([
        "ffmpeg", "-v", "error", "-y", "-i", str(joined), "-an", "-c:v", "libx264",
        "-crf", "26", "-preset", "slow", "-pix_fmt", "yuv420p", "-movflags", "+faststart",
        str(dest / "scouting.mp4"),
    ])
    run([
        "ffmpeg", "-v", "error", "-y", "-i", str(joined), "-an", "-c:v", "libvpx-vp9",
        "-crf", "38", "-b:v", "0", "-row-mt", "1", "-deadline", "good", "-cpu-used", "2",
        str(dest / "scouting.webm"),
    ])
    run([
        "ffmpeg", "-v", "error", "-y", "-ss", "1", "-i", str(joined), "-frames:v", "1",
        "-q:v", "3", str(dest / "scouting-poster.jpg"),
    ])


def duration(path):
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", str(path)],
        check=True, capture_output=True, text=True,
    )
    return float(out.stdout.strip())


def shoot(app, data, plan, work, name):
    plan_path = work / f"{name}.json"
    plan_path.write_text(json.dumps(plan, indent=2))
    env = {**os.environ, "SCOUT4ALL_DATA_DIR": str(data)}
    run([str(app), "--shots", str(plan_path)], env=env, timeout=600)
    shots = [s["shot"] for s in plan["steps"] if "shot" in s]
    records = [s["record"] for s in plan["steps"] if "record" in s]
    missing = [s for s in shots if not (Path(plan["out"]) / f"{s}.png").exists()]
    missing += [r for r in records if not (Path(plan["out"]) / f"{r}.mkv").exists()]
    if missing:
        fail(f"{name}: not taken: {', '.join(missing)}")


def to_webp(png_dir, names, dest):
    """The captures resized for the site, as WebP (sharp, from `web`)."""
    dest.mkdir(parents=True, exist_ok=True)
    width, height = SITE_SIZE
    script = (
        "const sharp = require('sharp');"
        "const [src, dst, names, w, h] = process.argv.slice(1);"
        "Promise.all(names.split(',').map(n => sharp(`${src}/${n}.png`)"
        ".resize(+w, +h).webp({ quality: 90 }).toFile(`${dst}/${n}.webp`)))"
        ".catch(e => { console.error(e); process.exit(1) });"
    )
    run(
        ["node", "-e", script, str(png_dir), str(dest), ",".join(names), str(width), str(height)],
        cwd=ROOT / "web",
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--video-match", type=Path, help="the folder of the match of the video, in the data")
    parser.add_argument("--video", type=Path, help="another file for the video of the match")
    parser.add_argument("--examples", type=Path, default=Path.home() / "scout4all-esempi" / "dati")
    parser.add_argument("--langs", default=",".join(LANGS))
    parser.add_argument("--keep", action="store_true", help="keep the temporary folders")
    parser.add_argument("--out", type=Path, help="the WebP here (<out>/<lang>/), not in the site, to look at them first")
    parser.add_argument("--no-hero", action="store_true", help="no video for the site")
    args = parser.parse_args()
    langs = [l for l in args.langs.split(",") if l]
    unknown = [l for l in langs if l not in LANGS]
    if unknown:
        fail(f"unknown languages: {', '.join(unknown)}")
    if not (ROOT / "web" / "node_modules" / "sharp").exists():
        fail("web/node_modules/sharp missing: npm install in web")
    making_hero = bool(args.video_match) and not args.no_hero
    if making_hero and not shutil.which("ffmpeg"):
        fail("ffmpeg missing (the video of the site), or --no-hero")
    app = build()
    work = Path(tempfile.mkdtemp(prefix="scout4all-shots-"))
    try:
        for lang in langs:
            source = args.examples / lang / "scout4all"
            if not source.exists():
                fail(f"{source}: no example matches")
            data = work / lang / "data"
            shutil.copytree(source, data)
            set_language(data, lang)
            out = work / lang / "png"
            plan = examples_plan(data, out)
            if args.video_match:
                team, match = video_data(args.video_match, args.video, lang, work / lang / "video-data")
                shoot(app, work / lang / "video-data", video_plan(team, match, out), work, f"{lang}-video")
            shoot(app, data, plan, work, f"{lang}-examples")
            names = sorted(p.stem for p in out.glob("*.png"))
            site = ROOT / "web" / "src" / "assets" / "screens"
            to_webp(out, names, (args.out or site) / lang)
            print(f"{lang}: {', '.join(names)}", flush=True)
            if making_hero:
                recordings = work / lang / "hero"
                shoot(app, work / lang / "video-data", video_hero_plan(team, match, recordings), work, f"{lang}-hero-video")
                fresh = work / lang / "hero-data"
                shutil.copytree(source, fresh)
                set_language(fresh, lang)
                shoot(app, fresh, scouting_hero_plan(fresh, recordings), work, f"{lang}-hero-scouting")
                media = (args.out / lang) if args.out else ROOT / "web" / "public" / "media" / lang
                hero(recordings, media)
                print(f"{lang}: video of the site in {media}", flush=True)
    finally:
        if args.keep:
            print(f"kept: {work}")
        else:
            shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()
