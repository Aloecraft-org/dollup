import pathlib

MARK = '''<svg width="{w}" height="{w}" viewBox="0 0 100 100" aria-hidden="true" style="display:block;flex-shrink:0">
<circle cx="50" cy="53" r="47" fill="#8FB8EC"/>
<circle cx="50" cy="55" r="39" fill="#2A63D6"/>
<path d="M50 14C56 26 76 44 76 58A26 26 0 1 1 24 58C24 44 44 26 50 14Z" fill="#0D2B5E" transform="translate(50 14) scale(1.06) translate(-50 -14)"/>
<path d="M50 14C56 26 76 44 76 58A26 26 0 1 1 24 58C24 44 44 26 50 14Z" fill="#FFFFFF" transform="translate(50 14) scale(0.87) translate(-50 -14)"/>
<path d="M50 14C56 26 76 44 76 58A26 26 0 1 1 24 58C24 44 44 26 50 14Z" fill="#00E026" transform="translate(50 14) scale(0.83) translate(-50 -14)"/>
<path d="M50 14C56 26 76 44 76 58A26 26 0 1 1 24 58C24 44 44 26 50 14Z" fill="#FFFFFF" transform="translate(50 14) scale(0.67) translate(-50 -14)"/>
<path d="M50 14C56 26 76 44 76 58A26 26 0 1 1 24 58C24 44 44 26 50 14Z" fill="#C6DFB4" transform="translate(50 14) scale(0.63) translate(-50 -14)"/>
<path d="M62 46A17 17 0 0 1 40 70A19 19 0 0 0 62 46Z" fill="#FFFFFF" opacity="0.8"/>
</svg>'''

def mark(w): return MARK.format(w=w)

# Decorative ripple field — the drop's rings, bleeding off an edge.
def ripples(size, color, op):
    rings = "".join(
        f'<circle cx="{size/2}" cy="{size/2}" r="{r}" fill="none" stroke="{color}" '
        f'stroke-width="{1.4 if i%2 else 2.2}" opacity="{op*(1-i*0.09):.3f}"/>'
        for i, r in enumerate(range(46, int(size/2), 34)))
    return (f'<svg width="{size}" height="{size}" viewBox="0 0 {size} {size}" aria-hidden="true" '
            f'style="display:block">{rings}</svg>')

SANS = "ui-sans-serif, system-ui, -apple-system, 'Segoe UI', Helvetica, Arial, sans-serif"
MONO = "ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, 'Liberation Mono', monospace"
KEY = 'ed25519:3yauIanxJZhPJkSisaonksYmeU2TsCRqmTRel8OrW2U='

ICONS = {
"deployment": '''<svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="{c}" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M3 7.5A1.5 1.5 0 0 1 4.5 6h4l1.6 2h9.4A1.5 1.5 0 0 1 21 9.5v8A1.5 1.5 0 0 1 19.5 19h-15A1.5 1.5 0 0 1 3 17.5z"/><path d="M7 12.5h10M7 15.5h6"/></svg>''',
"verified": '''<svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="{c}" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="4.6" opacity="0.45"/><path d="M9.4 12.2l1.9 1.9 3.4-3.9"/></svg>''',
"inert": '''<svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="{c}" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="4" width="16" height="16" rx="3"/><path d="M8.5 9h7M8.5 12h7M8.5 15h4"/></svg>''',
}
def icon(k, c): return ICONS[k].format(c=c)

CARDS = [
    ("deployment", "A deployment",
     "One directory holding your programs, the sources they came from, and a lockfile pinning every version and hash. Copy it to another machine and you get the same thing, byte for byte."),
    ("verified", "Verified on the way in",
     "Packages are named by the hash of their contents and listed in an index the publisher signs. The same package from this mirror, a git remote, or a zipball is a single artifact."),
    ("inert", "Nothing executes on install",
     "A manifest is data and can express no behaviour. What a program is allowed to do stays in your config — installing something never grants it anything."),
]

PKGS = [("hostcall", "The token-matched hostcall discipline, written once", "0.1.0"),
        ("hello", "What a deployment looks like from inside a program", "0.1.0")]

