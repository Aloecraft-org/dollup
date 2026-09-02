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
# The public key is derived from the private one rather than hunted for
# beside it -- `${key%.*}.pub` then `$key.pub` then an error naming both was
# guesswork about a filename standing in for a fact the key itself carries,
# and the two could drift.
pubkey="$("$dollup" repo pubkey --key-file "$key_file")"

echo "==> publishing (seal, index, sign, blobs, and resolve the result)"
# One verb, because this sequence is the same in every repo that publishes:
# `dollup repo publish` seals every package, indexes, signs, projects blobs,
# and then RESOLVES the tree it produced in a throwaway deployment before
# calling it publishable. What used to be forty lines here, including the
# hunt for a .pub file beside the key -- the public half is derived from the
# private one now, so the two cannot drift.
"$dollup" repo publish "$src" --key-file "$key_file" --stage "$stage/std-repo"

echo "==> staging the landing page beside it"
# `repo publish --stage` wrote the repo itself; the site is this repo's own
# addition and is not part of the format.
cp -r "$web"/. "$stage"/
rm -f "$stage/README.md"

# Stamp the real public key into the page, so what a reader pins and what
# the repo is signed with cannot drift apart.
if grep -rq "__DOLLUP_STD_PUBKEY__" "$stage"; then
    grep -rl "__DOLLUP_STD_PUBKEY__" "$stage" | while read -r f; do
        sed -i.bak "s|__DOLLUP_STD_PUBKEY__|$pubkey|g" "$f" && rm -f "$f.bak"
    done
    echo "    stamped $pubkey"
fi

if [ "$deploy" = 1 ]; then
    echo "==> deploying to $target"
    rsync -avz --delete "$stage"/ "$target"
else
    echo "built in $stage — pass --deploy to rsync to $target"
fi
