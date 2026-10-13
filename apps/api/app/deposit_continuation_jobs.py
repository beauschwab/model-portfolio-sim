"""Private post-upload source binding for the selected deposit SQL publication."""
from copy import deepcopy
import hashlib,math,time
import sqlalchemy as sa
from . import persistence as db
from .repository import jobs,workspaces
from portfolio_risk.analytics.modeled_json_identity import exact_json_identity as exact

SCHEMA='deposit-performing-income-ledger-1'
SCHEMAS={SCHEMA,'deposit-performing-portfolio-ledger-2'}

def prepare_publication(request,output,*,payload_identity,expected_deadline,job,stopping,selected_schemas=SCHEMAS):
    """Reconstruct uploaded sources once; return a guard for the final SQL lock.

    No caller completion flag/financial state supplies authority. The guard owns
    private copies of the already independently verified immutable descriptors.
    """
    from .model_jobs import verify_modeled_persisted
    from portfolio_risk.analytics import model_ledger,ledger_native,balance_stream
    if request.get('specification',{}).get('schema') not in selected_schemas:
        raise ValueError('explicit performing deposit source required')
    if type(expected_deadline) is not float or not math.isfinite(expected_deadline) or not exact(expected_deadline,request.get('publication_deadline_unix')):
        raise ValueError('deposit accepted deadline differs')
    raw=deepcopy(request);identity=deepcopy(payload_identity);ref=deepcopy(output)
    bound_job={key:deepcopy(job[key]) for key in ('id','owner','token','request','revision','attempt')}
    objects=db.CODEC.objects;codec=db.CODEC;repo=db.REPO
    def probe():
        if stopping():raise InterruptedError('deposit publication cancelled')
        if time.time()>=expected_deadline:raise TimeoutError('deposit original publication deadline exceeded')
    def load_bound(descriptor,selected=None):
        import json
        probe()
        manifest=json.loads(objects.get_bounded(descriptor,32*1024*1024))
        probe()
        if type(manifest.get('schema_version')) is not int or manifest['schema_version']!=1 or manifest.get('value',{}).get('type')!='dict':
            raise ValueError('deposit bounded manifest schema')
        if selected is not None:
            manifest['value']={'type':'dict','items':[pair for pair in manifest['value']['items'] if isinstance(pair[0],str) and pair[0] in selected]}
        def visit(node):
            probe()
            if isinstance(node,dict):
                kind=node.get('type')
                if kind not in {'dict','list','tuple','partitioned','object-reference'}:
                    raise ValueError('deposit manifest cannot hydrate financial tables or foreign model nodes')
                if kind=='dict':
                    for pair in node['items']:
                        visit(pair[0]);visit(pair[1])
                elif kind in {'list','tuple'}:
                    for item in node['items']:visit(item)
        visit(manifest['value'])
        return codec.decode(manifest['value'])
    probe()
    result=load_bound(ref)
    verify_modeled_persisted(raw,result,job=job,stopping=stopping,expected_deadline=expected_deadline)
    probe()
    descriptors=[ref,bound_job['request'],result['execution']['input'],result['execution']['modeled_source'],result['execution']['producer_completion']]
    for name in model_ledger.SCHEMAS[raw['specification']['schema']]:
        for part in result[name]['parts']:
            if len(descriptors)>=65536:raise ValueError('deposit publication object-count cap')
            descriptors.append(part['ref'])
    # This bounds descriptor retention/IO independently of native financial work.
    if len(descriptors)>65536:raise ValueError('deposit publication object-count cap')
    descriptors=deepcopy(descriptors)
    for descriptor in descriptors:
        if (type(descriptor.get('bytes')) is not int or not 0<descriptor['bytes']<=32*1024*1024 or
                descriptor.get('format') not in {'json','bin','parquet'} or
                type(descriptor.get('sha256')) is not str or len(descriptor['sha256'])!=64):
            raise ValueError('invalid bounded deposit immutable object descriptor')
    def owner(conn):
        probe()
        row=conn.execute(sa.select(jobs.c.status,jobs.c.owner,jobs.c.token,jobs.c.request,jobs.c.revision,jobs.c.attempt).where(repo.scoped(jobs),jobs.c.id==bound_job['id'])).mappings().first()
        lease=conn.execute(sa.select(workspaces.c.owner).where(repo.scoped(workspaces),workspaces.c.owner==bound_job['owner'],workspaces.c.lease_until>time.time())).scalar_one_or_none()
        if row is None or row['status']!='running' or row['owner']!=bound_job['owner'] or row['token']!=bound_job['token'] or lease is None:
            raise InterruptedError('deposit SQL publication ownership lost')
        if any(not exact(row[key],bound_job[key]) for key in ('request','revision','attempt')):
            raise ValueError('deposit queued SQL authority changed')
        queued=load_bound(bound_job['request'],{'operation','args','identity'})
        if (queued.get('operation')!='store.run_modeled_balance_stress' or not exact(queued.get('identity'),identity) or
                type(queued.get('args')) not in (tuple,list) or len(queued['args'])!=1 or not exact(queued['args'][0],raw)):
            raise ValueError('deposit queued raw/bundle/deadline authority changed')
        probe()
    def artifacts():
        probe()
        if not exact(db.current_identity(),identity):raise RuntimeError('MODEL_VERSION_CHANGED: deposit deployed bundle changed')
        for path,sha in ((model_ledger.binary_path(),raw['binary_sha256']),(ledger_native.library_path(),raw['validation_library_sha256'])):
            probe()
            if not path.is_file() or balance_stream._hash(path)!=sha:raise RuntimeError('MODEL_VERSION_CHANGED: deposit native artifacts changed')
            probe()
        if balance_stream.source_identity()!=raw['source_sha256']:raise RuntimeError('MODEL_VERSION_CHANGED: deposit source changed')
        probe()
    def commit(conn):
        if not exact(output,ref) or not exact(request,raw) or any(not exact(job[key],bound_job[key]) for key in bound_job):
            raise ValueError('private deposit publication identity changed')
        if db.CODEC is not codec or db.REPO is not repo or codec.objects is not objects:
            raise RuntimeError('deposit publication storage owner changed')
        owner(conn);artifacts()
        for descriptor in descriptors:
            probe()
            data=objects.get_bounded(descriptor,32*1024*1024)
            if len(data)!=descriptor['bytes'] or hashlib.sha256(data).hexdigest()!=descriptor['sha256']:
                raise ValueError('deposit verified immutable object changed')
            probe()
        artifacts();owner(conn)
    return commit
