"""Bounded selected model-ledger process and partition transport only.
Rust actual Sim is the sole financial runtime. Independent source/GL replay is a
publication gate and never a substitute execution backend.
"""
from copy import deepcopy
from pathlib import Path
import hashlib,json,os,subprocess,sys,tempfile,threading,time,uuid
from .balance_stream import _hash
from .saved_derivative_identity import encoded,loads,probe,exact_identity
from .saved_derivative_transport import DerivativeWriter,SCHEMAS,SCHEMA
from .saved_derivative_mixed_reference import verify_preparation,verify_stream,SCHEMAS_SUPPORTED
MAX_BYTES=32*1024*1024
MAX_PARTS=65536

def binary_path():
    name='portfolio-model-ledger.exe'if sys.platform=='win32'else 'portfolio-model-ledger'
    return Path(__file__).resolve().parents[4]/'portfolio-decision-native/target/release'/name

def run(raw,directory,*,timeout=1800.,cancelled=lambda:False,partition_rows=65536,progress=None,expected_binary=None):
    if type(timeout)not in(int,float)or not 0<timeout<=1800.:raise ValueError('finite derivative original remaining deadline required')
    if type(partition_rows)is not int or not 1<=partition_rows<=65536:raise ValueError('bounded derivative partitions required')
    started=time.monotonic();deadline=started+timeout
    def check():
        probe(cancelled)
        if time.monotonic()>=deadline:raise TimeoutError('derivative original process/publication deadline expired')
        return False
    if type(raw)is not dict or raw.get('schema')not in SCHEMAS_SUPPORTED:raise ValueError('explicit saved derivative or hedge mixed schema required')
    check();binary=binary_path();sha=_hash(binary)
    if expected_binary is not None and sha!=expected_binary:raise RuntimeError('MODEL_VERSION_CHANGED: derivative binary differs')
    body=encoded(dict(request=raw,execution_timeout_ms=max(1,int(timeout*1000))))
    # The explicit acceptance tier admits a larger envelope, streams the journal and binds the
    # exact input bytes by hash; every other request keeps the historical bounds.
    tiered=raw.get('scale_tier')is not None
    envelope_cap,stream_cap,retained_cap=(512*1024*1024,16*1024**3,16*1024**3)if tiered else(MAX_BYTES,MAX_BYTES,128*1024*1024)
    if len(body)>envelope_cap:raise ValueError('derivative raw envelope exceeds32MiB')
    body_sha256=hashlib.sha256(body).hexdigest()
    root=Path(directory).resolve();root.mkdir(parents=True,exist_ok=True)
    staging=Path(tempfile.mkdtemp(prefix='derivative-staging-',dir=root));writer=DerivativeWriter(staging,partition_rows)
    header=None;completion=None;dictionary=[];wire_bytes=0;control=None;stop=threading.Event();failure=[]
    try:
        (staging/'input.json').write_bytes(body)
        with(staging/'stderr.log').open('wb')as err:
            process=subprocess.Popen([str(binary)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=err)
            def watch():
                while not stop.wait(.02):
                    try:check()
                    except BaseException as e:
                        failure.append(e)
                        if process.poll()is None:process.kill()
                        return
            thread=threading.Thread(target=watch,daemon=True);thread.start()
            try:
                process.stdin.write(body);process.stdin.close()
                while True:
                    check();line=process.stdout.readline(MAX_BYTES+1)
                    if not line:break
                    wire_bytes+=len(line)
                    if wire_bytes>stream_cap:raise ValueError('complete derivative native stream exceeds32MiB')
                    message=loads(line)
                    if completion is not None:raise ValueError('native output after completion')
                    if header is None:
                        if set(message)!={'workflow'}:raise ValueError('first workflow header required')
                        header=message['workflow']
                        identity_key='input_sha256'if tiered else'prepared_request'
                        if set(header)!={'schema','model_schema',identity_key,'source_receipt'}or header['schema']!='model-ledger-preparation-1'or header['model_schema']!=raw['schema']:raise ValueError('derivative workflow shape')
                        if tiered:
                            if header['input_sha256']!=body_sha256:raise ValueError('native input identity differs from the sent envelope')
                            control=verify_preparation(raw,None,header['source_receipt'],check,input_sha256=body_sha256)
                        else:control=verify_preparation(raw,header['prepared_request'],header['source_receipt'],check)
                        (staging/'source.json').write_bytes(encoded(header));continue
                    if 'complete'in message:
                        if not exact_identity(message,{'complete':True,'protocol':3}):raise ValueError('actual strict completion required')
                        completion=message;(staging/'completion.json').write_bytes(encoded(completion));continue
                    if message.get('table')not in SCHEMAS:raise ValueError('unknown derivative table')
                    name=message['table']
                    if 'columns'in message:
                        if name!='journal'or set(message)!={'table','columns','dictionary'}:raise ValueError('unknown native columnar table')
                        columns=message['columns'];keys={'scenario','account','event','gl_account','instrument_id'}
                        if set(columns)!=keys|{'day','value','transaction_number'}or len({len(v)for v in columns.values()})!=1:raise ValueError('journal wire columns')
                        if type(message['dictionary'])is not list or any(type(s)is not str for s in message['dictionary']):raise ValueError('journal string dictionary')
                        dictionary.extend(message['dictionary']);rows=[]
                        for i,value in enumerate(columns['value']):
                            check();row={}
                            for k in keys:
                                index=columns[k][i]
                                if type(index)is not int or not 0<=index<len(dictionary):raise ValueError('journal dictionary index')
                                row[k]=dictionary[index]
                            number,day=columns['transaction_number'][i],columns['day'][i]
                            if type(number)is not int or type(day)is not int:raise ValueError('journal exact integer wire')
                            row.update(transaction_id=f'{row["scenario"]}:{number}',day=day,debit=max(value,0.),credit=-min(value,0.));rows.append(row)
                        writer.add(name,rows)
                    else:
                        if set(message)!={'table','rows'}:raise ValueError('unknown native row table')
                        writer.add(name,message['rows'])
                    if progress:progress(name)
                process.wait(timeout=max(.001,deadline-time.monotonic()))
                if failure:raise failure[0]
                if process.returncode or completion is None:raise RuntimeError('native derivative process failed: '+(staging/'stderr.log').read_text(errors='replace')[-2000:])
            finally:
                stop.set();thread.join(timeout=1.)
                if process.poll()is None:process.kill()
                process.wait();process.stdout.close()
        writer.finish();writer.completion=completion
        receipt=verify_stream(raw,control,writer,check)
        tables={name:dict(schema={k:str(v)for k,v in schema.items()},parts=deepcopy(writer.parts[name]))for name,schema in SCHEMAS.items()}
        descriptors=[]
        for name in('input.json','source.json','completion.json'):
            f=staging/name;descriptors.append(dict(path=name,bytes=f.stat().st_size,sha256=_hash(f)))
        count=0;size=sum(d['bytes']for d in descriptors)
        for table in tables.values():
            for part in table['parts']:
                count+=1;size+=part['bytes']
        # Columnar metadata overhead is additional to native32MiB stream, explicit128MiB cap.
        if count>MAX_PARTS or size>retained_cap:raise ValueError('derivative retained transport capacity/count cap')
        manifest=dict(version='saved-derivative-partitions-1',model_version=raw['schema'],backend='rust',binary_sha256=sha,partition_rows=partition_rows,
            validation=receipt,input=descriptors[0],modeled_source=descriptors[1],producer_completion=descriptors[2],tables=tables)
        (staging/'stderr.log').unlink()
        (staging/'manifest.json').write_bytes(encoded(manifest))
        def finalcheck(current):
            check()
            if _hash(binary)!=sha:raise RuntimeError('MODEL_VERSION_CHANGED: derivative executed binary changed')
            for d in descriptors+[p for t in tables.values()for p in t['parts']]:
                check();f=current/d['path']
                if f.stat().st_size!=d['bytes']or _hash(f)!=d['sha256']:raise ValueError('verified derivative transport changed')
                check()
            if not exact_identity(loads((current/'manifest.json').read_bytes()),manifest):raise ValueError('verified derivative manifest changed')
            check()
        final=root/('derivative-'+uuid.uuid4().hex);finalcheck(staging);staging.rename(final)
        try:finalcheck(final)
        except BaseException:
            import shutil;shutil.rmtree(final);raise
        return final/'manifest.json'
    except BaseException:
        import shutil
        if staging.exists():shutil.rmtree(staging)
        raise
