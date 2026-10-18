"""Independent persisted credit verification. Never an execution backend.

SciPy's matrix exponential is independent of native uniformization. The verifier
replays explicit accounting reviews and actual-date boundaries, retaining one
run's source state and closing GL rather than a daily output tensor.
"""
from collections import defaultdict
from copy import deepcopy
import math

import numpy as np
from scipy.linalg import expm

ASSETS = {'cash', 'asset_principal', 'credit_default_principal', 'accrued_interest',
          'credit_default_accrued_interest', 'book_adjustment', 'credit_default_book_adjustment', 'allowance'}
EARNINGS = {'interest_income', 'credit_loss_expense', 'basis_recognition_expense', 'recovery_income'}
LIVE = ('principal', 'undrawn', 'accrued_interest', 'book_adjustment')


def close(a, b):
    return math.isfinite(a) and math.isfinite(b) and math.isclose(a, b, rel_tol=1e-12, abs_tol=1e-9)


def replay_credit_journal(frames):
    """Independent bounded signed-GL replay for downstream verified-source bridges."""
    balances=defaultdict(float);sequence=0;last_day=defaultdict(int)
    current=None;values=[];runs={};count=0
    def check():
        if abs(math.fsum(values))>1e-9*max(1.,math.fsum(abs(v) for v in values)):
            raise ArithmeticError('unbalanced credit journal transaction')
    for frame in frames:
        for row in frame.iter_rows(named=True):
            run=row['run_id'];scope=(row['scenario'],row['severity'])
            if run in runs and runs[run]!=scope:raise ValueError('credit journal run scope changed')
            runs[run]=scope
            key=(run,row['transaction_number'],row['day'],row['ordinal'],row['account'],row['event'])
            if key!=current:
                if current is not None:check()
                sequence+=1
                if row['transaction_number']!=sequence or row['day']<last_day[run]:
                    raise ValueError('credit journal transaction sequence mismatch')
                last_day[run]=row['day'];current=key;values=[]
            value=row['value']
            if not math.isfinite(value):raise ValueError('invalid credit journal amount')
            glkey=(run,row['scenario'],row['severity'],row['account'],row['gl_account'],row['instrument_id'])
            balances[glkey]+=value
            if not math.isfinite(balances[glkey]):raise ArithmeticError('credit journal overflow')
            values.append(value);count+=1
    if current is not None:check()
    return dict(balances),count


def compare(row, expected, sums=None):
    """`sums` maps an aggregate column to (terms, magnitude): the native ledger adds that many
    balances term by term, so its rounding is bounded by terms * eps * magnitude, while this
    reference sums them exactly. Every other column keeps the fixed tolerance."""
    if row.keys() != expected.keys(): raise ArithmeticError('credit persisted columns differ')
    for key, value in expected.items():
        actual = row[key]
        if isinstance(value, (float, np.floating)):
            good = close(actual, value)
            if not good and sums and key in sums:
                terms, magnitude = sums[key]
                good = math.isfinite(actual) and abs(actual - value) <= terms * 1.12e-16 * magnitude
        elif isinstance(value, (list, np.ndarray)):
            good = len(actual) == len(value) and all(close(a, b) for a, b in zip(actual, value))
        else: good = actual == value
        if not good: raise ArithmeticError(f'credit persisted source mismatch: {key}: {actual} != {value}')


def aggregate_sums(balances, account, allowances):
    """Term counts and magnitudes of the native running sums behind each aggregate column."""
    def part(values):
        values = list(values)
        return len(values), math.fsum(abs(v) for v in values)
    cash = part(v for g, v in balances if g == 'cash')
    assets = part(v for g, v in balances if g in ASSETS)
    earnings = part(v for g, v in balances if g in EARNINGS)
    equity = (earnings[0] + 1, earnings[1] + abs(account['equity']))
    return dict(cash=cash, assets=assets, equity=equity, allowance=part(allowances),
                reconciliation_error=(assets[0] + equity[0], assets[1] + equity[1]))


