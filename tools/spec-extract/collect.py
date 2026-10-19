#!/usr/bin/env python3
"""Loopback-only collector for specs explicitly sent from the reference WebView."""
import json
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2] / 'docs/parity/baselines'
class Handler(BaseHTTPRequestHandler):
    def do_OPTIONS(self):
        self.send_response(204); self.headers_out(); self.end_headers()
    def headers_out(self):
        self.send_header('Access-Control-Allow-Origin', 'http://localhost:1420')
        self.send_header('Access-Control-Allow-Headers', 'Content-Type')
        self.send_header('Access-Control-Allow-Methods', 'POST, OPTIONS')
    def do_POST(self):
        length = int(self.headers.get('Content-Length', '0'))
        if self.headers.get('Origin') != 'http://localhost:1420' or not 0 < length < 1000000:
            self.send_error(400); return
        data = json.loads(self.rfile.read(length))
        view = data['viewport']; theme = data['theme']
        if theme not in ('light', 'dark') or view['width'] not in (1200,768) or view['height'] != 800:
            self.send_error(400); return
        name = f"reference-{theme}-{view['width']}x{view['height']}.json"
        ROOT.mkdir(parents=True, exist_ok=True)
        (ROOT / name).write_text(json.dumps(data, ensure_ascii=False, indent=2)+'\n')
        self.send_response(200); self.headers_out(); self.end_headers(); self.wfile.write(b'captured')
    def log_message(self, *_): pass
print('Reference collector on 127.0.0.1:1421', flush=True)
HTTPServer(('127.0.0.1', 1421), Handler).serve_forever()
