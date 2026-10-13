"""Empty development tokens must not produce an invalid HTTP header."""
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from threading import Thread
from types import SimpleNamespace
import pytest
from app import persistence as db


@pytest.mark.parametrize('token',['','configured-token'])
def test_remote_worker_authorization_header_is_valid(monkeypatch,token):
    headers=[]
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            headers.append(self.headers.get('Authorization'))
            self.send_response(200);self.send_header('Content-Type','application/json');self.end_headers()
            self.wfile.write(b'{"worker_ready":true}')
        def log_message(self,*args):pass
    server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
    thread=Thread(target=server.serve_forever,daemon=True);thread.start()
    monkeypatch.setattr(db,'CONFIG',SimpleNamespace(worker_url=f'http://127.0.0.1:{server.server_port}',worker_token=token))
    try:
        assert db.remote('/state',method='GET').json()=={'worker_ready':True}
        assert headers==[f'Bearer {token}' if token else None]
    finally:server.shutdown();server.server_close();thread.join()
