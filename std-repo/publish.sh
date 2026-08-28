#!/usr/bin/env bash
# Publish the standard repo and the landing page.
#
#   ./std-repo/publish.sh --key-file ~/.dollup/std-repo.key            # build only
#   ./std-repo/publish.sh --key-file ~/.dollup/std-repo.key --deploy   # and rsync
#
# Build is local and offline: index, sign, project blobs, assemble the site
# into a staging tree. Deploy is one rsync of that tree. Nothing here needs
# to run on the server.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
src="$repo_root/std-repo"
web="$repo_root/web"
stage="${DOLLUP_STAGE:-$repo_root/.publish}"
target="${DOLLUP_TARGET:-cloud1:/var/www/dollup/}"
dollup="${DOLLUP_BIN:-$repo_root/target/release/dollup}"
key_file="${DOLLUP_KEY_FILE:-}"
deploy=0

while [ $# -gt 0 ]; do
    case "$1" in
        --key-file) key_file="$2"; shift 2 ;;
        --deploy)   deploy=1; shift ;;
        --target)   target="$2"; shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done

[ -n "$key_file" ] || { echo "need --key-file (or DOLLUP_KEY_FILE)" >&2; exit 2; }
[ -r "$key_file" ] || {
    echo "cannot read key file: $key_file" >&2
    echo "  mint one:  $dollup repo keygen --out $key_file" >&2
    exit 2
}
# keygen --out writes `<prefix without extension>.pub`; a prefix with no
# extension gets `<prefix>.pub`. Accept either, and say which is missing.
pub_file="${key_file%.*}.pub"
[ -r "$pub_file" ] || pub_file="$key_file.pub"
[ -r "$pub_file" ] || {
    echo "cannot find the public key beside $key_file" >&2
    echo "  looked for: ${key_file%.*}.pub and $key_file.pub" >&2
    exit 2
}
pubkey="$(tr -d '\n' < "$pub_file")"

[ -x "$dollup" ] || { echo "no dollup binary at $dollup (cargo build --release)" >&2; exit 2; }

echo "==> sealing packages"
# Sealing is idempotent; running it here means a hand-edited file can never
# ship with a stale hash. `index` re-hashes independently regardless.
for pkg in "$src"/packages/*/*/; do
    [ -f "$pkg/manifest.json" ] || continue
    "$dollup" repo seal "$pkg" | head -1
done

echo "==> indexing"
"$dollup" repo index "$src"

echo "==> signing"
"$dollup" repo sign "$src" --key-file "$key_file"

echo "==> projecting blobs"
# The tree is canonical and blobs/ is a projection of it, so rebuild rather
# than accumulate: a blob left over from a deleted package would be served
# forever under a name nothing indexes.
rm -rf "$src/blobs"
"$dollup" repo blobs "$src"

echo "==> staging into $stage"
rm -rf "$stage"
mkdir -p "$stage/std-repo"
cp -r "$web"/. "$stage"/
rm -f "$stage/README.md"
for item in index.json index.json.sig packages blobs; do
    [ -e "$src/$item" ] && cp -r "$src/$item" "$stage/std-repo/"
done

# Stamp the real public key into the page, so what a reader pins and what
# the repo is signed with cannot drift apart.
if grep -rq "__DOLLUP_STD_PUBKEY__" "$stage"; then
    grep -rl "__DOLLUP_STD_PUBKEY__" "$stage" | while read -r f; do
        sed -i.bak "s|__DOLLUP_STD_PUBKEY__|$pubkey|g" "$f" && rm -f "$f.bak"
    done
    echo "    stamped $pubkey"
fi

echo "==> verifying the staged repo resolves"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
(
    cd "$tmp"
    "$dollup" init >/dev/null
    "$dollup" source add "file://$stage/std-repo" --key "$pubkey" >/dev/null
    # Every package in the tree, not a chosen one: the point is that the
    # published repo resolves, all of it.
    for pkg in "$src"/packages/*/; do
        "$dollup" add "$(basename "$pkg")" >/dev/null
    done
    "$dollup" verify
)

if [ "$deploy" = 1 ]; then
    echo "==> deploying to $target"
    rsync -avz --delete "$stage"/ "$target"
else
    echo "built in $stage — pass --deploy to rsync to $target"
fi
