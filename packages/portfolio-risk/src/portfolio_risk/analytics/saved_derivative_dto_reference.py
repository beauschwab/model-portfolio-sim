"""Independent declared saved/Accounting/CSA DTO normalization.

Only input f64 field declarations are normalized. Original JSON Value identity is
checked separately; no producer output, digest or reported shape defines types.
Identity payloads contain only declared strings and are never converted.
"""
from .saved_derivative_mapping_reference import typed as trade_typed,bits

# Input declarations: Accounting OpeningState/TradeAnchor, CSA Node/Path/
# DefaultPolicy/Policy. Trade/rate/index f64 declarations remain trade_typed.
ACCOUNTING_CSA_F64=frozenset((
 'clean_value','cash','posted_vm_asset','received_vm_liability','posted_im_asset',
 'received_im_custody','equity_credit_balance','discount_from_asof',
 'collateral_rate','funding_spread','counterparty_recovery',
 'posted_vm_return_recovery','posted_im_return_recovery','receive_threshold',
 'post_threshold','minimum_transfer','opening_vm','opening_received_im',
 'opening_posted_im'))

def typed(value,field=None):
    if field in ACCOUNTING_CSA_F64:
        bits(value);return float(value)
    if type(value)is list:return [typed(x)for x in value]
    if type(value)is dict:
        return trade_typed({k:typed(v,k)for k,v in value.items()},field)
    return trade_typed(value,field)

def saved_dto(value):
    result=typed(value)
    result.setdefault('contractual_upgrade_review',None)
    for legacy in result['book']['legacy_hedges']:legacy.setdefault('float_spread',0.)
    physical=result['physical'];physical.setdefault('pricing_xva_reference',None)
    for review in physical['policy']['trades']:review.setdefault('delivery_review',None)
    for derivative in [physical['derivative']]+([]if physical['pricing_xva_reference']is None else[physical['pricing_xva_reference']]):
        for path in derivative['operation']['request']['paths']:
            path.setdefault('default',None)
            if path['default']is not None:path['default'].setdefault('mpor',None)
    return result
