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


def numerical_reserve(raw, *, columns=None, fits_continuation=True):
    dates = len(raw["policy"]["exercise_dates"])
    if not dates:
        return 0, 0
    if columns is None:
        degree = {"native_observable_linear_svd_train_evaluate-1":1,
                  "native_observable_quadratic_svd_train_evaluate-1":2,
                  "native_observable_cubic_svd_train_evaluate-1":3}[raw["regression_policy"]]
        columns = min(comb(active_feature_upper(raw)+degree, degree), raw["training"]["paths"]//2)
    work, workspace = shape_bound(raw["training"]["paths"], columns)
    # Physical generated application runs the complete raw metadata census but
    # applies retained Q coefficients. It does not execute this checked fit.
    return (dates*work if fits_continuation else 0)+8*scan_work_upper(raw), workspace+128


def scan_work_upper(raw):
    source=raw['source']
    draws=source.get('performing_draw_tranches',[])
    periods=len(source['source']['current']['periods'])+sum(len(d['source']['current']['periods'])for d in draws)
    return (32+15*len(source['credit']['intervals'])+len(raw['annual_covariance'])
            +2*periods+8*len(draws)+len(raw['coupon_loadings']))


def active_feature_upper(raw):
    """Structural constants only; no sample realizations or economic netting."""
    source=raw['source']
    full=full_feature_upper(raw)
    if source.get('performing_draw_tranches'):return full
    credit=any(h!=0 for interval in source['credit']['intervals']
               for row in interval['hazards_per_year']for h in row)
    if raw.get('nonintegral_fees') is not None and not constant_fee_observations(raw):
        return full
    if not credit and all(c==0 for c in raw['annual_covariance']):return 0
    contract=source['source']['current']
    if (not credit and source.get('reviewed_credit_actions') is None
            and source['source'].get('transfer') is None
            and contract['principal_plan']['kind']=='supplied'):
        d=len(raw['drivers'])
        if all(row['loading']==0 for row in raw['coupon_loadings']):
            return d+(d if raw['hazard_loadings'] else 0)
        return (d+(d if raw['hazard_loadings']else 0)+1+len(contract['periods'])
                +sum(len(p['coupon'].get('observations',[]))for p in contract['periods']))
    return full


def full_feature_upper(raw):
    from .commercial_generated_physical_reference import feature_bound
    law=raw.get('nonintegral_fees')
    return feature_bound(raw)+11*len(law['policy']['fees'] if law is not None else [])


def constant_fee_observations(raw):
    """Independent raw-law proof; never infer constancy from generated samples."""
    source=raw['source']
    law=raw.get('nonintegral_fees')
    return (law is not None and law['policy'].get('transfer') is None
            and source['source'].get('transfer') is None
            and not source.get('performing_draw_tranches')
            and source.get('reviewed_credit_actions') is None
            and source['source']['current']['principal_plan']['kind']=='supplied'
            and all(h==0 for p in source['credit']['intervals']
                    for row in p['hazards_per_year']for h in row))


def verify_numeric_reserve(raw, actual):
    work, workspace = numerical_reserve(raw)
    if actual["admitted_work_upper"] < work or actual["retained_bytes_upper"] < workspace:
        raise ArithmeticError("checked regression work/workspace missing before generation")
    return work, workspace
