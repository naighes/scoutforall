#!/usr/bin/env bash
# Releases the next version: major, minor or patch.
#
#     scripts/release.sh <major|minor|patch>
#
# From master, up to date with origin and clean, but for the release notes
# of the app (android/fastlane/metadata/android/<language>/changelogs/
# <versionCode>.txt), which go in the release commit if not committed yet.
# Shows what it is about to do and asks; then bumps [workspace.package] of
# Cargo.toml, updates Cargo.lock, commits "release vX.Y.Z.", tags vX.Y.Z and
# pushes both: the tag starts publish.yml (the CLI) and publish-android.yml
# (the app, to Play).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

die() { echo "error: $*" >&2; exit 1; }

part="${1:-}"
case "${part}" in
    major|minor|patch) ;;
    *) die "usage: scripts/release.sh <major|minor|patch>" ;;
esac

# --- where we are -------------------------------------------------------

branch="$(git rev-parse --abbrev-ref HEAD)"
[ "${branch}" = master ] || die "on ${branch}: releases start from master"

git fetch --quiet --tags origin master
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/master)" ] \
    || die "master differs from origin/master: pull or push first"

notes_path='android/fastlane/metadata/android/*/changelogs/*.txt'
others="$(git status --porcelain --untracked-files=all | grep -v -E '^.. android/fastlane/metadata/android/[^/]+/changelogs/[0-9]+\.txt$' || true)"
[ -z "${others}" ] || die "changes besides the release notes:
${others}"

# --- the versions -------------------------------------------------------

current="$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' Cargo.toml)"
[[ "${current}" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]] || die "no version in [workspace.package] of Cargo.toml"
major="${BASH_REMATCH[1]}"; minor="${BASH_REMATCH[2]}"; patch="${BASH_REMATCH[3]}"
case "${part}" in
    major) next="$((major + 1)).0.0" ;;
    minor) next="${major}.$((minor + 1)).0" ;;
    patch) next="${major}.${minor}.$((patch + 1))" ;;
esac
tag="v${next}"
IFS=. read -r a b c <<< "${next}"
(( b < 100 && c < 100 )) || die "${next}: minor and patch must stay below 100 (the versionCode of the app)"
code=$(( a * 10000 + b * 100 + c ))

git rev-parse -q --verify "refs/tags/${tag}" > /dev/null && die "the tag ${tag} exists already"
[ -z "$(git ls-remote --tags origin "refs/tags/${tag}")" ] || die "the tag ${tag} exists already on origin"

# the release notes of the app, in every language, within the limits of Play
missing="$(for d in android/fastlane/metadata/android/*/; do
    [ -s "${d}changelogs/${code}.txt" ] || echo "  ${d}changelogs/${code}.txt"
done)"
[ -z "${missing}" ] || die "no release notes for ${next} (versionCode ${code}):
${missing}"

# --- what it is about to do ---------------------------------------------

previous="v${current}"
git rev-parse -q --verify "refs/tags/${previous}" > /dev/null || previous="$(git describe --tags --abbrev=0 2>/dev/null || true)"

echo
echo "You are about to release ${tag} (${part}): ${current} → ${next}, versionCode ${code}."
echo
if [ -n "${previous}" ]; then
    echo "Commits since ${previous}:"
    git log --oneline --no-decorate "${previous}..HEAD" | sed 's/^/  /'
    echo
fi
echo "Release notes of the app (en-US):"
sed 's/^/  /' "android/fastlane/metadata/android/en-US/changelogs/${code}.txt"
echo
echo "It will:"
echo "  1. set version = \"${next}\" in Cargo.toml and update Cargo.lock"
echo "  2. commit \"release ${tag}.\" on master$(git status --porcelain -- "${notes_path}" | grep -q . && echo ', with the release notes not committed yet')"
echo "  3. tag ${tag} and push master and the tag to origin"
echo "  4. the tag starts publish.yml (the CLI) and publish-android.yml (the app, to Play)"
echo
read -r -p "Release ${tag}? [y/N] " answer
[[ "${answer}" =~ ^[yY]$ ]] || { echo "Nothing done."; exit 0; }

# --- the release --------------------------------------------------------

sed -i.bak "/^\[workspace.package\]/,/^\[/s/^version *= *\"${current}\"/version = \"${next}\"/" Cargo.toml
rm Cargo.toml.bak
[ "$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' Cargo.toml)" = "${next}" ] \
    || { git checkout -- Cargo.toml; die "could not set the version in Cargo.toml"; }
cargo update --workspace --quiet
python3 -I android/fastlane/check.py --release \
    || { git checkout -- Cargo.toml Cargo.lock; die "the listing or the release notes are not within the limits of Play"; }

git add Cargo.toml Cargo.lock
git add -- "${notes_path}"
git commit --quiet -m "release ${tag}."
git tag -a "${tag}" -m "${tag}"
git push --atomic origin master "${tag}"

echo
echo "Released ${tag}: the runs are in the Actions of the repository."