def coefficients(contract):
    q = np.zeros((7, 7)); reward = np.zeros((7, 7))
    for i, hazards in enumerate(contract['transition_hazards_per_year']):
        q[i, :3] = hazards[:3]; q[i, i] = -sum(hazards)
        q[i, 3+i] = hazards[3]; q[i, 6] = hazards[4]
        reward[i, i] = contract['accrual_rates'][i]
    block = np.block([[q, reward], [np.zeros((7, 7)), q]])
    value = expm(block / 365.)
    return value[:7, :7], value[:7, 7:]


def _admit_loan_action(action):
    fields = {
        'maturity': {'kind'},
        'draw': {'kind', 'amount_by_state'},
        'repayment': {'kind', 'principal_fraction', 'accrued_fraction', 'cancel_undrawn_fraction'},
    }
    kind = action.get('kind') if isinstance(action, dict) else None
    expected = fields.get(kind) if isinstance(kind, str) else None
    if expected is None or action.keys() != expected:
        raise ArithmeticError('credit boundary action fields differ from its explicit kind')


def _admit_all_loan_actions(raw, check):
    count = 0
    for run in raw['runs']:
        for source in run['positions']:
            for boundary in source['boundaries']:
                if check():
                    raise InterruptedError('credit source verification cancelled')
                count += 1
                if count > 1_000_000:
                    raise ArithmeticError('credit boundary verification admission exceeded')
                _admit_loan_action(boundary['action'])


