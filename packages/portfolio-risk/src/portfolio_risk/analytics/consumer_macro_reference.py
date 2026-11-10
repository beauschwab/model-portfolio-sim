"""Independent macro source/diagnostic verifier; never a financial execution backend.

Daily product ownership, fitted hazards and Pricing/EIR income require their own
verification. This receipt does not authorize durable publication or adoption.
"""
from copy import deepcopy
import hashlib
import math
import struct
import json
import unicodedata

POLICY = 'consumer-macro-typed-binary64-1'
SCHEMA = 'consumer-macro-scenarios-1'
METRICS = {
    'short_rate': 'percent', 'unemployment_level': 'percent',
    'unemployment_change12m': 'percentage_points', 'used_vehicle_index': 'index_level',
    'used_vehicle_index_change12m': 'percent', 'prime_minus_policy': 'percentage_points',
    'card_apr_spread': 'percentage_points', 'consumer_rate_change12m': 'percentage_points',
    'real_disposable_income_growth': 'percent', 'hpi_change12m': 'percent',
}

def require(condition, message):
    if not condition: raise ArithmeticError('consumer macro reference: '+message)

def checkpoint(check):
    if check(): raise InterruptedError('consumer macro verification cancelled')

def keys(value, names):
    require(type(value) is dict and set(value)==set(names.split()), 'exact source fields')

def label(value):
    require(type(value) is str and value.strip() and len(value.encode())<=256
            and all(unicodedata.category(c)!='Cc' for c in value), 'identity')

def identity(value):
    keys(value, 'id revision'); label(value['id']); label(value['revision'])

def integer(value, lower, upper):
    require(type(value) is int and lower<=value<=upper, 'integer domain')
    return value

def finite(value):
    require(type(value) in (float,int) and math.isfinite(value), 'finite binary64')
    return float(value)

def instant(value):
    keys(value,'ordinal seconds_of_day')
    return integer(value['ordinal'],1,3652059)*86400+integer(value['seconds_of_day'],0,86399)

def digest(value, check=lambda:False):
    sha=hashlib.sha256(POLICY.encode())
    def size(n): sha.update(struct.pack('<Q',n))
    def walk(v):
        checkpoint(check)
        if v is None: sha.update(b'n')
        elif type(v) is bool: sha.update(b't' if v else b'f')
        elif type(v) is str:
            data=v.encode();sha.update(b's');size(len(data));sha.update(data)
        elif type(v) is float:
            require(math.isfinite(v),'nonfinite digest');sha.update(b'd');sha.update(struct.pack('<d',v))
        elif type(v) is int:
            integer(v,-2**63,2**64-1)
            sha.update(b'i' if v<2**63 else b'u');sha.update(struct.pack('<q' if v<2**63 else '<Q',v))
        elif type(v) is list:
            sha.update(b'a');size(len(v))
            for item in v: walk(item)
        elif type(v) is dict:
            require(all(type(k) is str for k in v),'object keys');sha.update(b'o');size(len(v))
            for k in sorted(v): walk(k);walk(v[k])
        else: raise ArithmeticError('consumer macro reference: unsupported digest type')
    walk(value);return sha.hexdigest()

def normalise(raw):
    # Typed JSON input permits integer spelling of declared f64 fields, not of
    # ordinal/path/seed identities. Preserve every original binary64 sign bit.
    require(len(json.dumps(raw,allow_nan=False,separators=(',',':')).encode())<=4*1024*1024,'raw input byte bound')
    r=deepcopy(raw)
    keys(r,'schema measure as_of common_random_numbers credit_draw_stream policy drivers history interval_ends scenarios')
    require(r['schema']==SCHEMA and r['measure']=='physical','selected Physical schema')
    for d in r['drivers']:
        keys(d,'driver_id metric region_id unit raw_domain availability_lag_days innovation_scale scaling_source scaling_training_end source')
        d['raw_domain']=[finite(v) for v in d['raw_domain']];d['innovation_scale']=finite(d['innovation_scale'])
    h=r['history'];keys(h,'source data_kind training_split holdout_split training_end holdout_end observations')
    for o in h['observations']:
        keys(o,'row_id start end start_values end_values published_at')
        for field in ('start_values','end_values'):o[field]=[finite(v) for v in o[field]]
    draws=r['credit_draw_stream']
    if draws['kind']=='supplied':
        keys(draws,'kind stream_id stream_revision paths')
        for p in draws['paths']:
            keys(p,'path_key pairs')
            for u in p['pairs']:
                keys(u,'waiting_uniform destination_uniform')
                for name in u:u[name]=finite(u[name])
    else:keys(draws,'kind stream_id stream_revision seed')
    for s in r['scenarios']:
        keys(s,'scenario_id kind source conditional_rate_source issued_at paths')
        for p in s['paths']:
            keys(p,'path_key short_rates_percent values')
            p['short_rates_percent']=[finite(v) for v in p['short_rates_percent']]
            p['values']=[[finite(v) for v in row] for row in p['values']]
    return r

