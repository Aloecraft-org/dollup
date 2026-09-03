#!/usr/bin/env bash
# Sign and stage the standard repo. The publisher's tool, not the deployer's.
#
#   ./std-repo/publish.sh --key-file ~/.dollup/std-repo.key
#
# What it does, in order:
#   1. `dollup repo publish` seals every package, indexes, signs, projects
#      blobs, and resolves the result in a throwaway app before calling it
#      publishable -- all IN PLACE in std-repo/.
#   2. Writes site/std-repo.pub, derived from the private key, so the key
#      the landing page shows and the key that signed the index cannot drift.
#   3. Builds the site into .publish/ as a preview, by the same path the
#      deployment tooling uses.
#
# Then you COMMIT: std-repo/index.json, std-repo/index.json.sig and
# site/std-repo.pub. Deploying is not done here -- the site contract
# (site/build.sh) forbids it, and the tooling that stages every Aloecraft
# site picks up the committed tree from the repo. The private key is the
# only thing this script needs that the build does not, which is the point.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
src="$repo_root/std-repo"
stage="${DOLLUP_STAGE:-$repo_root/.publish}"
dollup="${DOLLUP_BIN:-$repo_root/target/release/dollup}"
key_file="${DOLLUP_KEY_FILE:-}"

while [ $# -gt 0 ]; do
    case "$1" in
        --key-file) key_file="$2"; shift 2 ;;
        --deploy|--target)
            echo "publish.sh no longer deploys: the site contract owns that. Commit the" >&2
            echo "signed index and let the deployment tooling stage it." >&2
            exit 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done

[ -n "$key_file" ] || { echo "need --key-file (or DOLLUP_KEY_FILE)" >&2; exit 2; }
[ -r "$key_file" ] || {
    echo "cannot read key file: $key_file" >&2
    echo "  mint one:  $dollup repo keygen --out $key_file" >&2
    exit 2
}
[ -x "$dollup" ] || { echo "no dollup binary at $dollup (cargo build --release)" >&2; exit 2; }

echo "==> publishing in place (seal, index, sign, blobs, and resolve the result)"
"$dollup" repo publish "$src" --key-file "$key_file"

echo "==> recording the public key beside the site"
# Derived from the private half rather than hunted for beside it: what the
# page shows is what signed the index, by construction.
"$dollup" repo pubkey --key-file "$key_file" > "$repo_root/site/std-repo.pub"
cat "$repo_root/site/std-repo.pub"

echo "==> verifying the committed pair"
"$dollup" repo verify "$src" --key-file "$repo_root/site/std-repo.pub"

echo "==> building the site as a preview"
"$repo_root/site/build.sh" --out "$stage"

cat <<EOF

Now commit the signed repo and the public key:

  git add std-repo/index.json std-repo/index.json.sig site/std-repo.pub std-repo/packages
  git commit -m "Publish the standard repo"

The deployment tooling stages the committed tree; nothing is pushed from here.
EOF
