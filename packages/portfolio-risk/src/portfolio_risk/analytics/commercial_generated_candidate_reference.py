"""Independent raw generated acquisition and partitioned commercial journal gate.

Native receipts confer no authority: laws, original training policies, stopped
cash, original acquisition anchors and coefficients are recomputed. Temporary
SQLite sequences keep complete accounting groups and comparison rows off heap;
only the current path/group and current daily path snapshots are expanded.
"""
from collections.abc import Sequence
from copy import deepcopy
from dataclasses import fields
import itertools
import json
import math
from types import SimpleNamespace

from ._commercial_generated_spool import replay_store, SnapshotMap
from .bounded_native_records import loads
from .commercial_candidate_reference import allocation
from .commercial_credit_reference import full_end
from .commercial_dto import graph_typed
from .commercial_generated_graph_reference import accounting_spool
from .commercial_generated_transport import SCHEMA, WORKFLOW, METADATA_TABLES, SCHEMAS, validate_trailer
from .commercial_graph_reference import verify_units, verify_graph
from .commercial_ledger_reference import verify_stream as verify_journal
from .commercial_reports_reference import normalize_specification, scope, reports
from .commercial_source_reference import compare
from .modeled_json_identity import exact_json_identity as exact


def _rows(frames, name, check):
    for frame in frames.frames(name):
        check()
        for row in frame.iter_rows(named=True):
            check()
            yield row


def _elapsed(year):
    value = year * 365.
    rounded = round(value)
    day = rounded if rounded / 365. == year else math.ceil(value)
    return day + (year > day / 365.)


