#!/usr/bin/env bash
# Uploads files to a GitHub release, or downloads them, retrying when GitHub
# fails: uploads.github.com answers HTTP 500 now and then (the universal macOS
# archive of v0.14.0), and one failed request must not fail a release.
#
#     release-assets.sh upload <tag> <file>...
#     release-assets.sh download <tag> <dir> <asset>...
#
# An upload is done only when the release lists the asset as uploaded with the
# size and the sha256 of the file: a request cut short may leave a broken asset
# behind, which --clobber replaces at the next attempt.
set -euo pipefail

attempts=6

die() { echo "error: $*" >&2; exit 1; }

sha256() {
    if command -v sha256sum > /dev/null; then
        sha256sum "$1" | cut -d ' ' -f 1
    else
        shasum -a 256 "$1" | cut -d ' ' -f 1
    fi
}

# the asset of the release as "<state> <size> <digest>", empty when missing
listed() {
    gh release view "$1" --json assets \
        --jq ".assets[] | select(.name == \"$2\") | \"\(.state) \(.size) \(.digest)\""
}

uploaded() {
    local tag="$1" file="$2" name expected
    name="$(basename "${file}")"
    expected="uploaded $(wc -c < "${file}" | tr -d ' ') sha256:$(sha256 "${file}")"
    [ "$(listed "${tag}" "${name}" || true)" = "${expected}" ]
}

# runs the command until it succeeds and the check (if any) passes, waiting
# longer after each failure: 10, 20, 40, 80, 160 s
retry() {
    local check="$1"; shift
    local attempt=1 wait=10
    while true; do
        if "$@" && { [ -z "${check}" ] || eval "${check}"; }; then
            return 0
        fi
        (( attempt < attempts )) || die "failed ${attempts} times: $*"
        echo "attempt ${attempt} of ${attempts} failed, again in ${wait} s: $*" >&2
        sleep "${wait}"
        attempt=$(( attempt + 1 )); wait=$(( wait * 2 ))
    done
}

command="${1:-}"; shift || true
case "${command}" in
    upload)
        (( $# >= 2 )) || die "usage: release-assets.sh upload <tag> <file>..."
        tag="$1"; shift
        for file in "$@"; do
            [ -f "${file}" ] || die "no such file: ${file}"
            retry "uploaded \"${tag}\" \"${file}\"" gh release upload "${tag}" "${file}" --clobber
            echo "uploaded and verified: $(basename "${file}")"
        done
        ;;
    download)
        (( $# >= 3 )) || die "usage: release-assets.sh download <tag> <dir> <asset>..."
        tag="$1"; dir="$2"; shift 2
        patterns=()
        for asset in "$@"; do patterns+=(--pattern "${asset}"); done
        retry "" gh release download "${tag}" --dir "${dir}" --clobber "${patterns[@]}"
        for asset in "$@"; do
            uploaded "${tag}" "${dir}/${asset}" || die "${asset}: downloaded file differs from the release"
        done
        ;;
    *) die "usage: release-assets.sh <upload|download> ..." ;;
esac
