from . import liquidity_policy_reference as liquidity_policy
from ._deposit_continuation_calendar import *
from ._deposit_continuation_calendar import _check,_cumulative,_rights,_path_rights,_side,_principal,_rights_rows,_observe,_decode,_equal
from . import _deposit_continuation_income_reference as c
def _replay(raw, spec, events, acquisitions, end, pool_for_event, coupon_signs, check, income_paths, emit=None):
    """Own the complete journal ordering; producer postings are never inputs."""
    dated = raw['backbook']['dated']; asof = dated['as_of_ordinal']; horizon = end-asof
    run_by_scene = {r['scenario_id']:r for r in dated['runs']}
    scenarios = ['baseline']+[s['name'] for s in spec['scenarios']]
    output = {k:[] for k in ('journal', 'trial_balance', 'closing_statements', 'daily_balance',
        'dated_rights', 'event_lineage', 'acquisition_lineage', 'exposures', 'ledger',
        'reporting_rights', 'reported_daily_balance')}
    streamed={'journal','event_lineage','dated_rights','deposit_principal_lineage','deposit_continuation_state','exposures','daily_balance'}
    if emit is not None:
        class Emission(list):
            def __init__(self,name):self.name=name
            def append(self,value):emit('path' if self.name=='daily_balance' else self.name,value)
            def extend(self,values):
                for value in values:self.append(value)
            def __iadd__(self,values):self.extend(values);return self
        for name in streamed:output[name]=Emission(name)
    all_observations=[]
    for scenario in scenarios:
        _check(check); run = run_by_scene[scenario]; runid = run['run_id']
        positions = deepcopy(spec['positions']); by_id = {p['id']:p for p in positions}
        accounts = deepcopy(spec['accounts']); account_by_id = {a['id']:a for a in accounts}
        selected = [e for e in events if e['selector'] in {None, runid}]
        selected_acq = [a for a in acquisitions if a['run_id'] == runid]
        rights = {p['id']:[0., 0., 0., 0.] for p in positions}; accrued = {p['id']:p['opening_accrued'] for p in positions}
        state = []; pools = {a['event_id']:0. for a in selected_acq}; gl = {}; sequence = 0
        aggregates = {}
        for e in selected:
            interest = _cumulative(e['segments'], asof, 'accrued_interest'); book = _cumulative(e['segments'], asof, 'book_amortization')
            state.append(dict(earned=interest, book=book, settled=False))
            values = _rights(interest, 0., False)
            for i, value in enumerate(values):
                rights[e['event']['position_id']][i] += value
        for p in positions:
            _equal(sum(rights[p['id']]), _side(p)*p['opening_accrued'], 'mixed/opening_source_rights')

        def post(day, account, event, lines, cash=0., earnings=0., position='', aggregate=True):
            nonlocal sequence
            lines = list(lines)
            if aggregate:
                lines += [('cash', '', cash), ('pnl:'+event, position, -earnings)]
            lines = [line for line in lines if line[2] != 0.]
            if lines:
                if abs(math.fsum(l[2] for l in lines)) > 1e-9*max(1., sum(abs(l[2]) for l in lines)):
                    raise ArithmeticError('mixed independent journal event fails balance')
                sequence += 1
                for code, instrument, value in lines:
                    key = account, code, instrument
                    gl[key] = product._finite(gl.get(key, 0.)+value)
                    output['journal'].append(dict(transaction_number=sequence, scenario=scenario, day=day,
                        account=account, event=event, gl_account=code, instrument_id=instrument, value=value))
            if aggregate:
                a = account_by_id[account]; a['cash'] += cash; a['equity'] += earnings
                sums = aggregates.setdefault((account, event), [0., 0.]); sums[0] += cash; sums[1] += earnings
            return sequence if lines else None

        for a in accounts:
            lines = [('cash', '', a['cash']), ('opening_equity', '', -a['equity'])]
            for p in positions:
                if p['account'] == a['id']:
                    lines += [(_principal(p), p['id'], _side(p)*p['balance']),
                              ('book_adjustment', p['id'], _side(p)*p['book_adjustment']),
                              ('allowance', p['id'], -p['allowance'])]
                    lines += [(code, p['id'], value) for code, value in zip(RIGHTS, rights[p['id']], strict=True)]
            post(0, a['id'], 'opening', lines, aggregate=False)

        atom_cursor=0
        def source_day(day,ordinal):
            nonlocal atom_cursor
            source=income_paths[runid]; current=source['native']['current']; pos=positions[0]
            while atom_cursor<len(current['atoms']) and current['atoms'][atom_cursor]['ordinal']<=ordinal:
                atom=current['atoms'][atom_cursor]; transition=current['transitions'][atom_cursor]
                c.exact(atom['ordinal'],ordinal);c.exact(pos['balance'],atom['principal_before'])
                tx=post(day,pos['account'],'source_principal_basis_reclassification',
                    [('funding_principal',pos['id'],-transition['principal_partition_delta']),
                     ('book_adjustment',pos['id'],-transition['basis_partition_delta'])],aggregate=False)
                pos['balance']=atom['principal_after'];pos['book_adjustment']+=transition['basis_partition_delta'];atom_cursor+=1
                output.setdefault('deposit_principal_lineage',[]).append(dict(scenario=scenario,run_id=runid,ordinal=ordinal,day=day,
                    position_id=pos['id'],allocation_index=atom['allocation_index'],source_atom=deepcopy(atom),transition=deepcopy(transition),
                    review=deepcopy(source['native']['raw_request']['review']),transaction_number=tx,cash=0.,income=0.,
                    source_owned_principal=True,capital_ftp_eligible=False))
            continuation=source['native']['raw_request']['continuation']
            if ordinal==continuation['revision_ordinal']:
                revision=source['native']['basis_revision']
                post(day,pos['account'],'deposit_basis_revision',[('book_adjustment',pos['id'],-revision)],earnings=-revision,position=pos['id'])
                pos['book_adjustment']+=revision
                output.setdefault('deposit_revision',[]).append(dict(scenario=scenario,run_id=runid,ordinal=ordinal,day=day,
                    position_id=pos['id'],basis_revision=revision,revised_basis=pos['book_adjustment'],review=deepcopy(continuation['policy']['review']),
                    cash=0.,original_yield_retained=True,accounting_standard_approval=False,capital_ftp_eligible=False))
            if day:
                row=current['income']['rows'][day-1];a=-sum(rights[pos['id']])
                for actual,want in [(pos['balance'],row['closing_principal']),(pos['book_adjustment'],row['closing_book_adjustment']),
                    (a,row['closing_accrued_interest'])]:c.near(actual,want)
                output.setdefault('deposit_continuation_state',[]).append(dict(scenario=scenario,run_id=runid,ordinal=ordinal,day=day,
                    position_id=pos['id'],principal=pos['balance'],book_adjustment=pos['book_adjustment'],accrued_interest=a,
                    gross_carrying_value=pos['balance']+pos['book_adjustment']+a,gross_rights=deepcopy(rights[pos['id']]),
                    source_income=deepcopy(row),source_atom_cursor=atom_cursor,original_yield_anchor=deepcopy(continuation['anchor']['original_yield_anchor']),
                    accounting_standard_approval=False,capital_ftp_eligible=False))

        def apply_event(index, day, interest, book, settlement):
            e = selected[index]; event = e['event']; v = state[index]; pos = by_id[event['position_id']]
            pool = pool_for_event.get((runid, event['event_id']))
            before = (_path_rights(_side(pos), coupon_signs[pool], pools[pool]) if pool is not None
                      else _rights(v['earned'], event['components']['cash_interest'], v['settled']))
            if settlement and v['settled']:
                raise ArithmeticError('mixed duplicate settlement')
            v['earned'] += interest; v['book'] += book
            if settlement:
                v['settled'] = True
            if pool is not None:
                pools[pool] += interest-(event['components']['cash_interest'] if settlement else 0.)
                after = _path_rights(_side(pos), coupon_signs[pool], pools[pool])
            else:
                after = _rights(v['earned'], event['components']['cash_interest'], v['settled'])
            delta = [a-b for a, b in zip(after, before, strict=True)]
            for i, value in enumerate(delta):
                rights[pos['id']][i] += value
            accrued[pos['id']] = _side(pos)*sum(rights[pos['id']])
            pc = event['components']['principal'] if settlement else 0.; ic = event['components']['cash_interest'] if settlement else 0.
            if abs(pc) > pos['balance']+1e-9:
                raise ArithmeticError('mixed settlement exceeds live principal')
            pos['book_adjustment'] += _side(pos)*book
            lines = [(_principal(pos), pos['id'], -_side(pos)*abs(pc)), ('book_adjustment', pos['id'], book)]
            lines += [(code, pos['id'], value) for code, value in zip(RIGHTS, delta, strict=True)]
            tx = post(day, pos['account'], 'dated_settlement' if settlement else 'dated_recognition', lines,
                      cash=pc+ic, earnings=interest+book, position=pos['id'])
            owner = e['owner']; provenance = event['provenance']
            output['event_lineage'].append(dict(scenario=scenario, run_id=runid, input_revision=run['input_revision'], day=day,
                transaction_number=tx, position_id=pos['id'], event_id=event['event_id'], phase='settlement' if settlement else 'recognition',
                source_id=owner['source_id'], source_revision=owner['source_revision'],
                provenance_json=json.dumps(provenance, separators=(',', ':'), ensure_ascii=False), measure=event['measure'], ownership=owner['ownership'],
                policy_id=raw['backbook']['recognition_policy']['policy_id'], policy_revision=raw['backbook']['recognition_policy']['policy_revision'],
                principal=pc, cash_interest=ic, accrued_interest=interest, book_amortization=book, fees=0.))

        for day in range(horizon+1):
            _check(check); ordinal = asof+day
            for a in selected_acq:
                if a['ordinal'] != ordinal:
                    continue
                pos = by_id[a['position_id']]; side = _side(pos); basis = a['cost']-a['principal']
                pos['balance'] += a['principal']; pos['book_adjustment'] += basis
                tx = post(day, pos['account'], 'dated_acquisition',
                          [(_principal(pos), pos['id'], side*a['principal']), ('book_adjustment', pos['id'], side*basis)],
                          cash=-side*a['cost'], position=pos['id'])
                output['acquisition_lineage'].append(dict(scenario=scenario, severity=1., run_id=runid, day=day, ordinal=ordinal,
                    position_id=pos['id'], account=pos['account'], event_id=a['event_id'], source=a['source'], path_id=a['path_id'],
                    source_weight=a['source_weight'], allocation_factor=a['allocation_factor'], original_yield_anchor=a['original_yield_anchor'],
                    measure='physical', policy=raw.get('policy',raw['candidate']['policy'])['identity'], principal=a['principal'], cost=a['cost'], basis=basis, transaction_number=tx))
            if day:
                for i, e in enumerate(selected):
                    interest = book = 0.; active = False
                    for s in e['segments']:
                        if s['start_ordinal'] <= ordinal-1 < s['end_ordinal']:
                            n = s['end_ordinal']-s['start_ordinal']; f0 = (ordinal-1-s['start_ordinal'])/n; f1 = (ordinal-s['start_ordinal'])/n
                            interest += s['accrued_interest']*f1-s['accrued_interest']*f0
                            book += s['book_amortization']*f1-s['book_amortization']*f0; active = True
                    if active:
                        apply_event(i, day, interest, book, False)
            for i, e in enumerate(selected):
                if e['event']['payment_ordinal'] == ordinal:
                    apply_event(i, day, 0., 0., True)
            source_day(day,ordinal)
            for account in accounts:
                if account['cash'] < account['cash_floor']-1e-9:
                    raise ArithmeticError('end-of-day cash floor breached; replacement funding unsupported')
            rows = _observe(spec, positions, accounts, rights, accrued, scenario, day)
            all_observations.extend(rows)
            if day <= 30 or day % 30 == 0 or day == horizon or day == spec['horizon_days']:
                output['daily_balance'] += rows
            if ordinal == asof+spec['horizon_days']:
                output['reported_daily_balance'] += rows
            if day == 0 or day % 30 == 0 or day == horizon:
                for p in positions:
                    output['exposures'].append(dict(scenario=scenario, day=day, account=p['account'], position=p['id'], kind=p['kind'],
                        balance=p['balance'], allowance=p['allowance'], watch_fraction=p['watch_fraction'], commitment=p['commitment'],
                        market_factor=1., encumbered_fraction=p['encumbered_fraction'], accrued_interest=accrued[p['id']]))
            for (account, event), sums in aggregates.items():
                if any(v != 0. for v in sums):
                    output['ledger'].append(dict(scenario=scenario, day=day, account=account, event=event, cash=sums[0], earnings=sums[1], aoci=0., memo_amount=0.))
            aggregates.clear()
        trial = [dict(scenario=scenario, account=a, gl_account=g, instrument_id=i, balance=v) for (a, g, i), v in sorted(gl.items())]
        output['dated_rights'] += _rights_rows(selected,state,scenario,runid,end,pools,pool_for_event,selected_acq)
        output['trial_balance'] += trial
        closing, _ = statements(trial, {a['id']:_decode(Account, a) for a in spec['accounts']}, [scenario])
        output['closing_statements'] += closing
        for p in positions:
            _equal(p['balance'], 0., 'mixed/complete_principal_tail')
            _equal(p['book_adjustment'], 0., 'mixed/complete_basis_tail')
            for value in rights[p['id']]:
                _equal(value, 0., 'mixed/complete_gross_right_tail')
    output.pop('reporting_rights',None);output.pop('reported_daily_balance',None);output.pop('acquisition_lineage',None)
    output['summary']=[]
    output['breaches']=[]
    for scenario in scenarios:
        first=set()
        for row in all_observations:
            if row['scenario']!=scenario:continue
            for metric in ('cash','cet1','leverage','lcr','nsfr','htm'):
                key=(row['account'],metric)
                if row[metric+'_headroom'] is not None and row[metric+'_headroom'] < -1e-9 and key not in first:
                    first.add(key);output['breaches'].append(dict(scenario=scenario,account=row['account'],day=row['day'],metric=metric))
    for scenario in scenarios:
        for account in spec['accounts']:
            rows=[x for x in all_observations if x['scenario']==scenario and x['account']==account['id']]
            last=rows[-1];minimum=min([account['cash']]+[x['cash'] for x in rows])
            summary=dict(scenario=scenario,account=account['id'],entity=account['entity'],currency=account['currency'],final_cash=last['cash'],final_equity=last['equity'],minimum_cash=minimum,peak_cash_shortfall=max(0.,account['cash_floor']-minimum),max_reconciliation_error=max(abs(x['reconciliation_error']) for x in rows))
            for label,metric in [('cash','cash'),('capital','cet1'),('leverage','leverage'),('lcr','lcr'),('nsfr','nsfr'),('htm','htm')]:
                summary['first_'+label+'_breach_day']=next((x['day'] for x in rows if x[metric+'_headroom'] is not None and x[metric+'_headroom'] < -1e-9),None)
            liquidity_policy.apply_summary(summary, account, rows)
            output['summary'].append(summary)
    output['consolidated']=[{k:x[k] for k in ('scenario','currency','assets','liabilities','equity','cash','restricted_cash')} for x in output['closing_statements']]
    totals={}
    for row in output['ledger']:
        sums=totals.setdefault((row['scenario'],row['account'],row['event']),[0.,0.,0.])
        for i,k in enumerate(('cash','earnings','aoci')):sums[i]+=row[k]
    output['attribution']=[]
    keys=sorted({(a,e) for _,a,e in totals})
    for scenario in scenarios[1:]:
        for a,event in keys:
            old=totals.get(('baseline',a,event),[0.,0.,0.]);new=totals.get((scenario,a,event),[0.,0.,0.])
            if ('baseline',a,event) in totals or (scenario,a,event) in totals:
                output['attribution'].append(dict(scenario=scenario,account=a,event=event,cash_delta=new[0]-old[0],earnings_delta=new[1]-old[1],aoci_delta=new[2]-old[2]))
    return output