def expected(raw, check=lambda:False):
    checkpoint(check);r=normalise(raw);h=r['history'];drivers=r['drivers'];n=len(drivers)
    require(2<=n<=16 and len(h['observations'])<=4096,'bounded dimensions')
    asof=instant(r['as_of']);cut=instant(h['training_end']);end=instant(h['holdout_end'])
    require(cut<end<=asof and h['data_kind'] in ('synthetic','observed'),'chronological split')
    for v in [r['policy'],r['common_random_numbers'],h['source'],h['training_split'],h['holdout_split']]:identity(v)
    require(h['training_split']['id']!=h['holdout_split']['id'],'split identity')
    names=set();coordinates=set();rate=[]
    for j,d in enumerate(drivers):
        checkpoint(check);label(d['driver_id']);label(d['region_id'])
        require(d['driver_id'] not in names and (d['metric'],d['region_id']) not in coordinates,'unique driver')
        names.add(d['driver_id']);coordinates.add((d['metric'],d['region_id']))
        require(METRICS.get(d['metric'])==d['unit'],'metric/unit')
        identity(d['source']);identity(d['scaling_source'])
        integer(d['availability_lag_days'],0,36500)
        require(0<d['innovation_scale']<=1e9 and instant(d['scaling_training_end'])<=cut,'frozen scale')
        bounds=d['raw_domain'];require(len(bounds)==2 and -1e9<=bounds[0]<bounds[1]<=1e9,'driver domain')
        if d['metric']=='used_vehicle_index':require(bounds[0]>0,'positive index')
        if d['metric']=='unemployment_level':require(0<=bounds[0]<bounds[1]<=100,'unemployment domain')
        if d['metric']=='short_rate':rate.append(j)
    require(len(rate)==1,'single conditional rate');rate=rate[0]
    def vector(values):
        require(len(values)==n and all(d['raw_domain'][0]<=x<=d['raw_domain'][1] for x,d in zip(values,drivers)),'bounded vector')
    train=[];hold=[];cross=[];late=[];prior=None;ids=set()
    for o in h['observations']:
        checkpoint(check);label(o['row_id']);require(o['row_id'] not in ids,'unique observation');ids.add(o['row_id'])
        a,b=instant(o['start']),instant(o['end']);require(a<b<=end and b-a<=36600*86400,'observation dates')
        vector(o['start_values']);vector(o['end_values']);require(len(o['published_at'])==n,'release coverage')
        if prior is not None:
            require(prior['end']==o['start'] and digest(prior['end_values'])==digest(o['start_values']),'contiguous history')
        prior=o;available=b
        for published,d in zip(o['published_at'],drivers):
            published=instant(published);require(published>=b,'release chronology')
            available=max(available,published,b+d['availability_lag_days']*86400)
        if a<cut<b:cross.append(o['row_id']);continue
        if available>(cut if b<=cut else end):late.append(o['row_id']);continue
        years=(b-a)/(365*86400)
        x=[(z-y)/(d['innovation_scale']*math.sqrt(years)) for y,z,d in zip(o['start_values'],o['end_values'],drivers)]
        require(all(math.isfinite(v) and abs(v)<=1e6 for v in x),'innovation domain')
        (train if b<=cut else hold).append((o,x))
    def moments(rows):
        require(len(rows)>=2,'two available observations per split')
        mean=[math.fsum(x[j] for _,x in rows)/len(rows) for j in range(n)]
        cov=[[math.fsum((x[i]-mean[i])*(x[j]-mean[j]) for _,x in rows)/(len(rows)-1) for j in range(n)] for i in range(n)]
        return dict(row_ids=[o['row_id'] for o,_ in rows],mean_annualized_standardized_innovation=mean,sample_annual_covariance=cov)
    training,holdout=moments(train),moments(hold)
    stops=[instant(t) for t in r['interval_ends']]
    require(1<=len(stops)<=64 and all(a<b for a,b in zip([asof]+stops,stops)) and stops[-1]-asof<=36600*86400,'projection grid')
    scenarios=r['scenarios'];require(len(scenarios)==4,'four templates')
    require({s['kind'] for s in scenarios}=={'baseline','parallel_up200','parallel_down200','severely_adverse'},'scenario kinds')
    require(len({s['scenario_id'] for s in scenarios})==4,'scenario ids')
    base=next(s for s in scenarios if s['kind']=='baseline');path_keys=[p['path_key'] for p in base['paths']]
    require(1<=len(path_keys)<=16 and len(set(path_keys))==len(path_keys),'path coverage')
    for k in path_keys:integer(k,0,2**64-1)
    draws=r['credit_draw_stream'];require((draws['stream_id'],draws['stream_revision'])==(r['common_random_numbers']['id'],r['common_random_numbers']['revision']),'raw draw identity')
    if draws['kind']=='supplied':
        require(len(draws['paths'])==len(path_keys) and {p['path_key'] for p in draws['paths']}==set(path_keys),'draw coverage')
        for p in draws['paths']:
            integer(p['path_key'],0,2**64-1);require(1<=len(p['pairs'])<=128,'draw count')
            for u in p['pairs']:require(all(0<x<1 for x in u.values()),'uniform domain')
    else:
        require(draws['kind']=='counter_splitmix64_v1','draw kind');integer(draws['seed'],0,2**64-1)
    for s in scenarios:
        checkpoint(check);label(s['scenario_id']);identity(s['source']);identity(s['conditional_rate_source'])
        require(instant(s['issued_at'])<=asof and [p['path_key'] for p in s['paths']]==path_keys,'available paired scenario')
        for p,b in zip(s['paths'],base['paths']):
            integer(p['path_key'],0,2**64-1)
            require(len(p['values'])==len(stops)==len(p['short_rates_percent']),'scenario grid')
            for j,v in enumerate(p['values']):
                vector(v);require(digest(v[rate])==digest(p['short_rates_percent'][j]),'conditional rate parent')
                if s['kind'] in ('parallel_up200','parallel_down200'):
                    want=b['short_rates_percent'][j]+(2. if s['kind']=='parallel_up200' else -2.)
                    require(digest(want)==digest(p['short_rates_percent'][j]),'exact 200bp template')
    diagnostics=dict(training=training,holdout=holdout,holdout_mean_error_from_training=[a-b for a,b in zip(holdout['mean_annualized_standardized_innovation'],training['mean_annualized_standardized_innovation'])],excluded_cross_split=cross,excluded_unavailable_at_split=late)
    train_identity=[SCHEMA,drivers,r['policy'],h['source'],h['training_split'],h['training_end'],h['data_kind'],[o for o,_ in train]]
    return r,diagnostics,digest(train_identity,check)

