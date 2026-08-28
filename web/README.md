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

## Invariants, and getting a design pass done elsewhere

The page carries details a designer has no reason to know about: the
`__DOLLUP_STD_PUBKEY__` token `publish.sh` substitutes, the `id="pkgs"`
element the live package table fills, the fetch that fills it. Losing one
does not look broken — the page renders fine and quietly stops working — so
they are checked mechanically rather than by eye:

```sh
python3 web/check.py                  # checks web/index.html; CI runs this
python3 web/check.py candidate.html   # check a returned file before adopting
```

To hand the page to another tool for an aesthetic pass, give it
[`DESIGN-BRIEF.md`](DESIGN-BRIEF.md) followed by `index.html`. The brief says
what must survive and what is free, and asks for one file back rather than a
project. When the file comes back:

```sh
python3 web/check.py candidate.html && cp candidate.html web/index.html
git diff web/index.html          # read the design change on its own
```

Because the deliverable is the same single file, this is a diff rather than a
transcription — nothing has to be read across and retyped, and `check.py`
catches the load-bearing details a redesign tends to drop.