def verify_preparation(raw, prepared, receipt, check=lambda: False):
    check()
    wanted = set('original_request allocation_replay admitted_work_upper retained_bytes_upper reporting_end_ordinal full_end_ordinal physical_paths maximum_live_full_physical_owners shared_daily_sim_adoption graph_exercise_source_adoption opening_identity_verified mixed_ledger_adoption empirical_validation'.split())
    if raw.get('schema') != SCHEMA or set(receipt) != wanted or not exact(raw, receipt['original_request']):
        raise ArithmeticError('generated immutable original request/receipt differs')
    for name, value in dict(maximum_live_full_physical_owners=1,
            shared_daily_sim_adoption=True, graph_exercise_source_adoption=True,
            opening_identity_verified=False, mixed_ledger_adoption=False, empirical_validation=False).items():
        if not exact(receipt[name], value):
            raise ArithmeticError('generated source ownership/eligibility differs/' + name)
    for key, maximum in [('admitted_work_upper', 4_000_000_000), ('retained_bytes_upper', 128 << 20)]:
        if type(receipt[key]) is not int or not 0 < receipt[key] <= maximum:
            raise ArithmeticError('generated exact admission integer differs')
    units = raw['commercial']['units']
    if not 1 <= len(units) <= 32 or len(units) != len(raw['owners']) or len(units) != len(raw['allocation']):
        raise ArithmeticError('generated complete unit/owner/allocation coverage differs')
    normalized = normalize_specification(raw['specification'])
    count = 0
    horizon = max(30, raw['reporting_end_ordinal'] - raw['as_of_ordinal'])
    if not horizon <= normalized['horizon_days'] <= 1080:
        raise ArithmeticError('generated explicit outer journal calendar identity differs')
    horizon = normalized['horizon_days']
    scenarios = {'baseline'} | {s['name'] for s in normalized['scenarios']}
    mapping = {m['source_scenario_id']: m for m in raw['run_mapping']}
    if (len(mapping) != len(raw['run_mapping']) or len(mapping) != len(scenarios)
            or {m['ledger_scenario_id'] for m in mapping.values()} != scenarios):
        raise ArithmeticError('generated complete unique source/run mapping differs')
    positions = {p['id']: p for p in normalized['positions']}
    if len(positions) != len(units) or set(positions) != {o['position_id'] for o in raw['owners']}:
        raise ArithmeticError('generated exclusive position ownership differs')
    for unit, owner, allocated in zip(units, raw['owners'], raw['allocation'], strict=True):
        check()
        graph = unit['graph']
        if len(graph['positions']) != 1 or not graph['positions'][0].get('generated_policy'):
            raise ArithmeticError('generated selected native policy required')
        position = graph['positions'][0]
        if position.get('fees'):
            raise ArithmeticError('generated fee adoption requires explicit same-policy economics')
        offset = graph['as_of_ordinal'] - raw['as_of_ordinal']
        if offset < 0 or graph['reporting_ordinals'][0] != graph['as_of_ordinal']:
            raise ArithmeticError('generated original purchase offset differs')
        if (owner['unit_id'] != unit['unit_id'] or allocated['unit_id'] != unit['unit_id']
                or owner['ownership'] != 'exclusive_native_commercial_graph-1'
                or owner['accounting_category'] != unit['accounting_category'] or unit['accounting_category'] != 'AC'
                or owner['ledger_classification'] != 'ac'
                or any(owner[k] != 0 for k in ('pre_purchase_principal', 'pre_purchase_accrued', 'pre_purchase_basis'))):
            raise ArithmeticError('generated acquired owner/classification differs')
        face = allocated['face']
        if type(face) not in (int, float) or not math.isfinite(face) or not 0 <= face <= 1e15:
            raise ArithmeticError('generated allocated face domain differs')
        spec_position = positions[owner['position_id']]
        issuer = position['credit_identity'] == 'issuer_credit_debt'
        if (spec_position['account'] != owner['account_id'] or spec_position['classification'] != 'ac'
                or spec_position['kind'] != ('security' if issuer else 'loan')
                or spec_position['opening_market_price'] != 1.
                or any(spec_position[k] != 0 for k in ('balance','duration','book_adjustment','opening_accrued','allowance','commitment','rate','annual_pd','migration_rate','risk_weight','rsf_weight','hqla_weight','lcr_inflow_weight','encumbered_fraction','asf_weight','lcr_outflow_weight'))):
            raise ArithmeticError('generated zero placeholder/source ownership differs')
        if len(graph['scenarios']) != len(mapping):
            raise ArithmeticError('generated graph scenario coverage differs')
        for scene in graph['scenarios']:
            if scene['scenario_id'] not in mapping or float(scene['severity']) != float(mapping[scene['scenario_id']]['severity']):
                raise ArithmeticError('generated source/run severity differs')
        generated = position['generated_policy']
        from .commercial_generated_physical_reference import validate as validate_inputs,current as role_inputs,source as physical_source
        validate_inputs(generated,check)
        for law in generated['current_physical']:
            if issuer and (law['source']['credit'].get('opening_undrawn_commitment', 0.) != 0.
                    or law['source'].get('performing_draw_tranches')):
                raise ArithmeticError('generated issuer security cannot own facility draws')
            paths = law['evaluation']['paths']
            if type(paths) is not int or not 128 <= paths <= 512:
                raise ArithmeticError('generated physical sample count differs')
            count += paths
            inputs=role_inputs(generated,law)
            if inputs is None:horizon=max(horizon,offset+_elapsed(full_end(law['source'])))
            else:
                for index in range(paths):horizon=max(horizon,offset+_elapsed(full_end(physical_source(law,inputs,index))))
    normalized['horizon_days'] = horizon
    from ._deposit_continuation_models import Account, Position, Scenario
    for key, cls in [('accounts', Account), ('positions', Position), ('scenarios', Scenario)]:
        for row in normalized[key]:
            for f in fields(cls):
                if f.type is float:
                    row[f.name] = float(row[f.name])
    compare(normalized, prepared, check=check)
    for key, value in [('reporting_end_ordinal', raw['reporting_end_ordinal']),
            ('full_end_ordinal', raw['as_of_ordinal'] + horizon), ('physical_paths', count)]:
        if not exact(receipt[key], value):
            raise ArithmeticError('generated source calendar/path coverage differs/' + key)
    spec = {k: [SimpleNamespace(**v) for v in vs] if k in ('accounts','positions','scenarios') else deepcopy(vs)
            for k, vs in normalized.items()}
    scope(spec, positions)
    return dict(raw=deepcopy(raw), receipt=deepcopy(receipt), spec=spec, horizon=horizon,
        verified=True, preparation_identity_replayed=True)