FOOT = [("Source", "#"), ("Spec", "#"), ("Repo format", "#"), ("Threat notes", "#")]

def shell(body, bg, css=""):
    return f'''<!doctype html>
<html>
<head>
<meta charset="utf-8">
<script src="./support.js"></script>
</head>
<body>
<x-dc>
<helmet>
<style>
body {{ margin:0; background:{bg}; font-family:{SANS}; -webkit-font-smoothing:antialiased; }}
a {{ color:#2A63D6; text-decoration:none; }}
a:hover {{ color:#0D2B5E; text-decoration:underline; }}
* {{ box-sizing:border-box; }}
{css}
</style>
</helmet>
{body}
</x-dc>
</body>
</html>
'''

def commands(fg, dim, line, surface, prompt, size=15):
    rows = [
        ('dollup init', None),
        ('dollup source add https://dollup.aloecraft.org/std-repo/ \\', '--key ' + KEY),
        ('dollup add hello', None),
    ]
    out = []
    for cmd, cont in rows:
        out.append(f'''<div style="display:flex;gap:14px;align-items:baseline">
<span style="color:{prompt};font-weight:700;user-select:none">$</span>
<span style="color:{fg};word-break:break-all">{cmd}</span></div>''')
        if cont:
            out.append(f'''<div style="padding-left:30px;color:{dim};word-break:break-all">{cont}</div>''')
    return f'''<div style="display:flex;flex-direction:column;gap:11px;font-family:{MONO};font-size:{size}px;line-height:1.6">
{"".join(out)}
</div>'''

# ── Main: dark hero, the command deck straddling the fold ────────────────
NAVY, INK, DIM, LINE, BG, SURF = "#0A2450", "#0C1524", "#5A6B85", "#DCE4F0", "#F5F8FC", "#FFFFFF"
BLUE, GREEN, PALE = "#2A63D6", "#00C41F", "#9FC0EE"

