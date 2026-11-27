"""Copy and shrink the assets the desktop app embeds from the ear-web checkout.

The web app ships 1536px product renders (~90 MB). The desktop UI shows them at
<=160 logical px, so they are downscaled to 320px lossy WebP (~2-3 MB total).
Run from the repo root:  python tools/prepare_assets.py [path/to/ear-web]
"""
import json, os, re, shutil, sys
from PIL import Image

src = sys.argv[1] if len(sys.argv) > 1 else "ear-web"
res = os.path.join(src, "res")
out = "assets"
size = 320

os.makedirs(os.path.join(out, "devices"), exist_ok=True)
for name in sorted(os.listdir(os.path.join(res, "assets"))):
    if not re.match(r"b\d+_[a-z]+_(left|right|case)\.webp$", name):
        continue
    im = Image.open(os.path.join(res, "assets", name)).convert("RGBA")
    bbox = im.getbbox()  # crop transparent margins so images sit tight in the UI
    if bbox:
        im = im.crop(bbox)
    im.thumbnail((size, size), Image.LANCZOS)
    im.save(os.path.join(out, "devices", name), "WEBP", quality=88, method=6)

# minified model/firmware capability table
with open(os.path.join(res, "js", "ear_config_file.json"), encoding="utf8") as f:
    data = json.load(f)
with open(os.path.join(out, "ear_config.json"), "w", encoding="utf8") as f:
    json.dump(data, f, separators=(",", ":"))

fonts = {"space_grotesk_regular.ttf": "SpaceGrotesk-Regular.ttf",
         "space_grotesk_medium.ttf": "SpaceGrotesk-Medium.ttf",
         "ndot_57.otf": "Ndot57.otf"}
os.makedirs(os.path.join(out, "fonts"), exist_ok=True)
for a, b in fonts.items():
    shutil.copy(os.path.join(res, "fonts", a), os.path.join(out, "fonts", b))
shutil.copy(os.path.join(res, "icons", "256x256.png"), os.path.join(out, "icon.png"))
print("ok")
