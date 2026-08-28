"""Render each artboard's body standalone and report its real height, so the
canvas.json frames are measured rather than estimated (surplus frame paints
dead background; a short frame clips)."""
import json, pathlib, re, subprocess, tempfile

CHROME = "/opt/pw-browsers/chromium_headless_shell-1194/chrome-linux/headless_shell"
widths = {"Main": 1440, "MainDark": 1440, "Mobile": 390, "DirectionB": 1440, "DirectionC": 1440}
out = {}
for name, w in widths.items():
    src = pathlib.Path(f"{name}.dc.html").read_text()
    style = re.search(r"<helmet>\s*<style>(.*?)</style>\s*</helmet>", src, re.S).group(1)
    body = re.search(r"</helmet>(.*?)</x-dc>", src, re.S).group(1)
    page = f"<!doctype html><meta charset=utf-8><style>{style}</style>{body}"
    with tempfile.NamedTemporaryFile("w", suffix=".html", delete=False) as f:
        f.write(page); path = f.name
    r = subprocess.run([CHROME, "--headless", "--no-sandbox", "--disable-gpu",
                        f"--window-size={w},900", "--virtual-time-budget=2000",
                        "--dump-dom", f"file://{path}"], capture_output=True, text=True, timeout=60)
    # measure via a second pass using --screenshot full page height is awkward;
    # instead ask the page itself
    probe = page + "<script>document.title=document.documentElement.scrollHeight</script>"
    with open(path, "w") as f: f.write(probe)
    r = subprocess.run([CHROME, "--headless", "--no-sandbox", "--disable-gpu",
                        f"--window-size={w},900", "--virtual-time-budget=2000",
                        "--dump-dom", f"file://{path}"], capture_output=True, text=True, timeout=60)
    m = re.search(r"<title>(\d+)</title>", r.stdout)
    out[name] = int(m.group(1)) if m else None
print(json.dumps(out, indent=2))
