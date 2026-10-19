#!/usr/bin/env python3
"""Read-only loopback fixture, synthetic password only; never prints query/auth data."""
import argparse
import hashlib
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs
SONGS = [dict(id='song-1',title='本地协议验证',artist='Mio Fixture',album='验证专辑',duration=125),
         dict(id='song-2',title='搜索与导航',artist='Mio Fixture',album='验证专辑',duration=189)]
class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        url = urlparse(self.path); q=parse_qs(url.query)
        token=hashlib.md5(('fixture-password'+q.get('s',[''])[0]).encode()).hexdigest()
        response={'status':'ok','version':'1.16.1'}
        if q.get('u')!=['fixture'] or q.get('t')!=[token]:
            response={'status':'failed','error':{'code':40,'message':'Authentication failed'}}
        elif url.path.endswith('/ping.view'): pass
        elif url.path.endswith('/getAlbumList2.view'):
            response['albumList2']={'album':[dict(id='album-1',name='验证专辑',artist='Mio Fixture',songCount=2)]}
        elif url.path.endswith('/search3.view'):
            term=q.get('query',[''])[0].casefold()
            response['searchResult3']={'song':[s for s in SONGS if term in (s['title']+' '+s['artist']+' '+s['album']).casefold()]}
        elif url.path.endswith('/getAlbum.view'):
            response['album']={'id':'album-1','name':'验证专辑','song':SONGS}
        else: response={'status':'failed','error':{'code':70,'message':'Not in fixture'}}
        body=json.dumps({'subsonic-response':response},ensure_ascii=False).encode()
        self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
    def log_message(self,*_): pass
if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--port',type=int,default=19450);args=parser.parse_args()
    print(f'Synthetic Subsonic fixture on 127.0.0.1:{args.port}',flush=True)
    ThreadingHTTPServer(('127.0.0.1',args.port),Handler).serve_forever()