class Oracle:
    def __init__(self, source):
        for boundary in source['boundaries']:
            _admit_loan_action(boundary['action'])
        self.source = source; self.c = source['contract']; self.p, self.f = coefficients(self.c)
        opening = source['opening']; self.prob = np.array(opening['probability']); self.matured = 0.
        self.money = np.array([[r[k] for k in LIVE] for r in opening['accounting']['live']])
        self.allowance = opening['accounting']['allowance']
        self.claims = {c['claim_id']: deepcopy(c) for c in opening['accounting']['claims']}
        self.boundaries = defaultdict(list)
        for b in source['boundaries']: self.boundaries[b['recognition_ordinal']].append(b)
        self.reviews = {r['recognition_ordinal']: r for r in source['reviews']}

    def advance(self, ordinal, post, lineage):
        c = self.c; pos = c['position_id']; policy = c['accounting_policy']
        initial = np.zeros((7, 4)); initial[:3] = self.money
        transported = self.p.T @ initial
        transported[:, 2] += initial[:, 0] @ self.f
        earned = float(np.sum(initial[:, 0] @ self.f))
        self.prob = self.prob @ self.p
        self.money = transported[:3].copy()
        loss_total = draw_total = recovery_total = 0.
        def emit(event, lines, claim=None, identity=None):
            posted = post(event, [(gl, '@cash' if gl == 'cash' else claim if gl.startswith('credit_default_') else pos, v)
                                  for gl, v in lines], source=self.source)
            if posted: lineage(self,event,claim,identity or c['economic_source'])
        emit('joint_credit_earned_interest', [('accrued_interest', earned), ('interest_income', -earned)])
        review = self.reviews.get(ordinal, {}); target = review.get('allowance_review')
        def allowance():
            delta = target['target_amount'] - self.allowance
            emit('reviewed_allowance_target', [('credit_loss_expense', delta), ('allowance', -delta)], identity=target['review'])
            self.allowance = target['target_amount']
        if target and target['phase'] == 'before_credit': allowance()
        for i in range(3):
            principal, undrawn, accrued, basis = transported[3+i]
            if all(x == 0. for x in (principal, undrawn, accrued, basis)): continue
            draw = undrawn * c['credit_conversion_factor'][i]; gross = principal + draw
            loss = gross * c['loss_given_default'][i]; claim = f"{c['claim_namespace']}:{ordinal}:{i}"
            loss_total += loss; draw_total += draw
            emit('default_contingent_draw', [('asset_principal', draw), ('cash', -draw)], claim)
            emit('economic_default_transfer', [('asset_principal', -gross), ('credit_default_principal', gross)], claim)
            reverse = policy['accrued'] == 'reverse_income_at_default'
            expense = policy['basis'] == 'recognize_in_earnings_at_default'
            emit('default_accrued_income_reversal' if reverse else 'default_accrued_transfer',
                 [('accrued_interest', -accrued), ('interest_income' if reverse else 'credit_default_accrued_interest', accrued)], claim)
            emit('default_basis_earnings' if expense else 'default_basis_transfer',
                 [('book_adjustment', -basis), ('basis_recognition_expense' if expense else 'credit_default_book_adjustment', basis)], claim)
            self.claims[claim] = dict(claim_id=claim, economic_source=c['economic_source'],
                recovery_schedule_review=c['recovery_schedule_review'], default_ordinal=ordinal,
                initial_principal=gross, economic_principal_loss=loss, outstanding_principal=gross,
                outstanding_accrued_interest=0. if reverse else accrued, outstanding_book_adjustment=0. if expense else basis,
                principal_charged_off=0., recovery_cash_collected=0., recovery_applied_to_principal=0.,
                remaining_recoveries=[dict(payment_ordinal=ordinal+p['lag_days'], amount=(gross-loss)*p['weight'])
                                      for p in c['complete_recovery_tails'][i]])
        for instruction in review.get('chargeoffs', []):
            claim = self.claims[instruction['claim_id']]; name = claim['claim_id']
            principal = instruction['principal_amount']; basis = instruction['book_adjustment_amount']
            accrued = instruction['accrued_interest_reversal']; covered = min(principal+basis, self.allowance)
            emit('reviewed_credit_chargeoff', [('credit_default_principal', -principal),
                ('credit_default_book_adjustment', -basis), ('allowance', covered),
                ('credit_loss_expense', principal+basis-covered)], name, instruction['review'])
            emit('reviewed_accrued_income_reversal', [('credit_default_accrued_interest', -accrued), ('interest_income', accrued)], name, instruction['review'])
            claim['outstanding_principal'] -= principal; claim['outstanding_book_adjustment'] -= basis
            claim['outstanding_accrued_interest'] -= accrued; claim['principal_charged_off'] += principal
            self.allowance -= covered
        for claim in self.claims.values():
            due = math.fsum(p['amount'] for p in claim['remaining_recoveries'] if p['payment_ordinal'] == ordinal)
            applied = min(due, claim['outstanding_principal']); excess = due-applied
            contra = excess if policy['excess_cash_recovery'] == 'credit_allowance' else 0.
            emit('modeled_physical_recovery_cash', [('cash', due), ('credit_default_principal', -applied),
                 ('allowance', -contra), ('recovery_income', -(excess-contra))], claim['claim_id'], claim['recovery_schedule_review'])
            recovery_total += due
            claim['outstanding_principal'] -= applied; claim['recovery_cash_collected'] += due
            claim['recovery_applied_to_principal'] += applied; self.allowance += contra
            claim['remaining_recoveries'] = [p for p in claim['remaining_recoveries'] if p['payment_ordinal'] > ordinal]
        if target and target['phase'] == 'after_credit': allowance()
        principal, _, accrued, basis = transported[6]
        emit('economic_prepay_right_settlement', [('cash', principal+accrued), ('asset_principal', -principal),
             ('accrued_interest', -accrued), ('book_adjustment', -basis), ('basis_recognition_expense', basis)])
        lineage(self,'joint_credit_interval',None,c['economic_source'],
                event_id=f"{c['claim_namespace']}:{ordinal}:interval",totals=dict(earned_interest=earned,
                economic_principal_loss=loss_total,contingent_draw_cash=draw_total,prepay_cash=float(principal+accrued),recovery_cash=recovery_total))
        for boundary in self.boundaries[ordinal]:
            action = boundary['action']; kind = action['kind']; principal = accrued = basis = draw = 0.
            if kind == 'draw':
                funded = np.array(action['amount_by_state']); self.money[:, 0] += funded; self.money[:, 1] -= funded
                draw = float(np.sum(funded))
            else:
                pf = np.ones(3) if kind == 'maturity' else np.array(action['principal_fraction'])
                af = np.ones(3) if kind == 'maturity' else np.array(action['accrued_fraction'])
                uf = np.ones(3) if kind == 'maturity' else np.array(action['cancel_undrawn_fraction'])
                principal = float(np.sum(self.money[:, 0]*pf)); accrued = float(np.sum(self.money[:, 2]*af))
                basis = float(np.sum(self.money[:, 3]*pf))
                self.money[:, 0] *= 1-pf; self.money[:, 1] *= 1-uf
                self.money[:, 2] *= 1-af; self.money[:, 3] *= 1-pf
                if kind == 'maturity': self.matured += float(np.sum(self.prob[:3])); self.prob[:3] = 0.
            post(boundary['event_id'], [('cash','@cash',principal+accrued-draw), ('asset_principal',pos,draw-principal),
                 ('accrued_interest',pos,-accrued), ('book_adjustment',pos,-basis), ('basis_recognition_expense',pos,basis)],source=self.source)
            event={'draw':'scheduled_ordinary_draw','repayment':'scheduled_surviving_repayment','maturity':'contractual_maturity'}[kind]
            lineage(self,event,None,boundary['source'],event_id=boundary['event_id'],policy=boundary['policy'])