def verify(raw, result, check=lambda:False):
    keys(result,'schema identity_policy source_sha256 training_identity_sha256 raw_source diagnostics adoption_eligible empirical_accuracy_validated pricing_measure_permission')
    r,diagnostics,training_digest=expected(raw,check)
    require(result['schema']==SCHEMA and result['identity_policy']==POLICY,'selected result identity')
    source_digest=digest(r,check)
    require(digest(result['raw_source'],check)==source_digest==result['source_sha256'],'complete typed raw source binding')
    require(result['training_identity_sha256']==training_digest,'training-only source binding')
    require(all(result[k] is False for k in ['adoption_eligible','empirical_accuracy_validated','pricing_measure_permission']),'permission flags')
    maximum=[0.]
    def compare(actual,want):
        checkpoint(check)
        if type(want) is dict:
            require(type(actual) is dict and set(actual)==set(want),'diagnostic fields')
            for k in want:compare(actual[k],want[k])
        elif type(want) is list:
            require(type(actual) is list and len(actual)==len(want),'diagnostic dimensions')
            for a,b in zip(actual,want):compare(a,b)
        elif type(want) is float:
            actual=finite(actual);error=abs(actual-want);maximum[0]=max(maximum[0],error)
            require(error<=1e-12+1e-12*abs(want),'independent innovation moment')
        else:require(type(actual) is type(want) and actual==want,'diagnostic identity')
    compare(result['diagnostics'],diagnostics);checkpoint(check)
    return dict(schema='consumer-macro-independent-1',verified=True,source_sha256=source_digest,training_identity_sha256=training_digest,maximum_diagnostic_abs_error=maximum[0],scope='macro-source-and-innovation-diagnostics-only',adoption_eligible=False)
