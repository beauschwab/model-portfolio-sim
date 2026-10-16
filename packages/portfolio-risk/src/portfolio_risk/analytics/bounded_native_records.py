"""Strict JSON records for explicitly selected partitioned native protocols."""
import json


def loads(data):
    """Retain JSON scalar categories and reject duplicate/nonfinite values."""
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError(f'duplicate native record field: {key}')
            result[key] = value
        return result

    def nonfinite(value):
        raise ValueError(f'nonfinite native record: {value}')

    value = json.loads(data, object_pairs_hook=pairs, parse_constant=nonfinite)
    if type(value) is not dict:
        raise ValueError('native record must be an object')
    return value