class Graphs(Sequence):
    def __init__(self, raw, frames, check):
        self.raw, self.frames, self.check = raw, frames, check
    def __len__(self):
        return len(self.raw['commercial']['units'])
    def metadata(self, table, unit):
        found = 0
        for row in _rows(self.frames, table, self.check):
            if row['unit_index'] != unit:
                continue
            if row['source_index'] != found:
                raise ArithmeticError('generated metadata missing/duplicated/out-of-order source')
            found += 1
            yield loads(row['native_json'].encode())
    def __getitem__(self, unit):
        if not 0 <= unit < len(self):
            raise IndexError(unit)
        metadata = list(self.metadata('commercial_candidate_graphs', unit))
        if len(metadata) != 1:
            raise ArithmeticError('generated graph metadata coverage differs')
        graph = metadata[0]
        graph['raw_source'] = graph_typed(self.raw['commercial']['units'][unit]['graph'])
        for table, name in [('commercial_candidate_native_sources','native_sources'),
                ('commercial_candidate_original_sources','original_physical_sources'),
                ('commercial_candidate_original_acquisition_proofs','original_acquisition_proofs')]:
            graph[name] = list(self.metadata(table, unit))
        return graph


class Storage:
    def __init__(self, store, scenarios):
        self.store = store
        self.store.scenarios = {name: i for i, name in enumerate(scenarios)}
    def sequence(self, *, events=False):
        return self.store.sequence(events=events)
    def snapshots(self):
        return SnapshotMap(self.store)
    def transactions(self):
        return self.store.keyed()
    def accounting(self, raw, groups):
        result = self.sequence()
        for group in groups:
            result.append(group)
        return result
    @staticmethod
    def coverage(rows, count, message):
        seen = 0
        for row in rows:
            if seen >= count:
                raise ArithmeticError(message)
            seen += 1
            yield row
        if seen != count:
            raise ArithmeticError(message)
    @staticmethod
    def compare_rows(expected, received, check):
        missing = object()
        for wanted, actual in itertools.zip_longest(expected, received, fillvalue=missing):
            check()
            if wanted is missing or actual is missing:
                raise ArithmeticError('generated complete persisted row coverage differs')
            compare(wanted, actual, check=check)
    def verify_reports(self, prepared, events, writer, check):
        sinks = {}
        class Sink:
            def __init__(self, name):
                self.rows = iter(_rows(writer, name, check)); self.pending = None
            def append(self, value):
                self.finish_pending(); self.pending = value
            def extend(self, values):
                for value in values:
                    self.append(value)
            def __getitem__(self, index):
                if index != -1 or self.pending is None:
                    raise ArithmeticError('bounded report sink only retains current pending row')
                return self.pending
            def finish_pending(self):
                if self.pending is not None:
                    try: actual = next(self.rows)
                    except StopIteration as exc: raise ArithmeticError('missing generated persisted report row') from exc
                    compare(self.pending, actual, check=check); self.pending = None
            def finish(self):
                self.finish_pending()
                if next(self.rows, None) is not None:
                    raise ArithmeticError('unowned generated persisted report row')
        def sink(name):
            result = Sink(name); sinks[name] = result; return result
        class Days:
            def __init__(self, scenario): self.scenario = scenario
            def __getitem__(self, day): return events.for_day(self.scenario, day)
        reports(prepared, (), check, row_sink=sink, events_by_day=Days)
        for sink_ in sinks.values():
            sink_.finish()


