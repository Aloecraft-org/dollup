# Operator

dollup is the distribution tool for Diluvium/DRT artifacts: a Rust
workspace (`crates/dollup`, the CLI, and `crates/dollup-format`, the
artifact hashing and signature formats) that fills a DRT root with
packages from content-addressed repos and puts the `drt` runtime in it.
It ships as static binaries on the GitHub Releases page and the
software.aloecraft.org mirror, plus the dollup.aloecraft.org site built
from `site/`. The founding spec is `SPEC.md`; the repo format is
`doc/RepoFormat.md`; `THREAT-NOTES.md` only grows deliberately. The owner
is @aloecraft.

This repository is public. Everything under `.claude/` and `doc/lockstep/`
is readable by anyone: write nothing there that should not be.

Every run needs:

- Build and test, the same gates as `.github/workflows/ci.yml`:
  `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test`, `python3 ci/check-strings.py`,
  `./site/build.sh --out /tmp/site-out`.
- Release tooling is technoproj, pinned at the tag CI and `release.yml`
  pin (`pip install "git+https://github.com/Aloecraft-org/technoproj@v0.3.0"`):
  `technoproj check`, `make changelog-check`,
  `technoproj release check-workflow`, `technoproj release preflight`.
  A release is started with `technoproj release cut`, never by pushing
  a tag; nightly.yml cuts the `-dev.N` builds.
- The facts source is `CHANGELOG.yaml`: release notes, the version, and
  the compatibility facts (`dollup_repo`, `drt_config`) come from it.
  `script/checks.py` holds dollup's own invariant, the drt-config
  revision in `Cargo.lock` against the newest entry's.

The operating protocol is .claude/rules/operating.md. It and every
other file in .claude/rules/ apply to every run.
