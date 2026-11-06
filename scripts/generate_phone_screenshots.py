#!/usr/bin/env python3
"""The screenshots of the Play listing, in every language, from the phone.

The app takes them itself on the phone connected with USB debugging: the
instrumented test `PlayScreenshots` (android/app/src/androidTest) reaches
the screens as the user does, by the words on them, and saves each one
without the bars of the system, at most twice as tall as wide. For each
language: the example matches (`~/scout4all-esempi/dati/<lang>/scout4all`,
national teams only) copied into the debug app in place of its data, the
language of the app chosen, the test run, the PNGs copied into
`android/fastlane/metadata/android/<locale>/images/phoneScreenshots/`.

    scripts/generate_phone_screenshots.py [--out DIR] [--langs it,en]

Options: `--out` the PNGs in another folder (`<locale>/` each), to look at
them before they replace the listing's; `--langs` fewer languages;
`--examples` another folder of example matches (one `<lang>/scout4all`
each); `--serial` the phone, when more than one is connected; `--no-build`
the apps already installed.

The data of the debug app on the phone are replaced (the example matches
of the last language stay); its language is set back as it was. On a
Xiaomi phone the app may start its own screens from the test (MIUI blocks
it otherwise): set back as it was too.
Needs: adb (the Android SDK), the NDK and cargo-ndk as for any build.
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
ANDROID = ROOT / "android"
LISTING = ANDROID / "fastlane" / "metadata" / "android"
APP = "io.github.naighes.scout4all"
RUNNER = f"{APP}.test/androidx.test.runner.AndroidJUnitRunner"
TEST = f"{APP}.PlayScreenshots"
# where the test saves them, in the files of the app
SHOTS = "files/play-shots"
# the language of the data and of the app, the locale of the listing
LOCALES = {"en": "en-US", "it": "it-IT", "pt": "pt-BR", "pl": "pl-PL", "ja": "ja-JP"}
NAMES = [
    "1_scouting",
    "2_evaluation",
    "3_live_stats",
    "4_distribution",
    "5_detail",
    "6_trend",
    "7_analysis",
    "8_home",
]
# MIUI: the app may start an activity from the background (the test)
MIUI_BACKGROUND_START = "10021"


def fail(message):
    sys.exit(f"generate_phone_screenshots: {message}")


def run(command, **kwargs):
    print("+", " ".join(str(c) for c in command), flush=True)
    return subprocess.run(command, check=True, **kwargs)


class Phone:
    def __init__(self, serial):
        sdk = Path(os.environ.get("ANDROID_HOME", Path.home() / "Library/Android/sdk"))
        found = shutil.which("adb") or str(sdk / "platform-tools" / "adb")
        if not Path(found).exists():
            fail("no adb: set ANDROID_HOME")
        self.adb = [found] + (["-s", serial] if serial else [])
        devices = [
            line.split()[0]
            for line in self.output("devices").splitlines()[1:]
            if line.strip().endswith("device")
        ]
        match (serial, devices):
            case (None, [one]):
                self.serial = one
            case (None, []):
                fail("no phone connected (USB debugging on?)")
            case (None, _):
                fail(f"more phones connected, choose one with --serial: {', '.join(devices)}")
            case (s, _) if s in devices:
                self.serial = s
            case (s, _):
                fail(f"{s} not connected")

    def output(self, *args):
        return subprocess.run(self.adb + list(args), check=True, capture_output=True, text=True).stdout

    def shell(self, *args):
        return self.output("shell", *args)

    def app_locales(self):
        # "Locales for <app> for user 0 are [it-IT]"
        line = self.shell("cmd", "locale", "get-app-locales", APP)
        return line.strip().rsplit("[", 1)[-1].rstrip("]")

    def set_app_locales(self, locales):
        self.shell("cmd", "locale", "set-app-locales", APP, "--locales", locales)

    def miui(self):
        return bool(self.shell("getprop", "ro.miui.ui.version.name").strip())

    def background_start(self):
        # "MIUIOP(10021): ignore; rejectTime=..."
        mode = self.shell("appops", "get", APP, MIUI_BACKGROUND_START)
        return mode.split(":", 1)[-1].split(";", 1)[0].strip() or "default"

    def set_background_start(self, mode):
        self.shell("appops", "set", APP, MIUI_BACKGROUND_START, mode)

    def push_data(self, data):
        """As android/push-data.sh: the data in place of the app's."""
        tmp = "/data/local/tmp/scout4all"
        self.shell("rm", "-rf", tmp)
        run(self.adb + ["push", str(data), tmp], stdout=subprocess.DEVNULL)
        self.shell("run-as", APP, "sh", "-c",
                   f"'rm -rf files/scout4all && mkdir -p files && cp -r {tmp} files/scout4all'")
        self.shell("rm", "-rf", tmp)
        self.shell("am", "force-stop", APP)

    def instrument(self):
        print("+ am instrument", TEST, flush=True)
        result = subprocess.run(
            self.adb + ["shell", "am", "instrument", "-w", "-e", "class", TEST, RUNNER],
            capture_output=True, text=True, timeout=600,
        )
        return result.stdout + result.stderr

    def pull(self, name, to):
        png = subprocess.run(self.adb + ["exec-out", "run-as", APP, "cat", f"{SHOTS}/{name}.png"],
                             check=True, capture_output=True).stdout
        if not png.startswith(b"\x89PNG"):
            return False
        to.write_bytes(png)
        return True


