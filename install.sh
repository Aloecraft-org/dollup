#!/bin/sh
# Install dollup.
#
#   curl -fsSL https://github.com/Aloecraft-org/dollup/releases/latest/download/install.sh | sh
#   curl -fsSL https://software.aloecraft.org/releases/dollup/latest/install.sh | sh
#
# One file, verified against the SHA256SUMS.txt published beside it, into a
# directory you already own. It installs nothing else and touches nothing
# outside $DOLLUP_PREFIX. GitHub, the origin, is asked first and the release
# mirror second, for as long as the mirror lags; which one answered is
# printed.
#
# Knobs: DOLLUP_VERSION=vX.Y.Z pins a release; DOLLUP_PREFIX overrides the
# directory; DOLLUP_MIRROR points at a different mirror; DOLLUP_SOURCE
# points somewhere else entirely -- including a file:// directory laid out
# like the mirror, which is the air-gapped install.
set -eu

MIRROR="${DOLLUP_MIRROR:-https://software.aloecraft.org/releases/dollup}"
GITHUB="https://github.com/Aloecraft-org/dollup/releases"
VERSION="${DOLLUP_VERSION:-latest}"

# The aligned name (doc/ALIGNMENT.md §4: os, arch, libc where it matters)
# and the name every release up to v0.0.1 used -- they differ on Linux
# only. Which one a release carries is read off its SHA256SUMS.txt, never
# guessed from a version.
case "$(uname -s)" in
  Linux)  NEW_OS=linux; OLD_OS=linux_static; LIBC=_musl ;;
  Darwin) NEW_OS=darwin; OLD_OS=darwin; LIBC= ;;
  *) echo "install.sh: $(uname -s) has no prebuilt dollup yet; cargo build --release -p dollup" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64|amd64)  NEW_ARCH=x86_64; OLD_ARCH=x86_64 ;;
  arm64|aarch64) NEW_ARCH=arm64; OLD_ARCH=arm64 ;;
  *) echo "install.sh: $(uname -m) has no prebuilt dollup yet" >&2; exit 1 ;;
esac
NEW_ASSET="dollup_${NEW_OS}_${NEW_ARCH}${LIBC}"
OLD_ASSET="dollup_${OLD_OS}_${OLD_ARCH}"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

fetch() { curl -fsSL "$1" -o "$2" 2>/dev/null; }
sha256_of() { (sha256sum "$1" 2>/dev/null || shasum -a 256 "$1") | cut -d' ' -f1; }

# Which base has this release: an explicit source, else the origin, else
# the mirror. Decided on the sums file, which every release since the
# sums-publishing workflow carries; a release without one is asked for the
# asset by name instead, and installed unverified, saying so.
BASE=""
if [ -n "${DOLLUP_SOURCE:-}" ]; then
  CANDIDATES="${DOLLUP_SOURCE%/}"
elif [ "$VERSION" = latest ]; then
  CANDIDATES="$GITHUB/latest/download $MIRROR/latest"
else
  CANDIDATES="$GITHUB/download/$VERSION $MIRROR/$VERSION"
fi
for cand in $CANDIDATES; do
  if fetch "$cand/SHA256SUMS.txt" "$TMP/sums"; then BASE="$cand"; break; fi
done
if [ -z "$BASE" ]; then
  for cand in $CANDIDATES; do
    for asset in "$NEW_ASSET" "$OLD_ASSET"; do
      if fetch "$cand/$asset" "$TMP/dollup"; then BASE="$cand"; ASSET="$asset"; break 2; fi
    done
  done
  [ -n "$BASE" ] || {
    echo "install.sh: no dollup for this platform at any of: $CANDIDATES" >&2
    echo "  the published assets are listed at $GITHUB" >&2
    exit 1
  }
  echo "install.sh: $BASE has no SHA256SUMS.txt; skipping verification" >&2
  CHECKED="unverified (no SHA256SUMS.txt at the source)"
else
  ASSET=""
  for asset in "$NEW_ASSET" "$OLD_ASSET"; do
    if grep -q " $asset\$" "$TMP/sums"; then ASSET="$asset"; break; fi
  done
  [ -n "$ASSET" ] || {
    echo "install.sh: $BASE/SHA256SUMS.txt lists neither $NEW_ASSET nor $OLD_ASSET" >&2
    echo "  this platform has no prebuilt dollup in that release; cargo build --release -p dollup" >&2
    exit 1
  }
  fetch "$BASE/$ASSET" "$TMP/dollup" || {
    echo "install.sh: $BASE/SHA256SUMS.txt lists $ASSET but it is not there" >&2
    exit 1
  }
  # A mismatch always refuses.
  WANT=$(grep " $ASSET\$" "$TMP/sums" | cut -d' ' -f1)
  HAVE=$(sha256_of "$TMP/dollup")
  if [ "$WANT" != "$HAVE" ]; then
    echo "install.sh: checksum mismatch for $ASSET" >&2
    echo "  expected $WANT" >&2
    echo "  got      $HAVE" >&2
    exit 1
  fi
  CHECKED="sha256 ok"
fi

chmod +x "$TMP/dollup"
"$TMP/dollup" --version >/dev/null 2>&1 || {
  echo "install.sh: the downloaded binary does not run here ($ASSET from $BASE)" >&2
  exit 1
}

DEST="${DOLLUP_PREFIX:-}"
if [ -z "$DEST" ]; then
  if [ -w /usr/local/bin ]; then DEST=/usr/local/bin; else DEST="$HOME/.local/bin"; fi
fi
mkdir -p "$DEST"
mv "$TMP/dollup" "$DEST/dollup"

echo "installed $("$DEST/dollup" --version) to $DEST/dollup"
echo "  source:  $BASE/$ASSET"
echo "  checked: $CHECKED"
case ":$PATH:" in
  *":$DEST:"*) echo "  next:    dollup get drt" ;;
  *) echo "  note:    $DEST is not on your PATH — run it as $DEST/dollup" ;;
esac
