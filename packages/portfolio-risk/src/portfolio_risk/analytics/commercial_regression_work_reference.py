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
        from .commercial_generated_physical_reference import feature_bound
        from .commercial_stochastic_reference import BASIS_DEGREES
        degree = BASIS_DEGREES[raw["regression_policy"]]
        columns = min(comb(feature_bound(raw)+degree, degree), raw["training"]["paths"]//2)
    work, workspace = shape_bound(raw["training"]["paths"], columns)
    return dates*work, workspace


def verify_numeric_reserve(raw, actual):
    work, workspace = numerical_reserve(raw)
    if actual["admitted_work_upper"] < work or actual["retained_bytes_upper"] < workspace:
        raise ArithmeticError("checked regression work/workspace missing before generation")
    return work, workspace