def main_page(w=1440, pad=64, mobile=False):
    inner = min(w - pad*2, 1080)
    hero_mark = 76 if not mobile else 56
    wordmark = 66 if not mobile else 44
    cards = f"repeat({1 if mobile else 3}, minmax(0,1fr))"
    return f'''
<div style="width:100%;background:{BG}">

  <div style="position:relative;overflow:hidden;background:{NAVY};padding:{"56px" if mobile else "72px"} {pad}px {"128px" if mobile else "150px"}">
    <div style="position:absolute;top:{-160 if mobile else -240}px;right:{-260 if mobile else -200}px;pointer-events:none">
      {ripples(760, "#4E82E8", 0.30)}
    </div>
    <div style="position:relative;max-width:{inner}px;margin:0 auto;display:flex;flex-direction:column;gap:{"22px" if mobile else "26px"}">
      <div style="display:flex;align-items:center;gap:{"18px" if mobile else "22px"}">
        {mark(hero_mark)}
        <span style="font-family:{MONO};font-size:{wordmark}px;font-weight:600;letter-spacing:-0.03em;color:#FFFFFF;line-height:1">dollup</span>
      </div>
      <p style="margin:0;font-size:{"21px" if mobile else "27px"};line-height:1.4;color:{PALE};max-width:640px;text-wrap:pretty">
        Install Diluvium programs and the capabilities they run on.</p>
      <div style="display:flex;gap:10px;align-items:center">
        <span style="display:inline-flex;align-items:center;gap:9px;padding:7px 14px;border:1px solid #2A4B82;border-radius:999px;font-family:{MONO};font-size:12.5px;color:{PALE}">
          <span style="width:7px;height:7px;border-radius:50%;background:#00E026;box-shadow:0 0 8px #00E026"></span>
          pre-release · the format is settling</span>
      </div>
    </div>
  </div>

  <div style="position:relative;z-index:1;max-width:{inner}px;margin:0 auto;padding:0 {pad}px">
    <div style="margin-top:{-80 if mobile else -92}px;background:{SURF};border:1px solid {LINE};border-radius:14px;
                box-shadow:0 18px 44px rgba(10,36,80,0.14);padding:{"26px" if mobile else "34px"};display:flex;flex-direction:column;gap:22px">
      <div style="display:flex;align-items:center;justify-content:space-between;gap:16px">
        <span style="font-family:{MONO};font-size:11.5px;letter-spacing:0.13em;text-transform:uppercase;color:{DIM};font-weight:600">Get started</span>
        <span style="font-family:{MONO};font-size:11.5px;color:{DIM}">3 commands</span>
      </div>
      {commands(INK, DIM, LINE, SURF, GREEN, size=12.5 if mobile else 15)}
      <div style="height:1px;background:{LINE}"></div>
      <p style="margin:0;font-size:15.5px;line-height:1.65;color:{DIM};max-width:720px;text-wrap:pretty">
        You now have the program, everything it depends on, and a lockfile recording exactly which bytes
        arrived. Point <a href="#">DRT</a> at the directory to run it.</p>
    </div>
  </div>

  <div style="max-width:{inner}px;margin:0 auto;padding:{"64px" if mobile else "86px"} {pad}px 0">
    <div style="display:grid;grid-template-columns:{cards};gap:{"20px" if mobile else "26px"}">
      {"".join(f"""
      <div style="background:{SURF};border:1px solid {LINE};border-radius:12px;padding:26px;display:flex;flex-direction:column;gap:14px">
        <span style="display:inline-flex;align-items:center;justify-content:center;width:46px;height:46px;border-radius:11px;background:#EDF3FD">{icon(k, BLUE)}</span>
        <h3 style="margin:0;font-size:17.5px;font-weight:650;color:{INK};letter-spacing:-0.01em">{t}</h3>
        <p style="margin:0;font-size:14.5px;line-height:1.62;color:{DIM};text-wrap:pretty">{b}</p>
      </div>""" for k, t, b in CARDS)}
    </div>
  </div>

  <div style="max-width:{inner}px;margin:0 auto;padding:{"56px" if mobile else "74px"} {pad}px 0">
    <div style="position:relative;overflow:hidden;background:{NAVY};border-radius:14px;padding:{"28px" if mobile else "36px"};display:flex;flex-direction:column;gap:18px">
      <div style="position:absolute;bottom:-330px;right:-190px;pointer-events:none">{ripples(560, "#4E82E8", 0.22)}</div>
      <div style="position:relative;display:flex;flex-direction:column;gap:18px">
        <div style="display:flex;align-items:center;gap:11px">
          <span style="width:7px;height:7px;border-radius:50%;background:#00E026;box-shadow:0 0 8px #00E026"></span>
          <span style="font-family:{MONO};font-size:11.5px;letter-spacing:0.13em;text-transform:uppercase;color:{PALE};font-weight:600">Verifying this repo</span>
        </div>
        <div style="background:#061B3C;border:1px solid #22437A;border-left:3px solid #00E026;border-radius:9px;padding:16px 18px;
                    font-family:{MONO};font-size:{"12.5px" if mobile else "14.5px"};color:#CFE0F7;word-break:break-all;line-height:1.6">{KEY}</div>
        <p style="margin:0;font-size:14.5px;line-height:1.65;color:{PALE};max-width:660px;text-wrap:pretty">
          A verified index means a holder of this key signed exactly these bytes — authenticity, not freshness.</p>
      </div>
    </div>
  </div>

  <div style="max-width:{inner}px;margin:0 auto;padding:{"56px" if mobile else "74px"} {pad}px 0">
    <span style="font-family:{MONO};font-size:11.5px;letter-spacing:0.13em;text-transform:uppercase;color:{DIM};font-weight:600">Published packages</span>
    <div style="margin-top:18px;background:{SURF};border:1px solid {LINE};border-radius:12px;overflow:hidden">
      {"".join(f"""
      <div style="display:grid;grid-template-columns:{'1fr' if mobile else '190px 1fr 90px'};gap:{'6px' if mobile else '20px'};
                  padding:{'16px 20px' if mobile else '18px 24px'};{'' if i==0 else f'border-top:1px solid {LINE};'}align-items:baseline">
        <span style="font-family:{MONO};font-size:14.5px;color:{INK};font-weight:600">{n}</span>
        <span style="font-size:14px;color:{DIM};text-wrap:pretty">{d}</span>
        <span style="font-family:{MONO};font-size:13px;color:{DIM}">{v}</span>
      </div>""" for i, (n, d, v) in enumerate(PKGS))}
    </div>
  </div>

  <div style="max-width:{inner}px;margin:0 auto;padding:{"52px" if mobile else "70px"} {pad}px {"52px" if mobile else "64px"}">
    <div style="height:1px;background:{LINE}"></div>
    <div style="padding-top:26px;display:flex;flex-wrap:wrap;gap:{"16px" if mobile else "28px"};align-items:center">
      {"".join(f'<a href="{h}" style="font-size:14px">{t}</a>' for t, h in FOOT)}
      <span style="flex-grow:1"></span>
      <span style="font-size:13.5px;color:{DIM}">Apache-2.0 · Aloecraft</span>
    </div>
  </div>

</div>'''

