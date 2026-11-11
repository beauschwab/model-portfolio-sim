# Gaussian coupon, credit and exercise numerical family

This separate test family combines a genuinely varying observed reset with
independent supplied default intensities and one actual call or put date. It
changes no production code, existing benchmark, financial tolerance or capacity
limit. It is contained numerical evidence, not empirical credit calibration,
general exercise convergence, legal approval or completion of issues 15/25.

## Predeclared family and policy amendment

The complete 48 cells cross Brownian motion and a one-factor OU process with
mean reversion 0.7, credit off/enabled, option-free/call/put, 256/512 training and
fresh evaluation paths, and seed pairs (7,11)/(19,23). Annual variance is 0.04;
initial and long-run means are zero. Loading 20 deliberately creates a synthetic
coupon stress, not a calibrated or representative rate distribution. Every cell,
refusal and numerical miss remains in the receipt.

Before family execution, 18 signed source/book hand controls checked positive,
zero and negative coupons, interest recovery fractions zero/one, and defaults
on days 5, 20 and 32. An initial malformed fixture changed an indexed coupon to
fixed and correctly refused contractual modification; that setup failure is
retained. The corrected fixture changes only the allowed projected reset rate.

Zero-fraction controls retain signed gross legal **and recognized** claims:
annual rate -0.1 and default day 20 leave approximately -0.13698630136986314
in each balance, with zero coupon recovery cash. Day 32 leaves
-0.410958904109589. Zero modeled recovery is not waiver, extinguishment or a
valuation of the surviving obligation. Its eventual settlement and adoption
remain a distinct open model/policy gate. The owner reports
`claims_fully_closed=false`; complete modeled recovery cash is not all-rights
settlement. No inconsistent all-rights-settled claim was found.

The family was therefore amended **before execution** to select full signed
accrued-interest recovery on its explicit zero-lag tail. Negative amounts are
actually paid once as negative cash, and positive amounts are collected once.
Principal recovery remains 60 against face 100. Cell inventory, laws, strikes,
seeds and counts were unchanged. This policy does not close the zero-recovery
signed-obligation gate. Residual default principal 40 also remains identified;
legal write-off and accounting approval are not inferred.

## Contract, clocks and independent continuation

Let F=100, R=60, exercise/reset time e=15/365, accrual length tau=15/365 and actual
coupon/principal settlement T=35/365. The first coupon is zero. The second rate
is L*X(e), L=20; its term-simple coupon accrues on days 15–30 and pays on day 35.
Fixing, selection, projection information and reset cutoff are day-15 midnight.
Exercise occurs after that day's scheduled cash and reset availability, with
zero accrued balance at the new period's start. Principal and strike are 100;
original fixed OAS, parallel bump and discounting are zero. Original quote is
100 at unchanged original EIR zero.

The Pricing law is dX=-a*X*dt+0.2*dW. Thus X(e)=sigma*Z for standard normal Z;
sigma squared is 0.04*e when a=0, otherwise
0.04*(1-exp(-2*a*e))/(2*a). No Physical history fits this Pricing law. Separate
Physical hand owners audit shared signed-default semantics without an inferred
measure conversion.

Only performing-to-default is enabled. Default is absorbing and independent of
X. Enabled pre-exercise default probability is p0=0.05, with conditional post-
exercise probability p1=0.01; both are zero in credit-off cells. Supplied hazards
are -log(1-p0)/e and lambda=-log(1-p1)/(T-e). Default before exercise produces R
and has no live exercisable coupon right.

For an eligible survivor with observed x, default D years after exercise before
T produces R+F*L*x*min(D,tau). Surviving to T produces F+F*L*x*tau. Full signed
interest recovery includes default after accrual end but before settlement. Hence

    q = exp(-lambda*(T-e))
    A = integral_0^tau exp(-lambda*t) dt
      = (1-exp(-lambda*tau))/lambda, or tau at lambda=0
    C(x) = m+B*x
    m = R*(1-q)+F*q
    B = F*L*A

A follows from E[min(D,tau)], including actual survivor cash q*tau; it is not
q*tau alone. Signed hand controls and a separate default-time integral verify
the coefficient, day-30 accrual cap, once-only cash and retained obligations.
Future realized defaults never enter decision features.

## Optimal versus learned policy and sampling

At K=100, optimal issuer calls stop when C(x)>K and holder puts when C(x)<K;
ties continue. The boundary is x*=(K-m)/B. Root value is the separate pre-default
term R*p0 plus (1-p0) times E[min(K,C)] or E[max(K,C)]. Option-free value is
R*p0+(1-p0)*m.

