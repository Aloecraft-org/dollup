#!/bin/sh
# Install dollup.
#
#   curl -fsSL https://github.com/Aloecraft-org/dollup/releases/latest/download/install.sh | sh
#
# One file, verified against the SHA256SUMS.txt published beside it, into a
# directory you already own. It installs nothing else and touches nothing
# outside $DOLLUP_PREFIX.
#
# Knobs: DOLLUP_VERSION=vX.Y.Z pins a release; DOLLUP_PREFIX overrides the
# directory; DOLLUP_SOURCE points somewhere else entirely — including a
# file:// directory, which is the air-gapped install.
set -eu

GITHUB="https://github.com/Aloecraft-org/dollup/releases"
VERSION="${DOLLUP_VERSION:-latest}"

case "$(uname -s)" in
  Linux)  OS=linux_static ;;
  Darwin) OS=darwin ;;
  *) echo "install.sh: $(uname -s) has no prebuilt dollup yet; cargo build --release -p dollup" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64|amd64)  ARCH=x86_64 ;;
  arm64|aarch64) ARCH=arm64 ;;
  *) echo "install.sh: $(uname -m) has no prebuilt dollup yet" >&2; exit 1 ;;
esac
# Linux ships x86_64 only today. Refuse by name rather than handing over a
# binary that cannot exec and failing the --version check with "does not run
# here", which is true and no help.
if [ "$OS" = linux_static ] && [ "$ARCH" != x86_64 ]; then
  echo "install.sh: linux $ARCH has no prebuilt dollup yet — only x86_64." >&2
  echo "  build it: cargo build --release -p dollup" >&2
  exit 1
fi

ASSET="dollup_${OS}_${ARCH}"
if [ -n "${DOLLUP_SOURCE:-}" ]; then
  BASE="${DOLLUP_SOURCE%/}"
elif [ "$VERSION" = latest ]; then
  BASE="$GITHUB/latest/download"
else
  BASE="$GITHUB/download/$VERSION"
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

fetch() { curl -fsSL "$1" -o "$2" 2>/dev/null; }

fetch "$BASE/$ASSET" "$TMP/dollup" || {
  echo "install.sh: no $ASSET at $BASE" >&2
  echo "  the published assets are listed at $GITHUB" >&2
  exit 1
}

# A mismatch always refuses. A missing sums file warns rather than refusing,
# so pinning a release older than the sums-publishing workflow still works.
if fetch "$BASE/SHA256SUMS.txt" "$TMP/sums"; then
  WANT=$(grep " $ASSET\$" "$TMP/sums" | cut -d' ' -f1)
  HAVE=$( (sha256sum "$TMP/dollup" 2>/dev/null || shasum -a 256 "$TMP/dollup") | cut -d' ' -f1)
  if [ -z "$WANT" ]; then
    echo "install.sh: SHA256SUMS.txt does not list $ASSET; skipping verification" >&2
    CHECKED="unverified (asset not listed)"
  elif [ "$WANT" != "$HAVE" ]; then
    echo "install.sh: checksum mismatch for $ASSET" >&2
    echo "  expected $WANT" >&2
    echo "  got      $HAVE" >&2
    exit 1
  else
    CHECKED="sha256 ok"
  fi
else
  echo "install.sh: $BASE has no SHA256SUMS.txt; skipping verification" >&2
  CHECKED="unverified (no SHA256SUMS.txt at the source)"
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
