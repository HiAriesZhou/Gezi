#!/bin/sh
# Install published macOS commands; no build tools or shell configuration changes.
set -eu

# Parse the whole installer before acting when read from a pipe.
main() {

fail() { printf 'gezi install: %s\n' "$*" >&2; exit 1; }

if [ "$#" -ne 0 ]; then
    printf 'Usage: sh install.sh\n\nOptional environment:\n  GEZI_VERSION      Release tag, e.g. v0.1.7 (default: latest)\n  GEZI_INSTALL_DIR  Absolute directory (default: $HOME/.local/bin)\n\nLegacy TMD_VERSION and TMD_INSTALL_DIR remain accepted.\n'
    [ "$1" = "--help" ] && exit 0
    exit 1
fi

[ "$(uname -s)" = Darwin ] && [ "$(uname -m)" = arm64 ] ||
    fail "Prebuilt packages require Apple Silicon macOS in a native arm64 shell."
for tool in curl shasum tar mktemp install cmp; do
    command -v "$tool" >/dev/null 2>&1 || fail "Required command not found: $tool"
done

release_base="https://github.com/HiAriesZhou/Gezi/releases"
install_dir="${GEZI_INSTALL_DIR:-${TMD_INSTALL_DIR:-$HOME/.local/bin}}"
case "$install_dir" in /*) ;; *) fail "GEZI_INSTALL_DIR must be an absolute path." ;; esac

temporary=""
staged=""
short_staged=""
short_backup=""
short_changed=0
lock=""
cleanup() {
    if [ "$short_changed" -eq 1 ]; then
        if [ -n "$short_backup" ]; then
            if mv -f "$short_backup" "$short_target"; then
                short_backup=""
            else
                printf 'Could not restore ge; previous copy retained at %s\n' "$short_backup" >&2
                short_backup=""
            fi
        else
            rm -f "$short_target"
        fi
    fi
    [ -z "$short_backup" ] || rm -f "$short_backup"
    [ -z "$staged" ] || rm -f "$staged"
    [ -z "$short_staged" ] || rm -f "$short_staged"
    [ -z "$lock" ] || rmdir "$lock"
    [ -z "$temporary" ] || rm -rf "$temporary"
    :
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
temporary=$(mktemp -d "${TMPDIR:-/tmp}/gezi-install.XXXXXXXX")

download() {
    curl --fail --silent --show-error --location \
        --proto '=https' --proto-redir '=https' \
        --connect-timeout 10 --max-time 120 --retry 2 "$@"
}

tag="${GEZI_VERSION:-${TMD_VERSION:-}}"
if [ -z "$tag" ]; then
    latest=$(download --output /dev/null --write-out '%{url_effective}' "$release_base/latest")
    case "$latest" in "$release_base"/tag/*) tag="${latest##*/}" ;; *)
        fail "Could not resolve the latest release tag." ;;
    esac
fi
case "$tag" in *[!v0-9.]*) fail "Invalid release tag." ;; esac
printf '%s\n' "$tag" | LC_ALL=C grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$' ||
    fail "Expected a stable release tag such as v0.1.7."
case "$tag" in v0.0.*|v0.1.[0-6])
    fail "Releases through v0.1.6 use tmd; use install.sh from that release tag." ;;
esac

name="gezi-$tag-aarch64-apple-darwin"
archive="$name.tar.gz"
printf 'Downloading Gezi %s for macOS arm64...\n' "$tag"
download --output "$temporary/$archive" "$release_base/download/$tag/$archive"
download --output "$temporary/$archive.sha256" "$release_base/download/$tag/$archive.sha256"
expected=$(awk '{print $1}' "$temporary/$archive.sha256")
[ "${#expected}" -eq 64 ] || fail "Invalid release checksum."
case "$expected" in *[!0-9a-f]*) fail "Invalid release checksum." ;; esac
printf '%s  %s\n' "$expected" "$archive" > "$temporary/checksum"
(cd "$temporary" && shasum -a 256 -c checksum) || fail "Checksum mismatch; existing installation unchanged."

# Stream only the commands to known files. Never extract archive paths to disk.
for command in gezi ge; do
    tar -xOf "$temporary/$archive" "$name/$command" > "$temporary/$command" ||
        fail "The release package does not contain $command."
done
cmp -s "$temporary/gezi" "$temporary/ge" || fail "The package commands do not match."
chmod 755 "$temporary/gezi"
[ "$("$temporary/gezi" --version)" = "gezi ${tag#v}" ] ||
    fail "The package executable has an unexpected version."

mkdir -p "$install_dir"
install_dir=$(cd "$install_dir" && pwd -P)
mkdir "$install_dir/.gezi-install.lock" 2>/dev/null ||
    fail "Another installation may be running: $install_dir/.gezi-install.lock"
lock="$install_dir/.gezi-install.lock"
target="$install_dir/gezi"
short_target="$install_dir/ge"
for file in "$target" "$short_target"; do
    [ ! -L "$file" ] || fail "Refusing to replace a symlink: $file"
    [ ! -e "$file" ] || [ -f "$file" ] || fail "Not a regular file: $file"
done
if [ -f "$short_target" ] && ! cmp -s "$temporary/gezi" "$short_target"; then
    [ -f "$target" ] && cmp -s "$target" "$short_target" ||
        fail "ge is not a Gezi release binary: $short_target. If Cargo installed it, run cargo uninstall tmd-cli first; otherwise choose a different GEZI_INSTALL_DIR."
fi
if [ -f "$target" ] && [ -x "$target" ] && [ -x "$short_target" ] &&
    cmp -s "$temporary/gezi" "$target" && cmp -s "$temporary/gezi" "$short_target"; then
    printf 'Already installed: %s and %s (%s)\n' "$target" "$short_target" "$tag"
    exit 0
fi

staged=$(mktemp "$install_dir/.gezi-new.XXXXXXXX")
short_staged=$(mktemp "$install_dir/.gezi-new.XXXXXXXX")
install -m 755 "$temporary/gezi" "$staged"
install -m 755 "$temporary/gezi" "$short_staged"
if [ -f "$target" ] && ! cmp -s "$temporary/gezi" "$target"; then
    backup=$(mktemp "$install_dir/gezi.backup.XXXXXXXX")
    cp -p "$target" "$backup"
    printf 'Previous binary saved to %s\n' "$backup"
fi
if [ -f "$short_target" ]; then
    short_backup=$(mktemp "$install_dir/.gezi-previous.XXXXXXXX")
    cp -p "$short_target" "$short_backup"
fi
# Replace both commands as one step: a signal here could leave different
# versions that a rerun would mistake for another tool. A failed mv still
# restores ge through cleanup.
trap '' INT TERM HUP
mv -f "$short_staged" "$short_target"
short_staged=""
short_changed=1
mv -f "$staged" "$target"
staged=""
short_changed=0
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
printf 'Installed %s and %s (%s)\n' "$target" "$short_target" "$tag"
[ ! -e "$install_dir/tmd" ] || printf 'Existing tmd command retained: %s/tmd\n' "$install_dir"
case ":$PATH:" in *":$install_dir:"*) ;; *)
    printf 'Add this directory to your PATH: %s\n' "$install_dir" ;;
esac
}

main "$@"