For any normal stop interval (l,u), substituting K for C(sigma*Z)=m+s*Z changes
the live expectation by

    (K-m)*(Phi(u)-Phi(l)) - s*(phi(l)-phi(u)).

Exact Gaussian moments value both the optimal and actual learned rule. Only the
latter uses fitted coefficients; analytic C never consumes native cash targets.
Every varying feature maps independently onto x: market state X and the available
reset rate L*X. Principal, legal balance, credit indicators and period entitlements
are known constants at this clock. Linear SVD has two collinear varying columns
and rank two including its intercept. Controls also integrate reversed or
constant learned stopping rules without assuming the fitted rule is optimal.

Regret is learned minus optimal for the issuer's minimum, and optimal minus
learned for the holder's maximum. The existing 0.02 loss gate is unchanged.
Sampling error is native evaluation mean minus the independently integrated
learned-policy value, separate from regret and total error. No monotonic realized
convergence is required.

Every result must pass independent raw stochastic/credit/source and stopped-cash
replay. A second hand-derived oracle integrates supplied cumulative-hazard draws
and applies signed default-accrual cash to every path and eligible decision.
Pairs i and i+N/2 reconstruct the native cluster mean and sample standard error.
The unchanged five-SE diagnostic band retains every miss. Normal 95% coverage is
descriptive, without rare-event coverage or PRNG independence guarantees.
Financial parity remains 1e-9 absolute/1e-12 relative.

Credit-off/on comparisons retain identical market draws and compare actual
learned native decisions among common eligible paths. At least one change is
required; a constant-state substitute cannot satisfy this family. Both signed
default recovery directions must occur. Training and evaluation credit seeds
and domains remain separate.

## Domain, provenance and remaining gates

Gaussian laws are unbounded; native coupon domain guards remain. For |Z|<=12,
the worst selected coupon gross factor is positive. Receipts report an outside-
range probability bound and affine absolute-cash tail bound from Gaussian first
moments. Every realized sample must still pass native admission. This is not
global Gaussian native-domain admission or a value assigned to an invalid tail
contract. Extreme theoretical tails and finite PRNG samples remain distinct.

The harness pins executable, native sources, engine references and new benchmark/
test/doc bytes before and after execution. Existing receipts cannot be overwritten.
Prior 60/24/107 evidence and the original executable remain sealed. The initial
family reused that matching artifact; a subsequent numerical correction requires
a separately pinned matching artifact and new immutable receipt. State-dependent
hazards, market/credit dependence, multiple
exercise dates, fees, unresolved signed rights, empirical holdouts and mixed/scale
acceptance remain open. This family makes no DV01 or policy-refitting claim.

## Retained numerical blocker and correction gate

The initial complete execution retained 36 passing and 12 failing cells. All
12 fail the unchanged independent coefficient comparison; some also change
native learned decisions and monetary outputs. This invalidates release
acceptance for the supported family. No failed cell, seed, coefficient check,
financial tolerance or sampling criterion was removed.

Exact native DMatrix captures match independently constructed feature rows,
targets, normalization and column energies. The pinned decomposition itself
fails reconstruction and solve checks. A deterministic16-by-3 example reproduces
the problem; changing to explicit bounded EPS/100000 settings does not fix it.
Three cases also reproduce on the optimized combined release. Initial binaries,
raw/native outputs, spectra, coefficients, functional predictions and failures
remain immutable evidence.

The separate [checked native least-squares stage](commercial-checked-least-squares.md)
adds a bounded native correction, actual-matrix and solve checks, explicit work
and workspace admission, and cutoff/scale/cancellation controls. The complete
unchanged 48-cell family and prior families must pass against a new matching
artifact before that blocker is cleared. This document records the initial
failure independently of any later corrected receipt. The original fraction-zero
signed gross-obligation valuation/adoption boundary remains open even when the
explicit full signed-recovery family passes.

The continuation-regression question follows
[Longstaff and Schwartz (2001)](https://people.math.ethz.ch/~hjfurrer/teaching/LongstaffSchwartzAmericanOptionsLeastSquareMonteCarlo.pdf);
that context is not proof of this finite family's convergence. The survival
calculation specializes [Duffie and Garleanu (2001), section 3.1](https://www.darrellduffie.com/uploads/1/4/8/0/148007615/duffiegarleanu2000.pdf).
All coupon/recovery/exercise choices are explicit synthetic policies. Gaussian
moments and the signed default-accrual coefficient are derived for this contract.
