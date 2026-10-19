#!/usr/bin/env python3
"""Regenerate source inventories and raw CSS tokens; never writes to Mio-Music."""
import argparse
import hashlib
import json
import re
import subprocess
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def css_rules(text):
    """Yield balanced CSS blocks, retaining media/supports context and cascade order."""
    text = re.sub(r'/\*.*?\*/', '', text, flags=re.S)
    def walk(body, context):
        cursor = 0
        while (opening := body.find('{', cursor)) >= 0:
            selector = body[cursor:opening].strip()
            depth, end = 1, opening + 1
            while depth and end < len(body):
                depth += (body[end] == '{') - (body[end] == '}')
                end += 1
            if depth:
                raise ValueError('Unbalanced CSS')
            inner = body[opening + 1:end - 1]
            if selector.startswith(('@media', '@supports', '@layer')):
                yield from walk(inner, context + [selector])
            else:
                yield selector, inner, context
            cursor = end
    yield from walk(text, [])


def dump(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + '\n')


def table(path, columns, rows):
    def cell(value):
        return str(value).replace('|', '\\|').replace('\n', ' ')
    path.write_text('# ' + path.stem + '\n\n源快照静态清单；未核对映射不表示未实现，当前实现与证据见 [progress.md](progress.md)。\n\n'
                    + '| ' + ' | '.join(columns) + ' |\n'
                    + '| ' + ' | '.join(['---'] * len(columns)) + ' |\n'
                    + ''.join('| ' + ' | '.join(map(cell, row)) + ' |\n' for row in rows))


def extract_tokens(source):
    src = source / 'src'
    css = src / 'assets/main.css'
    component_css = source / 'node_modules/tdesign-vue-next/es/style/index.css'
    declarations = []
    themes = {'light': {}, 'dark': {}}
    # TDesign supplies aliases omitted by the application's theme overrides.
    for selector, body, context in css_rules(component_css.read_text()):
        if context or ':root' not in selector:
            continue
        values = {key: value.strip() for key, value in
                  re.findall(r'(--[\w-]+)\s*:\s*([^;]+);', body)}
        modes = ['dark'] if ':root.dark' in selector or "theme-mode='dark'" in selector else ['light', 'dark']
        for mode in modes:
            themes[mode].update(values)
    for selector, body, context in css_rules(css.read_text()):
        values = dict(re.findall(r'(--[\w-]+)\s*:\s*([^;]+);', body))
        if not values or ':root' not in selector:
            continue
        values = {key: value.strip() for key, value in values.items()}
        declarations.append({'selector': selector, 'context': context, 'values': values})
        if context:  # conditional tokens remain in rules; never silently flatten them
            continue
        modes = ['dark'] if "data-theme='dark'" in selector else ['light'] if "data-theme='light'" in selector else ['light', 'dark']
        for mode in modes:
            themes[mode].update(values)
    dump(ROOT / 'crates/mio-tokens/tokens.json', {
        'source': 'Mio-Music/src/assets/main.css',
        'sha256': hashlib.sha256(css.read_bytes()).hexdigest(),
        'component_source': 'Mio-Music/node_modules/tdesign-vue-next/es/style/index.css',
        'component_sha256': hashlib.sha256(component_css.read_bytes()).hexdigest(),
        'themes': themes, 'rules': declarations,
    })
    return themes


