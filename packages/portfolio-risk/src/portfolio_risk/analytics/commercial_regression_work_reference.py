"""Independent structural reserve for checked native continuation fitting.

This is a verification-only operation/retained-component census, not a solver,
runtime estimator, aggregate RSS bound, or replacement financial backend.
"""
from math import comb


def shape_bound(rows: int, columns: int) -> tuple[int, int]:
    if type(rows) is not int or type(columns) is not int or not 1 <= columns <= rows // 2 or rows > 1024:
        raise ArithmeticError("checked least-squares shape outside bounded tall domain")
    m, k = rows, columns
    candidate = (32*m*k*k + 8*k*k*k + 65*(2*k*k+8*k)*(m+k)
                 + 256*64*k + 8*m*k)
    jacobi = (64*k*(k-1)//2*(2*m+k+32) + 16*m*k + 12*k*k
              + 16*m + 128*k)
    check = 3*m*k*k + 2*k*k*k + 12*m*k + 8*m + 16*k*k + 64*k
    workspace = 8*(5*m*k+4*k*k+4*m+32*k)+1024
    return candidate+jacobi+2*check, workspace


def numerical_reserve(raw, *, columns=None):
    dates = len(raw["policy"]["exercise_dates"])
    if not dates:
        return 0, 0
    if columns is None:
        degree = {"native_observable_linear_svd_train_evaluate-1":1,
                  "native_observable_quadratic_svd_train_evaluate-1":2,
                  "native_observable_cubic_svd_train_evaluate-1":3}[raw["regression_policy"]]
        columns = min(comb(active_feature_upper(raw)+degree, degree), raw["training"]["paths"]//2)
    work, workspace = shape_bound(raw["training"]["paths"], columns)
    return dates*work+8*scan_work_upper(raw), workspace+128


def scan_work_upper(raw):
    source=raw['source']
    draws=source.get('performing_draw_tranches',[])
    periods=len(source['source']['current']['periods'])+sum(len(d['source']['current']['periods'])for d in draws)
    return 32+15*len(source['credit']['intervals'])+len(raw['annual_covariance'])+2*periods+8*len(draws)


def active_feature_upper(raw):
    """Structural constants only; no sample realizations or economic netting."""
    from .commercial_generated_physical_reference import feature_bound
    source=raw['source']
    if source.get('performing_draw_tranches'):return feature_bound(raw)
    credit=any(h!=0 for interval in source['credit']['intervals']
               for row in interval['hazards_per_year']for h in row)
    if not credit and all(c==0 for c in raw['annual_covariance']):return 0
    contract=source['source']['current']
    if not credit and contract['principal_plan']['kind']=='supplied':
        d=len(raw['drivers'])
        return (d+(d if raw['hazard_loadings']else 0)+1+len(contract['periods'])
                +sum(len(p['coupon'].get('observations',[]))for p in contract['periods']))
    return feature_bound(raw)


def verify_numeric_reserve(raw, actual):
    work, workspace = numerical_reserve(raw)
    if actual["admitted_work_upper"] < work or actual["retained_bytes_upper"] < workspace:
        raise ArithmeticError("checked regression work/workspace missing before generation")
    return work, workspace