def verify_credit_sources(raw, spec, writer, check):
    """Replay every source state, complete claim and ordered posting independently."""
    # Validate even future boundaries before any persisted row or financial replay.
    _admit_all_loan_actions(raw, check)
    def rows(table):
        for frame in writer.frames(table):
            check()
            yield from frame.iter_rows(named=True)
    streams = {table: iter(rows(table)) for table in ('journal','credit_states','credit_claims','credit_lineage','daily_balance','trial_balance','summary')}
    def take(table, expected, sums=None):
        got = next(streams[table], None)
        if got is None: raise ArithmeticError(f'missing credit {table} evidence')
        compare(got, expected, sums)
    transaction = 0; state_count = claim_count = 0
    accounts = [vars(a) for a in spec['accounts']]
    horizon = spec['horizon_days']; asof = raw['as_of_ordinal']; end = asof+horizon
    for run in raw['runs']:
        check(); gl = defaultdict(float); sources = [Oracle(p) for p in run['positions']]
        labels = dict(run_id=run['run_id'], scenario=run['scenario_id'], severity=run['severity'])
        day = 0; ordinal = asof; account_id = ''
        def post(event, lines, source=None):
            nonlocal transaction, account_id
            lines = [(g,i,float(v)) for g,i,v in lines if v != 0.]
            if not lines: return False
            transaction += 1
            if source is not None: account_id = source['account_id']
            # Exact posting order is retained; source amounts use independent math.
            for name, instrument, value in lines:
                take('journal', dict(labels, transaction_number=transaction, day=day, ordinal=ordinal,
                    account=account_id, event=event, gl_account=name, instrument_id=instrument, value=value))
                gl[account_id,name,instrument] += value
            if not close(math.fsum(v for _,_,v in lines),0.): raise ArithmeticError('independent credit transaction unbalanced')
            return True
        def lineage(o,event,claim,identity,*,event_id=None,policy=None,totals=None):
            c=o.c; policy=policy or c['accounting_policy']['identity']
            values=dict.fromkeys(('earned_interest','economic_principal_loss','contingent_draw_cash','prepay_cash','recovery_cash'),0.)
            if totals: values.update(totals)
            take('credit_lineage',dict(labels,day=day,ordinal=ordinal,account=o.source['account_id'],position=c['position_id'],
                event_id=event_id or f"{c['claim_namespace']}:{ordinal}:{event}:{claim or 'live'}",event=event,
                source_id=identity['id'],source_revision=identity['revision'],policy_id=policy['id'],policy_revision=policy['revision'],
                claim_id=claim,**values))
        for account in accounts:
            account_id = account['id']; lines = [('cash','@cash',account['cash']), ('equity_opening','@equity',-account['equity'])]
            for o in sources:
                if o.source['account_id'] != account_id: continue
                pos=o.c['position_id']; totals=np.sum(o.money,axis=0)
                lines += [('asset_principal',pos,totals[0]),('accrued_interest',pos,totals[2]),('book_adjustment',pos,totals[3]),('allowance',pos,-o.allowance)]
                for claim in o.claims.values():
                    lines += [(f'credit_default_{k}',claim['claim_id'],claim[f'outstanding_{k}']) for k in ('principal','accrued_interest','book_adjustment')]
            post('opening_balance',lines)
        max_error = 0.
        max_sum=(0,0.)
        for day in range(1,horizon+1):
            check(); ordinal=asof+day
            for o in sources:
                o.advance(ordinal,post,lineage); pos=o.c['position_id']; account_id=o.source['account_id']
                base=dict(labels,day=day,ordinal=ordinal,account=account_id,position=pos)
                defaults={k:math.fsum(c[f'outstanding_{k}'] for c in o.claims.values()) for k in ('principal','accrued_interest','book_adjustment')}
                take('credit_states',dict(base,contract_id=o.c['identity']['id'],contract_revision=o.c['identity']['revision'],
                    probability=o.prob,matured_probability=o.matured,live_principal=o.money[:,0],live_undrawn=o.money[:,1],
                    live_accrued_interest=o.money[:,2],live_book_adjustment=o.money[:,3],allowance=o.allowance,
                    **{f'default_{k}':v for k,v in defaults.items()}));state_count+=1
                for claim in o.claims.values():
                    expected=dict(base,claim_id=claim['claim_id'],economic_source_id=claim['economic_source']['id'],
                        economic_source_revision=claim['economic_source']['revision'],recovery_review_id=claim['recovery_schedule_review']['id'],
                        recovery_review_revision=claim['recovery_schedule_review']['revision'],default_ordinal=claim['default_ordinal'],
                        **{k:claim[k] for k in ('initial_principal','economic_principal_loss','outstanding_principal','outstanding_accrued_interest',
                            'outstanding_book_adjustment','principal_charged_off','recovery_cash_collected','recovery_applied_to_principal')},
                        remaining_recovery_cash=math.fsum(p['amount'] for p in claim['remaining_recoveries']),
                        recovery_tail_after_reporting=math.fsum(p['amount'] for p in claim['remaining_recoveries'] if p['payment_ordinal']>end))
                    take('credit_claims',expected);claim_count+=1
            for account in accounts:
                name=account['id']; balances=[(g,v) for (a,g,_),v in gl.items() if a==name]
                cash=math.fsum(v for g,v in balances if g=='cash'); assets=math.fsum(v for g,v in balances if g in ASSETS)
                equity=account['equity']-math.fsum(v for g,v in balances if g in EARNINGS)
                allowance=math.fsum(o.allowance for o in sources if o.source['account_id']==name)
                error=abs(assets-equity);max_error=max(max_error,error)
                sums=aggregate_sums(balances,account,[o.allowance for o in sources if o.source['account_id']==name])
                max_sum=max(max_sum,sums['reconciliation_error'],key=lambda x:x[0]*x[1])
                take('daily_balance',dict(labels,day=day,ordinal=ordinal,account=name,entity=account['entity'],currency=account['currency'],
                    cash=cash,assets=assets,equity=equity,allowance=allowance,reconciliation_error=error),sums)
        for account in accounts:
            name=account['id']
            for (_,g,i),value in sorted((k,v) for k,v in gl.items() if k[0]==name):
                take('trial_balance',dict(labels,account=name,gl_account=g,instrument_id=i,balance=value))
            balances=[(g,v) for (a,g,_),v in gl.items() if a==name]
            sums=aggregate_sums(balances,account,[])
            take('summary',dict(labels,account=name,final_cash=math.fsum(v for g,v in balances if g=='cash'),
                final_assets=math.fsum(v for g,v in balances if g in ASSETS),
                final_equity=account['equity']-math.fsum(v for g,v in balances if g in EARNINGS),max_reconciliation_error=max_error),
                dict(final_cash=sums['cash'],final_assets=sums['assets'],final_equity=sums['equity'],max_reconciliation_error=max_sum))
    for table, stream in streams.items():
        if next(stream,None) is not None: raise ArithmeticError(f'unexpected extra credit {table} evidence')
    check()
    return dict(journal_replayed=True,credit_source_reconciled=True,credit_states=state_count,credit_claim_rows=claim_count,
                recovery_tail_retained=True,producer_empirically_validated=False)