def extract(source):
    src = source / 'src'
    themes = extract_tokens(source)
    # Preserve original font paths as vector icons for platforms with different font fallback.
    font = ET.parse(src / 'assets/icon_font/iconfont.svg').getroot()
    glyphs = {g.attrib.get('unicode'): g.attrib for g in font.iter('{http://www.w3.org/2000/svg}glyph')}
    catalog = json.loads((src / 'assets/icon_font/iconfont.json').read_text())
    for name in ('faxian', 'yanchu', 'shouye', 'xiazai'):
        item = next(g for g in catalog['glyphs'] if g['font_class'] == name)
        path = glyphs[chr(int(item['unicode'], 16))]['d']
        destination = ROOT / f'crates/mio-app/assets/icons/nav-{name}.svg'
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024"><path transform="translate(0 896) scale(1 -1)" d="{path}"/></svg>\n')
    locales = {}
    # These tracked modules contain export-default object literals, not application code.
    script = "const vm=require('node:vm');const fs=require('node:fs');const input=JSON.parse(fs.readFileSync(0,'utf8'));process.stdout.write(JSON.stringify(input.map(s=>vm.runInNewContext('('+s.replace(/^export default\\s*/, '')+'\\n)',{}, {timeout:1000}))));"
    for language in ['zh-CN', 'en-US']:
        files = sorted(path for path in (src / 'locales' / language).glob('*.ts') if path.stem not in ['ai', 'index'])
        values = json.loads(subprocess.run(['node', '-e', script], input=json.dumps([f.read_text() for f in files]), text=True, capture_output=True, check=True).stdout)
        locales[language] = dict(zip([f.stem for f in files], values))
        dump(ROOT / f'crates/mio-core/locales/{language}.json', locales[language])
    def flatten(value, prefix=''):
        result = {}
        for key, item in value.items():
            name = prefix + key
            if isinstance(item, dict):
                result.update(flatten(item, name + '.'))
            else:
                result[name] = item
        return result
    zh, en = map(flatten, [locales['zh-CN'], locales['en-US']])
    missing_locale_keys = sorted(zh.keys() ^ en.keys())
    parity = ROOT / 'docs/parity'
    parity.mkdir(parents=True, exist_ok=True)
    table(parity / 'i18n.md', ['键', '中文', 'English'], [(key, zh.get(key, '源文案缺失'), en.get(key, '源文案缺失')) for key in sorted(zh.keys() | en.keys())])
    rust = source / 'src-tauri/src'
    handler = re.search(r'generate_handler!\s*\[([^]]+)\]', (rust / 'lib.rs').read_text(), re.S).group(1)
    handler = re.sub(r'//[^\n]*', '', handler)
    commands = re.findall(r'[\w]+(?:::[\w]+)+', handler)
    table(parity / 'commands.md', ['源路径', 'GPUI 实现', '状态'], [(cmd, '—', '待核对映射') for cmd in commands])
    events = {}
    for path in sorted(list(rust.rglob('*.rs')) + list(src.rglob('*.ts')) + list(src.rglob('*.vue'))):
        text = path.read_text()
        for match in re.finditer(r'\b(emit|emit_to|listen)\s*(?:<[^>]+>)?\s*\(\s*[\'\"]([\w:-]+)[\'\"]', text):
            events.setdefault((match[1], match[2]), []).append(str(path.relative_to(source)))
    table(parity / 'events.md', ['调用', '事件', '来源', '状态'], [(kind, event, ', '.join(sorted(set(paths))), '待核对映射') for (kind, event), paths in sorted(events.items())])
    router = (src / 'router/index.ts').read_text()
    routes = re.findall(r"name:\s*'([^']+)'", router)
    # Keep manually reconciled screen evidence when refreshing the source inventory.
    screen_file = parity / 'screens.md'
    screen_rows = {}
    if screen_file.exists():
        for line in screen_file.read_text().splitlines():
            cells = [cell.strip() for cell in line.strip('|').split('|')]
            if line.startswith('| ') and len(cells) == 4 and cells[0] in routes:
                screen_rows[cells[0]] = tuple(cells)
    table(screen_file, ['路由', '黄金图', '布局对比', '状态'],
          [screen_rows.get(name, (name, '未登记', '未登记', '待核对映射')) for name in routes])
    hotkeys = (src / 'types/hotkeys.ts').read_text().split('bindings: {')[-1]
    table(parity / 'hotkeys.md', ['动作', '默认键', '状态'], [(key, value, '待核对映射') for key, value in re.findall(r"(\w+):\s*'([^']+)'", hotkeys)])
    setting_rows = []
    for path in sorted((src / 'views/settings/sections').glob('*.vue')):
        bindings = sorted(set(re.findall(r'v-model(?::[\w-]+)?(?:\.[\w-]+)*="([^"]+)"', path.read_text())))
        setting_rows.extend((path.stem, binding, '待核对默认值/取值范围/业务影响') for binding in bindings)
    table(parity / 'settings.md', ['分区文件', '源控件绑定', '状态'], setting_rows)
    animation_rows = []
    for path in sorted(list(src.rglob('*.vue')) + list((src / 'assets').glob('*.css'))):
        styles = path.read_text()
        if path.suffix == '.vue':
            styles = '\n'.join(re.findall(r'<style[^>]*>(.*?)</style>', styles, re.S))
        styles = re.sub(r'/\*.*?\*/', '', styles, flags=re.S)
        for match in re.finditer(r'(?<![\w-])(transition(?:-[\w-]+)?|animation(?:-[\w-]+)?)\s*:\s*([^;{}]+);', styles):
            animation_rows.append((str(path.relative_to(source)), match[1], ' '.join(match[2].split()), '待核对触发与运行时值'))
    table(parity / 'animations.md', ['来源', '属性', '原始声明', '状态'], animation_rows)
    asset_rows = []
    for path in sorted((src / 'assets').rglob('*')):
        if path.is_file():
            asset_rows.append((str(path.relative_to(source)), hashlib.sha256(path.read_bytes()).hexdigest()))
    table(parity / 'assets.md', ['资源', 'SHA256'], asset_rows)
    dump(parity / 'source.json', {
        'commit': subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip(),
        'commands': len(commands), 'event_calls': len(events), 'routes': len(routes), 'locale_keys': len(zh),
        'tokens': {mode: len(values) for mode, values in themes.items()},
        'excluded_namespaces': ['ai'], 'source_locale_mismatches': missing_locale_keys,
        'scope': 'static source inventory; runtime layouts and conditional CSS require browser extraction',
    })
    print(f'Extracted {len(commands)} commands, {len(events)} event calls, {len(routes)} routes, {len(zh)} locale keys (ai excluded).')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, default=ROOT.parent / 'Mio-Music')
    args = parser.parse_args()
    extract(args.source.resolve())