pathlib.Path("Main.dc.html").write_text(shell(main_page(), BG))
pathlib.Path("Mobile.dc.html").write_text(shell(main_page(w=390, pad=22, mobile=True), BG))
print("Main + Mobile written")

# ── MainDark: the same direction, dark end of the theme ──────────────────
NAVY, INK, DIM, LINE, BG, SURF = "#04101F", "#E9F0FA", "#93A9C8", "#1D3A63", "#071429", "#0E2140"
BLUE, PALE = "#6FA0F0", "#9FC0EE"
dark = main_page()
dark = dark.replace("background:#EDF3FD", "background:#12294B").replace(
    "box-shadow:0 18px 44px rgba(10,36,80,0.14)", "box-shadow:0 18px 44px rgba(0,0,0,0.45)").replace(
    "background:#061B3C", "background:#020A16")
pathlib.Path("MainDark.dc.html").write_text(shell(dark, BG,
    css="a{color:#6FA0F0}a:hover{color:#9FC0EE}"))

# ── Direction B: split hero, the install as three numbered steps ─────────
B_BG, B_SURF, B_INK, B_DIM, B_LINE = "#FBFCFE", "#FFFFFF", "#0C1524", "#5A6B85", "#E2E9F4"
STEPS = [("dollup init", "A deployment is a directory you own."),
         ("dollup source add …/std-repo/ --key …", "Pin the publisher whose signature must check out."),
         ("dollup add hello", "Fetch, verify, lock, materialise.")]
