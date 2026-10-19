#!/usr/bin/env python3
"""Export the desktop's current visual resources to Flutter, without touching sources.

Run with a Python runtime containing Pillow (the Codex workspace runtime has it).
Unchanged bytes are reused; this is a resource exporter, not an acceptance check.
"""
import hashlib
import json
import re
import xml.etree.ElementTree as ET
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
DEST = ROOT / 'apps/mobile/assets'
manifest = {}


def write(relative, data, source):
    target = DEST / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    if not target.exists() or target.read_bytes() != data:
        target.write_bytes(data)
    manifest[relative] = {'source': source, 'sha256': hashlib.sha256(data).hexdigest()}


# Fail export on missing literal icons before producing an apparently valid
# resource manifest. Include the two UI helpers whose first argument is an icon.
icon_names = {path.stem for path in (ROOT / 'crates/mio-app/assets/icons').glob('*.svg')}
icon_helpers = {'full_player.dart': ['_button'], 'shell.dart': ['_iconButton']}
missing_icons = []
for dart in sorted((ROOT / 'apps/mobile/lib').rglob('*.dart')):
    calls = ['MioIcon', *icon_helpers.get(dart.name, [])]
    pattern = r"(?:" + '|'.join(calls) + r")\(\s*['\"]([A-Za-z][A-Za-z0-9_-]*)['\"]"
    source = dart.read_text()
    for match in re.finditer(pattern, source):
        if match.group(1) not in icon_names:
            line = source.count('\n', 0, match.start()) + 1
            missing_icons.append(f"{dart.relative_to(ROOT)}:{line}: missing icon {match.group(1)}")
if missing_icons:
    raise SystemExit('\n'.join(missing_icons))
print('Literal mobile icon references match desktop assets')


for relative, source in [
    ('background-LICENSES.txt', ROOT / 'crates/mio-app/src/shell/background-LICENSES.txt'),
    ('tokens.json', ROOT / 'crates/mio-tokens/tokens.json'),
    ('locales/zh-CN.json', ROOT / 'crates/mio-core/locales/zh-CN.json'),
    ('locales/en-US.json', ROOT / 'crates/mio-core/locales/en-US.json'),
]:
    write(relative, source.read_bytes(), str(source.relative_to(ROOT)))

desktop = ROOT / 'crates/mio-app/assets'
for directory in ['icons', 'images', 'fonts', 'welcome']:
    for source in sorted((desktop / directory).rglob('*')):
        if source.is_file() and source.name != '.DS_Store':
            data = source.read_bytes()
            origin = str(source.relative_to(ROOT))
            if directory == 'icons' and source.suffix == '.svg':
                # Some desktop assets use a legacy spelling; Flutter requires viewBox.
                text = data.decode('utf-8')
                root = re.search(r'<svg\b[^>]*>', text)
                if root is None:
                    raise SystemExit(f'{origin}: missing SVG root')
                opening = re.sub(r'\bview-box(?=\s*=)', 'viewBox', root.group())
                attributes = ET.fromstring(opening + '</svg>').attrib
                if not (attributes.get('viewBox') or
                        (attributes.get('width') and attributes.get('height'))):
                    raise SystemExit(f'{origin}: missing SVG viewport dimensions')
                if opening != root.group():
                    text = text[:root.start()] + opening + text[root.end():]
                    data = text.encode('utf-8')
                    origin += '; view-box normalized to viewBox'
            write(str(source.relative_to(desktop)), data, origin)

# Same threshold, sampling step and coordinate origin as welcome_page::text_points.
mask = Image.open(desktop / 'welcome/mio-mask.png').convert('RGBA')
points = [[x - 300, y - 150] for y in range(0, mask.height, 4)
          for x in range(0, mask.width, 4) if mask.getpixel((x, y))[3] > 128]
write('welcome/points.json', (json.dumps(points, separators=(',', ':')) + '\n').encode(),
      'crates/mio-app/assets/welcome/mio-mask.png; welcome_page.rs sampling')
data = (json.dumps(manifest, ensure_ascii=False, indent=2) + '\n').encode()
target = DEST / 'manifest.json'
if not target.exists() or target.read_bytes() != data:
    target.write_bytes(data)
print(f'Exported {len(manifest)} desktop resources and {len(points)} welcome particles')