def verify_stream(raw, prepared, frames, check=lambda: False):
    if not exact(raw, prepared['raw']):
        raise ArithmeticError('generated verification original source differs')
    if set(frames.schemas) != set(SCHEMAS):
        raise ArithmeticError('generated exact persisted table coverage differs')
    for table, schema in SCHEMAS.items():
        if frames.schemas[table] != {name: str(dtype) for name, dtype in schema.items()}:
            raise ArithmeticError('generated persisted column schema differs/' + table)
    header = dict(schema=WORKFLOW, model_schema=SCHEMA, prepared_request={}, source_receipt=prepared['receipt'])
    validate_trailer(header, frames.trailer)
    if not exact(frames.completion, dict(complete=True, protocol=6)):
        raise ArithmeticError('generated actual native Sim completion differs')
    coefficients = list(_rows(frames, 'commercial_candidate_units', check))
    if len(coefficients) != 1 or coefficients[0]['unit_index'] != 0 or coefficients[0]['source_index'] != 0:
        raise ArithmeticError('generated coefficient metadata coverage differs')
    result = loads(coefficients[0]['native_json'].encode())
    graphs = Graphs(raw, frames, check)
    for table in METADATA_TABLES - {'commercial_candidate_units'}:
        last = None
        for row in _rows(frames, table, check):
            key = row['unit_index'], row['source_index']
            if not 0 <= key[0] < len(graphs) or (last is not None and key <= last):
                raise ArithmeticError('generated duplicate/out-of-order/foreign metadata identity')
            last = key
    result['raw_source'] = graph_typed(raw['commercial']); result['native_graphs'] = graphs
    with replay_store(check) as store:
        storage = Storage(store, ['baseline'] + [s.name for s in prepared['spec']['scenarios']])
        with accounting_spool(storage.accounting):
            verify_units(raw['commercial'], result, check)
            wanted = allocation(result, raw['allocation'], check)
            actual = deepcopy(prepared['receipt']['allocation_replay'])
            if not actual.pop('scope', None):
                raise ArithmeticError('generated allocation scope absent')
            compare(wanted, actual, check=check)
            controls, allocations, participations = [], [], []
            mapping = {m['source_scenario_id']: m for m in raw['run_mapping']}
            for ui, (unit, owner) in enumerate(zip(raw['commercial']['units'], raw['owners'], strict=True)):
                graph = verify_graph(unit['graph'], graphs[ui], check)
                allocations.append(dict(position_id=owner['position_id'], allocated_face=raw['allocation'][ui]['face']))
                for source in graph['sources']:
                    if source['role'] != 'physical': continue
                    request = deepcopy(source['source_request'])
                    scene = mapping[source['scenario_id']]['ledger_scenario_id']
                    request['source']['current']['scenario_id'] = scene
                    request['source']['current']['position_id'] = owner['position_id']
                    controls.append(dict(raw=request, groups=source['groups'],
                        origin_day=unit['graph']['as_of_ordinal'] - raw['as_of_ordinal'],
                        source_class='issuer_ac_security-1' if unit['graph']['positions'][0]['credit_identity'] == 'issuer_credit_debt' else 'commercial_loan-1'))
                    participations.append(dict(scenario=scene, position_id=owner['position_id'],
                        path_key=source['path_key'], source_weight=source['weight']))
            if len(participations) != prepared['receipt']['physical_paths']:
                raise ArithmeticError('generated complete native Physical paths differ')
            control = dict(spec=prepared['spec'], horizon=prepared['horizon'], sources=controls)
            validation = verify_journal(dict(allocations=allocations, participations=participations), control, frames, check, storage=storage)
            validation.update(raw_graph_units_recomputed=True, allocation_replayed=True,
                purchased_journal_replayed=True, complete_tail_replayed=True,
                maximum_live_full_physical_owners=1, compact_absolute_endpoints_replayed=True,
                opening_identity_verified=False, mixed_ledger_adoption=False)
            return validation