b = f'''
<div style="width:100%;background:{B_BG}">
  <div style="max-width:1120px;margin:0 auto;padding:70px 64px 0">
    <div style="display:grid;grid-template-columns:1.15fr 0.85fr;gap:56px;align-items:center">
      <div style="display:flex;flex-direction:column;gap:24px">
        <span style="font-family:{MONO};font-size:60px;font-weight:600;letter-spacing:-0.035em;color:{B_INK};line-height:1">dollup</span>
        <p style="margin:0;font-size:26px;line-height:1.42;color:{B_DIM};text-wrap:pretty">
          Install Diluvium programs and the capabilities they run on.</p>
        <span style="display:inline-flex;align-self:flex-start;align-items:center;gap:9px;padding:7px 14px;border:1px solid {B_LINE};
                     border-radius:999px;font-family:{MONO};font-size:12.5px;color:{B_DIM}">
          <span style="width:7px;height:7px;border-radius:50%;background:#00C41F"></span>pre-release · the format is settling</span>
      </div>
      <div style="position:relative;display:flex;justify-content:center;align-items:center;height:300px">
        <div style="position:absolute;opacity:0.5">{ripples(420, "#8FB8EC", 0.9)}</div>
        <div style="position:relative">{mark(184)}</div>
      </div>
    </div>
  </div>

  <div style="max-width:1120px;margin:0 auto;padding:78px 64px 0">
    <span style="font-family:{MONO};font-size:11.5px;letter-spacing:0.13em;text-transform:uppercase;color:{B_DIM};font-weight:600">Get started</span>
    <div style="margin-top:24px;display:flex;flex-direction:column;gap:0">
      {"".join(f"""
      <div style="display:grid;grid-template-columns:56px 1fr;gap:22px;padding:22px 0;{'' if i==0 else f'border-top:1px solid {B_LINE};'}align-items:start">
        <span style="position:relative;display:inline-flex;align-items:center;justify-content:center;width:44px;height:44px">
          <span style="position:absolute;inset:0;border-radius:50%;border:1px solid #C9D9F2"></span>
          <span style="position:absolute;inset:7px;border-radius:50%;border:1px solid #8FB8EC"></span>
          <span style="position:relative;font-family:{MONO};font-size:14px;font-weight:700;color:{BLUE}">{i+1}</span>
        </span>
        <div style="display:flex;flex-direction:column;gap:8px">
          <code style="font-family:{MONO};font-size:16px;color:{B_INK};word-break:break-all">{c}</code>
          <span style="font-size:14.5px;color:{B_DIM}">{d}</span>
        </div>
      </div>""" for i, (c, d) in enumerate(STEPS))}
    </div>
    <p style="margin:22px 0 0;font-size:15.5px;line-height:1.65;color:{B_DIM};max-width:700px;text-wrap:pretty">
      You now have the program, everything it depends on, and a lockfile recording exactly which bytes arrived.
      Point <a href="#">DRT</a> at the directory to run it.</p>
  </div>

  <div style="max-width:1120px;margin:0 auto;padding:74px 64px 0">
    <div style="display:flex;flex-direction:column">
      {"".join(f"""
      <div style="display:grid;grid-template-columns:64px 250px 1fr;gap:26px;padding:26px 0;
                  {'' if i==0 else f'border-top:1px solid {B_LINE};'}align-items:start">
        <span style="display:inline-flex;align-items:center;justify-content:center;width:48px;height:48px;border-radius:50%;background:#EDF3FD">{icon(k, BLUE)}</span>
        <h3 style="margin:0;font-size:18px;font-weight:650;color:{B_INK};letter-spacing:-0.01em;line-height:1.35">{t}</h3>
        <p style="margin:0;font-size:15px;line-height:1.62;color:{B_DIM};text-wrap:pretty">{b}</p>
      </div>""" for i, (k, t, b) in enumerate(CARDS))}
    </div>
  </div>

  <div style="max-width:1120px;margin:0 auto;padding:70px 64px 0">
    <div style="background:{B_SURF};border:1px solid {B_LINE};border-left:4px solid #00C41F;border-radius:12px;padding:32px;display:flex;flex-direction:column;gap:16px">
      <span style="font-family:{MONO};font-size:11.5px;letter-spacing:0.13em;text-transform:uppercase;color:{B_DIM};font-weight:600">Verifying this repo</span>
      <div style="font-family:{MONO};font-size:15px;color:{B_INK};word-break:break-all;line-height:1.6">{KEY}</div>
      <p style="margin:0;font-size:14.5px;line-height:1.65;color:{B_DIM};max-width:680px;text-wrap:pretty">
        A verified index means a holder of this key signed exactly these bytes — authenticity, not freshness.</p>
    </div>
  </div>

  <div style="max-width:1120px;margin:0 auto;padding:70px 64px 0">
    <span style="font-family:{MONO};font-size:11.5px;letter-spacing:0.13em;text-transform:uppercase;color:{B_DIM};font-weight:600">Published packages</span>
    <div style="margin-top:16px">
      {"".join(f"""
      <div style="display:grid;grid-template-columns:190px 1fr 90px;gap:20px;padding:17px 0;border-top:1px solid {B_LINE};align-items:baseline">
        <span style="font-family:{MONO};font-size:14.5px;color:{B_INK};font-weight:600">{n}</span>
        <span style="font-size:14px;color:{B_DIM}">{d}</span>
        <span style="font-family:{MONO};font-size:13px;color:{B_DIM}">{v}</span>
      </div>""" for n, d, v in PKGS)}
    </div>
  </div>

  <div style="max-width:1120px;margin:0 auto;padding:66px 64px 60px">
    <div style="height:1px;background:{B_LINE}"></div>
    <div style="padding-top:26px;display:flex;flex-wrap:wrap;gap:28px;align-items:center">
      {"".join(f'<a href="{h}" style="font-size:14px">{t}</a>' for t, h in FOOT)}
      <span style="flex-grow:1"></span><span style="font-size:13.5px;color:{B_DIM}">Apache-2.0 · Aloecraft</span>
    </div>
  </div>
</div>'''
pathlib.Path("DirectionB.dc.html").write_text(shell(b, B_BG))
print("MainDark + DirectionB written")

