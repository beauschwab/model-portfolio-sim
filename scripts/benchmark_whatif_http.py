"""Actual HTTP/job/Arrow latency on a DISPOSABLE local API, default port 8001.

Does not edit books or market. Temporarily sets 128-path/four-thread settings,
restoring them in finally. Never aim this at a shared/live workbench.
"""
import io
import json
from pathlib import Path
import statistics
import struct
import time
import httpx
import numpy as np
import polars as pl

OUT = Path(__file__).resolve().parents[1] / 'docs/reviews/2026-09-28-whatif-http.json'


def decode(blob):
    assert blob[:4] == b'ARW1'
    size = struct.unpack_from('<I', blob, 4)[0]
    body = json.loads(blob[8:8+size]); at = 8+size
    count = struct.unpack_from('<I', blob, at)[0]; at += 4
    sizes = struct.unpack_from(f'<{count}I', blob, at); at += count*4
    frames = []
    for size in sizes:
        frames.append(pl.read_ipc(io.BytesIO(blob[at:at+size]))); at += size
    def resolve(v):
        if isinstance(v, dict):
            if '__arrow__' in v: return frames[v['__arrow__']]
            return {k: resolve(x) for k,x in v.items()}
        if isinstance(v, list): return [resolve(x) for x in v]
        return v
    return resolve(body)


def submit(client, body):
    response=client.post('/run',json=body); response.raise_for_status(); jid=response.json()['id']
    deadline=time.monotonic()+120
    while True:
        response=client.get(f'/jobs/{jid}'); response.raise_for_status(); job=response.json()
        if job['status'] in ('done','error'): break
        if time.monotonic()>deadline: raise TimeoutError(jid)
        time.sleep(.01)
    assert job['status']=='done', job
    response=client.get(f'/jobs/{jid}/result'); response.raise_for_status()
    return decode(response.content), len(response.content)


def main():
    with httpx.Client(base_url='http://127.0.0.1:8001', timeout=120) as client:
        settings = client.get('/settings').json()
        response = client.put('/settings', json=settings | {'n_paths':128,'n_paths_base':128,'n_threads':4,'horizon_months':27})
        response.raise_for_status()
        try:
            books = decode(client.get('/books/loans').content)
            if isinstance(books, dict):
                books = next(v for v in books.values() if isinstance(v, pl.DataFrame))
            ident, coupon = books['id'][0], books['coupon_or_spread'][0]
            revision = client.get('/state').json()['revision']
            report = {'url':str(client.base_url),'paths':128,'threads':4,'horizon':27,'positions':375,'poll_ms':10,'runs':[]}
            for analytics in (False, True):
                for backend_index, backend in enumerate(('numpy','numba','rust')):
                    timings=[]
                    for i in range(6):
                        body = dict(kind='whatif',books=['mbs','loans','debt','deposits','cds'],
                            assumption_overrides={'loans':{ident:{'coupon_or_spread':coupon+.001*(i+1)+.00013*(backend_index+1)}}},
                            include_analytics=analytics,backend=backend,expected_revision=revision)
                        start=time.perf_counter()
                        result, size=submit(client, body)
                        timings.append((time.perf_counter()-start)*1000)
                    reference=result if backend=='numpy' else submit(client, body | {'backend':'numpy'})[0]
                    errors=[]
                    for book, frame in result['revised']['positions'].items():
                        x=frame.drop('id').to_numpy(); y=reference['revised']['positions'][book].drop('id').to_numpy()
                        np.testing.assert_allclose(x,y,rtol=1e-9,atol=1e-4)
                        errors.append(float(np.max(np.abs(x-y))))
                    if analytics:
                        np.testing.assert_allclose(result['revised']['nii']['monthly']['nii'],reference['revised']['nii']['monthly']['nii'],rtol=1e-10,atol=1e-5)
                    report['runs'].append({'analytics':analytics,'backend':backend,'first_ms':timings[0],
                        'median_ms':statistics.median(timings[1:]),'p95_ms':float(np.percentile(timings[1:],95)),
                        'samples_ms':timings[1:],'arrow_bytes':size,'max_numpy_error':max(errors),
                        'cache':result['revised']['cache']})
                    print(analytics,backend,round(statistics.median(timings[1:]),2),flush=True)
            report['scope']='Loopback HTTP submission, queue, polling, serialization, transfer and Polars Arrow decode; no competing jobs. First call separate, not process-cold or comparable across backends. Different coupon edits for every backend/sample prevent shared cashflow-cache prewarming. Each last result checked against NumPy on identical inputs outside timing. Browser debounce/render measured separately.'
            OUT.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
        finally:
            client.put('/settings',json=settings).raise_for_status()


if __name__=='__main__': main()
