#!/usr/bin/env python3
"""Small cascade regression check for the source token extractor."""
import importlib.util
from pathlib import Path
spec = importlib.util.spec_from_file_location('extract', Path(__file__).with_name('extract-source.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
rules = list(module.css_rules('''
/* removed */ :root { --a: rgba(1, 2, 3, .4); }
@media (max-width: 768px) { :root { --a: red; } }
:root[data-theme='dark'] { --a: blue; }
:root { --b: var(--a); }
'''))
assert len(rules) == 4
assert rules[1][2] == ['@media (max-width: 768px)']
assert rules[2][0] == ":root[data-theme='dark']"
assert rules[3][2] == []
print('CSS cascade and conditional rule extraction passed.')