# ── Direction C: one dark deck, terminal-forward, hairline rules ─────────
C_BG, C_SURF, C_INK, C_DIM, C_LINE = "#08152C", "#0C1E3B", "#EAF1FB", "#8FA6C6", "#1A3358"
c = f'''
<div style="width:100%;background:{C_BG}">
  <div style="border-bottom:1px solid {C_LINE}">
    <div style="max-width:1080px;margin:0 auto;padding:20px 56px;display:flex;align-items:center;gap:12px">
      {mark(28)}
      <span style="font-family:{MONO};font-size:16px;font-weight:600;color:{C_INK};letter-spacing:-0.01em">dollup</span>
      <span style="flex-grow:1"></span>
      <div style="display:flex;gap:24px">
        {"".join(f'<a href="{h}" style="font-size:13.5px;color:{C_DIM}">{t}</a>' for t, h in FOOT[:3])}
      </div>
    </div>
  </div>

  <div style="position:relative;overflow:hidden">
    <div style="position:absolute;top:-300px;left:50%;pointer-events:none">{ripples(900, "#2A63D6", 0.20)}</div>
    <div style="position:relative;max-width:1080px;margin:0 auto;padding:88px 56px 0;display:flex;flex-direction:column;gap:26px">
      <h1 style="margin:0;font-family:{MONO};font-size:74px;font-weight:600;letter-spacing:-0.04em;color:#FFFFFF;line-height:1">dollup</h1>
      <p style="margin:0;font-size:27px;line-height:1.38;color:{C_DIM};max-width:620px;text-wrap:pretty">
        Install Diluvium programs and the capabilities they run on.</p>
      <span style="display:inline-flex;align-self:flex-start;align-items:center;gap:9px;padding:7px 14px;border:1px solid {C_LINE};
                   border-radius:999px;font-family:{MONO};font-size:12.5px;color:{C_DIM}">
        <span style="width:7px;height:7px;border-radius:50%;background:#00E026;box-shadow:0 0 8px #00E026"></span>pre-release · the format is settling</span>
    </div>
  </div>

  <div style="max-width:1080px;margin:0 auto;padding:52px 56px 0">
    <div style="background:{C_SURF};border:1px solid {C_LINE};border-radius:12px;padding:30px;display:flex;flex-direction:column;gap:22px">
      {commands("#EAF1FB", C_DIM, C_LINE, C_SURF, "#00E026")}
      <div style="height:1px;background:{C_LINE}"></div>
      <p style="margin:0;font-size:15.5px;line-height:1.65;color:{C_DIM};max-width:720px;text-wrap:pretty">
        You now have the program, everything it depends on, and a lockfile recording exactly which bytes
        arrived. Point <a href="#" style="color:#6FA0F0">DRT</a> at the directory to run it.</p>
    </div>
  </div>

  <div style="max-width:1080px;margin:0 auto;padding:76px 56px 0">
    <div style="display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:34px">
      {"".join(f"""
      <div style="display:flex;flex-direction:column;gap:14px;padding-top:20px;border-top:1px solid {C_LINE}">
        {icon(k, "#6FA0F0")}
        <h3 style="margin:0;font-size:17px;font-weight:650;color:{C_INK};letter-spacing:-0.01em">{t}</h3>
        <p style="margin:0;font-size:14.5px;line-height:1.62;color:{C_DIM};text-wrap:pretty">{b}</p>
      </div>""" for k, t, b in CARDS)}
    </div>
  </div>

  <div style="max-width:1080px;margin:0 auto;padding:76px 56px 0">
    <div style="display:grid;grid-template-columns:260px 1fr;gap:34px;align-items:start;
                padding-top:22px;border-top:1px solid {C_LINE}">
      <div style="display:flex;flex-direction:column;gap:10px">
        <span style="font-family:{MONO};font-size:11.5px;letter-spacing:0.13em;text-transform:uppercase;color:{C_DIM};font-weight:600">Verifying this repo</span>
        <p style="margin:0;font-size:14px;line-height:1.6;color:{C_DIM};text-wrap:pretty">
          A verified index means a holder of this key signed exactly these bytes — authenticity, not freshness.</p>
      </div>
      <div style="background:#04101F;border:1px solid {C_LINE};border-left:3px solid #00E026;border-radius:9px;padding:18px 20px;
                  font-family:{MONO};font-size:15px;color:#CFE0F7;word-break:break-all;line-height:1.6">{KEY}</div>
    </div>
  </div>

  <div style="max-width:1080px;margin:0 auto;padding:76px 56px 0">
    <span style="font-family:{MONO};font-size:11.5px;letter-spacing:0.13em;text-transform:uppercase;color:{C_DIM};font-weight:600">Published packages</span>
    <div style="margin-top:16px">
      {"".join(f"""
      <div style="display:grid;grid-template-columns:190px 1fr 90px;gap:20px;padding:17px 0;border-top:1px solid {C_LINE};align-items:baseline">
        <span style="font-family:{MONO};font-size:14.5px;color:{C_INK};font-weight:600">{n}</span>
        <span style="font-size:14px;color:{C_DIM}">{d}</span>
        <span style="font-family:{MONO};font-size:13px;color:{C_DIM}">{v}</span>
      </div>""" for n, d, v in PKGS)}
    </div>
  </div>

  <div style="max-width:1080px;margin:0 auto;padding:66px 56px 60px">
    <div style="height:1px;background:{C_LINE}"></div>
    <div style="padding-top:26px;display:flex;flex-wrap:wrap;gap:28px;align-items:center">
      {"".join(f'<a href="{h}" style="font-size:14px;color:#6FA0F0">{t}</a>' for t, h in FOOT)}
      <span style="flex-grow:1"></span><span style="font-size:13.5px;color:{C_DIM}">Apache-2.0 · Aloecraft</span>
    </div>
  </div>
</div>'''
pathlib.Path("DirectionC.dc.html").write_text(shell(c, C_BG, css="a{color:#6FA0F0}a:hover{color:#9FC0EE}"))

