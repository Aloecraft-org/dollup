# Design brief — dollup landing page

Paste this, then paste `site/template/index.html` after it.

---

## The job

Do an **aesthetic pass** on one static HTML page. I will paste the complete
file below; return the complete file back.

**This is not an application.** Do not create a project, add a build step,
install dependencies, initialise a framework, or split it into multiple
files. There is one file, it has one `<style>` block and one small `<script>`
block, and it should still be one file when you are done. If you would
normally scaffold something first — skip that entirely and edit the file.

## What the page is

The landing page for **dollup**, a tool that installs
[Diluvium](https://github.com/Aloecraft-org/diluvium) programs into a
directory and verifies them against a signed index. The audience is
developers who want to install something, plus a smaller group deciding
whether to trust the distribution channel at all.

Two moments on the page carry the most weight and are worth designing
deliberately: the **install commands** (what most visitors came for) and the
**signing key panel** (the trust anchor — people copy it into a config file).
Everything else supports those.

The feel to aim for: precise, calm, and legible — the aesthetic of
infrastructure people rely on rather than a product being sold. Not a
startup splash, no gradient hero, no stock illustration.

## Hard constraints

A change that breaks any of these cannot be used, so please treat them as
fixed:

1. **No third-party requests.** No Google Fonts, no CDN stylesheets or
   scripts, no remote images, no analytics. Every byte is served from our own
   origin — this is the page that hands out a signing key, so it does not
   load code from anyone else. Use system font stacks, or embed a font as a
   `data:` URI if you need one.
2. **Keep `__DOLLUP_STD_PUBKEY__` exactly as-is, in both places it appears.**
   It is a token our publish script substitutes with the real key. Do not
   replace it with example text or a shortened version.
3. **Keep the element with `id="pkgs"` and the script that fetches
   `/std-repo/index.json`.** That table is populated live from the published
   index. Restyle the table however you like; it just has to still fill in.
4. **Keep a `<link rel="icon">`.** Redesigning the icon is welcome — it is an
   inline SVG `data:` URI, so keep that form.
5. **Keep dark and light.** The page adapts via
   `@media (prefers-color-scheme: dark)`. Both must look deliberate; neither
   is the afterthought.
6. **Keep the words.** The copy was written carefully and is not part of this
   pass. Re-typeset it, change emphasis, change heading levels, reorder
   sections if the flow is better — but do not rewrite sentences or invent
   new claims about what the software does.

## Free rein

Everything visual: type scale and pairing, spacing and rhythm, colour, the
accent, borders and dividers, the code block and key panel treatment, section
structure, the wordmark, the favicon, and any decorative SVG you want to
inline. Responsive behaviour is yours too — it should work from a phone to a
wide desktop.

If you want to add a graphic element, inline SVG is the way (no external
assets). Restraint is welcome; so is one strong idea.

## Deliverable

The complete HTML file, in a single code block, ready to save over the
original. No commentary needed beyond a couple of sentences on what you
changed and why.

If you would rather not touch the markup, returning **just the replacement
`<style>` block** is also fine — say so if that is what you are giving me.
