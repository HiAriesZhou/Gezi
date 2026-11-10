#!/bin/sh
# Install a published macOS binary; no build tools or shell configuration changes.
set -eu

# Parse the whole installer before acting when read from a pipe.
main() {

fail() { printf 'tmd install: %s\n' "$*" >&2; exit 1; }

if [ "$#" -ne 0 ]; then
    printf 'Usage: sh install.sh\n\nOptional environment:\n  TMD_VERSION      Release tag, e.g. v0.1.4 (default: latest)\n  TMD_INSTALL_DIR  Absolute directory (default: $HOME/.local/bin)\n'
    [ "$1" = "--help" ] && exit 0
    exit 1
fi

[ "$(uname -s)" = Darwin ] && [ "$(uname -m)" = arm64 ] ||
    fail "Prebuilt packages require Apple Silicon macOS in a native arm64 shell."
for tool in curl shasum tar mktemp install cmp; do
    command -v "$tool" >/dev/null 2>&1 || fail "Required command not found: $tool"
done

release_base="https://github.com/HiAriesZhou/TMD/releases"
install_dir="${TMD_INSTALL_DIR:-$HOME/.local/bin}"
case "$install_dir" in /*) ;; *) fail "TMD_INSTALL_DIR must be an absolute path." ;; esac

temporary=""
staged=""
lock=""
cleanup() {
    [ -z "$staged" ] || rm -f "$staged"
    [ -z "$lock" ] || rmdir "$lock"
    [ -z "$temporary" ] || rm -rf "$temporary"
    :
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
temporary=$(mktemp -d "${TMPDIR:-/tmp}/tmd-install.XXXXXXXX")

download() {
    curl --fail --silent --show-error --location \
        --proto '=https' --proto-redir '=https' \
        --connect-timeout 10 --max-time 120 --retry 2 "$@"
}

tag="${TMD_VERSION:-}"
if [ -z "$tag" ]; then
    latest=$(download --output /dev/null --write-out '%{url_effective}' "$release_base/latest")
    case "$latest" in "$release_base"/tag/*) tag="${latest##*/}" ;; *)
        fail "Could not resolve the latest release tag." ;;
    esac
fi
case "$tag" in *[!v0-9.]*) fail "Invalid release tag." ;; esac
printf '%s\n' "$tag" | LC_ALL=C grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$' ||
    fail "Expected a stable release tag such as v0.1.4."

name="tmd-$tag-aarch64-apple-darwin"
archive="$name.tar.gz"
printf 'Downloading tmd %s for macOS arm64...\n' "$tag"
download --output "$temporary/$archive" "$release_base/download/$tag/$archive"
download --output "$temporary/$archive.sha256" "$release_base/download/$tag/$archive.sha256"
expected=$(awk '{print $1}' "$temporary/$archive.sha256")
[ "${#expected}" -eq 64 ] || fail "Invalid release checksum."
case "$expected" in *[!0-9a-f]*) fail "Invalid release checksum." ;; esac
printf '%s  %s\n' "$expected" "$archive" > "$temporary/checksum"
(cd "$temporary" && shasum -a 256 -c checksum) || fail "Checksum mismatch; existing installation unchanged."

# Stream only the executable to a known file. Never extract archive paths to disk.
tar -xOf "$temporary/$archive" "$name/tmd" > "$temporary/tmd" ||
    fail "The release package does not contain tmd."
chmod 755 "$temporary/tmd"
[ "$("$temporary/tmd" --version)" = "tmd ${tag#v}" ] ||
    fail "The package executable has an unexpected version."

mkdir -p "$install_dir"
install_dir=$(cd "$install_dir" && pwd -P)
mkdir "$install_dir/.tmd-install.lock" 2>/dev/null ||
    fail "Another installation may be running: $install_dir/.tmd-install.lock"
lock="$install_dir/.tmd-install.lock"
target="$install_dir/tmd"
[ ! -L "$target" ] || fail "Refusing to replace a symlink: $target"
[ ! -e "$target" ] || [ -f "$target" ] || fail "Not a regular file: $target"
if [ -f "$target" ] && cmp -s "$temporary/tmd" "$target"; then
    printf 'Already installed: %s (%s)\n' "$target" "$tag"
    exit 0
fi

staged=$(mktemp "$install_dir/.tmd-new.XXXXXXXX")
install -m 755 "$temporary/tmd" "$staged"
if [ -f "$target" ]; then
    backup=$(mktemp "$install_dir/tmd.backup.XXXXXXXX")
    cp -p "$target" "$backup"
    printf 'Previous binary saved to %s\n' "$backup"
fi
mv -f "$staged" "$target"
staged=""
printf 'Installed %s (%s)\n' "$target" "$tag"
case ":$PATH:" in *":$install_dir:"*) ;; *)
    printf 'Add this directory to your PATH: %s\n' "$install_dir" ;;
esac
}

main "$@"
