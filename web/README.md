# web/

The landing page for `dollup.aloecraft.org`. One file, no build step, no
outbound requests: system fonts, inline styles, and a little JavaScript that
reads `/std-repo/index.json` — the same index dollup reads — so the published
package list cannot drift from what is actually published.

`__DOLLUP_STD_PUBKEY__` is substituted with the real signing key by
[`../std-repo/publish.sh`](../std-repo/publish.sh) at publish time, so what a
reader pins and what the repo is signed with are the same string by
construction. Viewed straight from the repo the token shows through, which is
the intended tell that this copy is unpublished.

Deployed by `publish.sh`, which assembles this directory and the built repo
into one tree and rsyncs it. See [`../std-repo/deploy/nginx.conf`](../std-repo/deploy/nginx.conf).
