"""Shared independent commercial raw-reference discount and cash mathematics."""
import math


def _cash(credit):
    out=[]
    for path in credit['result']['paths']:
        for event in path['events']:
            if event['kind']=='transition' and event['cash']==0.:continue
            if event['kind']=='draw' or event['kind']=='transition' and event['cash']<0.:principal,interest,fees=event['cash'],0.,0.
            elif event['cash']!=0. or event['principal']!=0. or event['interest']!=0.:principal,interest,fees=event['principal'],event['interest'],event['cash']-event['principal']-event['interest']
            else:continue
            out.append(dict(event_id=event['event_id'],kind=event['kind'],year=event['year'],principal=principal,interest=interest,fees=fees))
        if path['default']:
            for index,event in enumerate(path['default']['recoveries']):out.append(dict(event_id=f'recovery:{index}',kind='recovery',year=event['year'],principal=event['principal'],interest=event['interest'],fees=0.))
    return sorted(out,key=lambda event:event['year'])


def _df(curve,year):
    x,y=curve['times_years'],curve['discount_factors']
    if year in x:return y[x.index(year)]
    if year>x[-1]:
        if curve['right_tail']=='reject':raise ArithmeticError('independent curve tail uncovered')
        logarithm=math.log(y[-1])*year/x[-1] if curve['right_tail']=='flat_zero' else math.log(y[-1])+(math.log(y[-1])-math.log(y[-2]))*(year-x[-1])/(x[-1]-x[-2])
    else:
        right=next(index for index,value in enumerate(x) if value>year);left=right-1
        logarithm=math.log(y[left])+(math.log(y[right])-math.log(y[left]))*(year-x[left])/(x[right]-x[left])
    return math.exp(logarithm)


def _price(cash,curve,asof,mark,oas,bump):
    year=(mark-asof)/365.;discount=_df(curve,year)
    return math.fsum((event['principal']+event['interest']+event['fees'])*_df(curve,event['year'])/discount*math.exp(-(oas+bump)*(event['year']-year)) for event in cash if event['year']>year)