import json
pathlib.Path("canvas.json").write_text(json.dumps({
  "artboards": [
    {"file": "Main.dc.html",       "x": 0,    "y": 0,    "w": 1440, "h": 1880},
    {"file": "MainDark.dc.html",   "x": 1560, "y": 0,    "w": 1440, "h": 1880},
    {"file": "Mobile.dc.html",     "x": 3120, "y": 0,    "w": 390,  "h": 2660},
    {"file": "DirectionB.dc.html", "x": 0,    "y": 2820, "w": 1440, "h": 2000},
    {"file": "DirectionC.dc.html", "x": 1560, "y": 2820, "w": 1440, "h": 1640},
  ],
  "annotations": [
    {"id": "lead", "x": 0, "y": -150, "w": 460,
     "text": "Main — the leading direction.\nDark hero, the install commands on a card straddling the fold so they read as the page's first object. Ripples from the mark carry the brand.\n\nMainDark is the same page at the dark end of prefers-color-scheme; Mobile is Main at 390."},
    {"id": "alts", "x": 0, "y": 2660, "w": 460,
     "text": "Two alternates, varying hero / commands / key panel.\n\nB — light split hero with the mark at size, install as three numbered steps that each say why.\nC — one dark deck, terminal-forward, hairline rules, key panel paired with its caveat."},
  ],
  "launch": {"view": "canvas"},
}, indent=2))
print("DirectionC + canvas.json written")
