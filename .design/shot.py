import pathlib, re, subprocess, sys
CHROME = "/opt/pw-browsers/chromium_headless_shell-1194/chrome-linux/headless_shell"
name, w, h = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
src = pathlib.Path(f"{name}.dc.html").read_text()
style = re.search(r"<helmet>\s*<style>(.*?)</style>\s*</helmet>", src, re.S).group(1)
body = re.search(r"</helmet>(.*?)</x-dc>", src, re.S).group(1)
pathlib.Path(f"/tmp/{name}.html").write_text(f"<!doctype html><meta charset=utf-8><style>{style}</style>{body}")
subprocess.run([CHROME, "--headless", "--no-sandbox", "--disable-gpu", "--hide-scrollbars",
                f"--window-size={w},{h}", "--virtual-time-budget=3000",
                f"--screenshot=/tmp/{name}.png", f"file:///tmp/{name}.html"],
               capture_output=True, timeout=90)
print(f"/tmp/{name}.png")