def install():
    gradlew = ANDROID / ("gradlew.bat" if os.name == "nt" else "gradlew")
    run([str(gradlew), "-q", "installDebug", "installDebugAndroidTest"], cwd=ANDROID)


def prepare(examples, lang, into):
    """The example matches of `lang` in a copy, the terms accepted, nothing
    sent."""
    source = examples / lang / "scout4all"
    if not (source / "config.json").exists():
        fail(f"{source}: no example matches")
    data = into / "scout4all"
    shutil.copytree(source, data)
    config_path = data / "config.json"
    config = json.loads(config_path.read_text())
    config.update({"language": lang, "analytics_enabled": False, "terms_version": 99})
    config_path.write_text(json.dumps(config, indent=2))
    return data


def shoot(phone, examples, lang, out, work):
    locale = LOCALES[lang]
    phone.push_data(prepare(examples, lang, work / lang))
    phone.set_app_locales(locale)
    report = phone.instrument()
    if "OK (1 test)" not in report:
        failed = work / f"{lang}-failed.png"
        seen = f"; the screen then: {failed}" if phone.pull("failed", failed) else ""
        print(report, file=sys.stderr)
        fail(f"{lang}: the test failed{seen}")
    folder = out / locale / ("images/phoneScreenshots" if out == LISTING else "")
    folder.mkdir(parents=True, exist_ok=True)
    for old in folder.glob("*.png"):
        old.unlink()
    missing = [name for name in NAMES if not phone.pull(name, folder / f"{name}.png")]
    if missing:
        fail(f"{lang}: not taken: {', '.join(missing)}")
    print(f"{lang}: {len(NAMES)} screenshots in {folder}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--out", type=Path, default=LISTING)
    parser.add_argument("--langs", default=",".join(LOCALES))
    parser.add_argument("--examples", type=Path, default=Path.home() / "scout4all-esempi" / "dati")
    parser.add_argument("--serial")
    parser.add_argument("--no-build", action="store_true")
    args = parser.parse_args()
    langs = [lang.strip() for lang in args.langs.split(",") if lang.strip()]
    unknown = [lang for lang in langs if lang not in LOCALES]
    if unknown:
        fail(f"unknown languages: {', '.join(unknown)} (known: {', '.join(LOCALES)})")
    out = args.out.resolve()

    phone = Phone(args.serial)
    if not args.no_build:
        install()
    locales = phone.app_locales()
    background = phone.background_start() if phone.miui() else None
    try:
        if background is not None:
            phone.set_background_start("allow")
        # kept if a test fails: the screen it was on
        work = Path(tempfile.mkdtemp(prefix="phone-shots-"))
        for lang in langs:
            shoot(phone, args.examples, lang, out, work)
        shutil.rmtree(work)
    finally:
        phone.set_app_locales(locales)
        if background is not None:
            phone.set_background_start(background)
    if out == LISTING:
        run([sys.executable, "-I", str(ANDROID / "fastlane" / "check.py")])


if __name__ == "__main__":
    main()
