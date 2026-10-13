"""Independent mathematical helpers for the prospective public pending-source-exit port.

These are internal numeric objects, not an assumed native wire schema. SciPy
augmented matrix exponentials independently propagate active AND pending slots.
Actual source claims and cash are owned only by a separate boundary action.
"""
import math
import numpy as np
from scipy.linalg import expm

S=7
MAX_AMOUNT=1e15


def ordered_sum(values):
    result=0.
    for value in values:result+=float(value)
    return result


def checked_vector(value, *, signed=False):
    array=np.asarray(value,dtype=float)
    if array.shape!=(S,) or not np.isfinite(array).all() or np.any(np.abs(array)>MAX_AMOUNT) or (not signed and np.any(array<0.)):
        raise ArithmeticError('bounded seven-state amount vector')
    return array


def checked_generator(value):
    q=np.asarray(value,dtype=float)
    if q.shape!=(S,S) or not np.isfinite(q).all():raise ArithmeticError('finite seven-state generator')
    for i,row in enumerate(q):
        if i>=3:
            if np.any(row!=0.):raise ArithmeticError('pending exits are absorbing until boundary')
        else:
            rates=[row[j] for j in range(S) if j!=i]
            if row[i]>0. or any(rate<0. for rate in rates) or abs(sum(rates)+row[i])>1e-12 or sum(rates)>100.:
                raise ArithmeticError('bounded competing risks and exact generator mass')
            if any(row[3+j]!=0. for j in range(3) if j!=i):raise ArithmeticError('default-by-exit-origin layout')
    return q


def propagate(book, generator, investor_rates, loan_rates, original_yield, years):
    """Raw independent row-vector block exponentials, without cash or claims."""
    q=checked_generator(generator)
    if not math.isfinite(years) or not 0.<=years<=64./365. or not math.isfinite(original_yield) or not -.99<=original_yield<=10.:
        raise ArithmeticError('bounded duration/original yield')
    inv=np.asarray(investor_rates,dtype=float);loan=np.asarray(loan_rates,dtype=float)
    if inv.shape!=(3,) or loan.shape!=(3,) or not np.isfinite(inv).all() or not np.isfinite(loan).all() or np.any(inv<0.) or np.any(inv>loan) or np.any(loan>100.):
        raise ArithmeticError('captured contractual and recognized live rates')
    p=checked_vector(book['principal']);a=checked_vector(book['accrued']);g=checked_vector(book['gross']);e=checked_vector(book['earned'],signed=True)
    lp=checked_vector(book['loan_principal']);la=checked_vector(book['loan_interest']);probability=checked_vector(book['probability'])
    if ordered_sum(probability)>1.+1e-12 or np.any(a>la):raise ArithmeticError('probability distinct from owned/gross rights')
    transition=expm(q*years)
    def accrued(principal,opening,rates):
        block=np.zeros((2*S,2*S));block[:S,:S]=q;block[S:,S:]=q;block[:S,S:]=np.diag(list(rates)+[0.]*4)
        answer=np.r_[principal,opening]@expm(block*years)
        return answer[:S],answer[S:]
    np_,na=accrued(p,a,inv);nlp,nla=accrued(lp,la,loan)
    k=np.diag([math.log1p(original_yield)]*3+[0.]*4)
    growth=np.zeros((2*S,2*S));growth[:S,:S]=q+k;growth[:S,S:]=k;growth[S:,S:]=q
    carried=np.r_[g,e]@expm(growth*years);ng=carried[:S];ne=carried[S:]
    reward=g@expm(growth*years)[:S,S:]
    return dict(probability=(probability@transition).tolist(),principal=np_.tolist(),accrued=na.tolist(),gross=ng.tolist(),earned=ne.tolist(),loan_principal=nlp.tolist(),loan_interest=nla.tolist()),reward.tolist()


def book_lines(before, after, new_reward):
    """Independent recognition algebra; tuples are NOT native wire postings."""
    lines=[]
    for i in range(S):
        old_basis=before['gross'][i]-before['principal'][i]-before['accrued'][i]
        new_basis=after['gross'][i]-after['principal'][i]-after['accrued'][i]
        lines.extend([('principal',i,after['principal'][i]-before['principal'][i]),('accrued',i,after['accrued'][i]-before['accrued'][i]),('basis',i,new_basis-old_basis)])
    lines.append(('original_eir',None,-ordered_sum(new_reward)))
    return lines
