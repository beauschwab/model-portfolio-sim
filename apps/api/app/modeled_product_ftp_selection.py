"""Transport-only explicit saved-job selection; existing raw native FTP contract."""
from copy import deepcopy
import math,time
from . import persistence as db
from . import modeled_product_ftp_jobs as jobs
from . import modeled_product_ftp_source_binding as binding

SCHEMA='modeled-product-ftp-completed-selection-1'

def queue_input(selection,*,timeout=1800.):
    if type(timeout)not in(int,float)or not math.isfinite(timeout)or not .001<=timeout<=1800.:raise ValueError('finite original selected FTP deadline required')
    deadline=time.time()+timeout
    def check():
        if time.time()>=deadline:raise TimeoutError('original selected FTP queue deadline expired')
        return False
    if db.REPO is None or type(selection)is not dict or set(selection)!={'schema','sources','specification'}or selection['schema']!=SCHEMA or type(selection['sources'])is not list or not 1<=len(selection['sources'])<=2:raise ValueError('bounded explicit completed source selection required')
    if len(binding.encoded(selection))>jobs.MAX_BODY:raise ValueError('selected FTP request exceeds1MiB')
    raw=deepcopy(selection['specification'])
    if type(raw)is not dict or raw.get('schema')!=jobs.SCHEMA or type(raw.get('source'))is not dict or set(raw['source'])!={'schema','reviews'}or raw['source']['schema']!='modeled-product-ftp-source-proof-1':raise ValueError('explicit FTP policy template without caller raw sources required')
    captured=[];sources=[];seen=set();seen_jobs=set()
    for s in selection['sources']:
        if type(s)is not dict or set(s)!={'source_id','source_job','kind','scope','scope_review'}or type(s['source_id'])is not str or not 1<=len(s['source_id'])<=128 or s['source_id']in seen or type(s['source_job'])is not str or not 1<=len(s['source_job'])<=100 or s['source_job']in seen_jobs:raise ValueError('distinct explicit source/job IDs and scope review required')
        scope={'card_v2':'exclusive_product_owners_with_inherited_dated_sources_retained_not_funded-1','deposit_performing':'whole_isolated_continuation_deposit_owner-1','deposit_portfolio':'whole_performing_deposit_portfolio-2'}.get(s['kind'])
        if scope is None or s['scope']!=scope or type(s['scope_review'])is not dict or set(s['scope_review'])!={'id','revision'}or any(type(v)is not str or not v for v in s['scope_review'].values()):raise ValueError('exact tagged reviewed source scope required')
        seen.add(s['source_id']);seen_jobs.add(s['source_job'])
        record=binding.capture(db.REPO,db.CODEC,s['source_job'],s['kind'],check);record['source_id']=s['source_id'];captured.append(record)
        sources.append(dict(source_id=s['source_id'],scope=s['scope'],scope_review=deepcopy(s['scope_review']),input=dict(kind={'card_v2':'product','deposit_performing':'deposit_continuation','deposit_portfolio':'deposit_portfolio'}[s['kind']],request=deepcopy(record['raw']))))
    portfolio_owners=set()
    for source in sources:
        if source['input']['kind']=='deposit_portfolio':
            r=source['input']['request']
            for owner in r['positions']:
                check()
                account=next(a for a in r['specification']['accounts']if a['id']==owner['account_id'])
                if any(type(account.get(k))is not str or not account[k]for k in ('entity','currency')):
                    raise ValueError('source2 funding requires explicit raw account entity/currency identities')
                key=(account['entity'],account['currency'],owner['position_id'])
                if key in portfolio_owners:raise ValueError('overlapping source2 economic owner across source jobs')
                portfolio_owners.add(key)
    reviews=raw['source']['reviews']
    if type(reviews)is not list or not reviews or any(type(r)is not dict for r in reviews)or {r.get('source_id')for r in reviews}!=seen:raise ValueError('each selected source requires explicitly tagged funding reviews')
    keys=set()
    for r in reviews:
        if type(r)is not dict or set(r)!={'source_id','owner_index','run_index','path_index','entity','currency','policy','cash_assignment','cash_exclusion_review','channel_change_review','other_income','income_review','resets'}or any(type(r[k])is not int or r[k]<0 for k in ('owner_index','run_index','path_index')):raise ValueError('explicit indexed funding review required')
        key=tuple(r[k]for k in ('source_id','owner_index','run_index','path_index'))
        if key in keys:raise ValueError('duplicate selected funding review')
        keys.add(key)
        p=r['policy']
        if type(p)is not dict or not isinstance(p.get('review'),dict)or p.get('termination')!='complete_gross_rights_and_commitment'or p.get('tail')!='no_synthetic_cash_or_tail':raise ValueError('reviewed complete-rights funding policy required')
    raw['source']['sources']=sources
    queued=jobs.queue_input(raw,timeout=timeout)
    queued['publication_deadline_unix']=deadline # never reset the accepted queue budget
    queued['completed_sources']=captured
    check();return queued
