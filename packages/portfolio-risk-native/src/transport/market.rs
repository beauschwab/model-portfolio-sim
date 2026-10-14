//! Array protocol adaptation to the native market lifecycle.
use crate::math::arrays::View;

pub(crate) fn market_paths(a: &[View<'_>], mortgage: bool) -> Vec<Vec<f64>> {
    let context = crate::market::MarketContext::new(
        a[0].v,
        a[1].v,
        a[2].v,
        a[3].v,
        a[4].v,
        a[4].shape,
        [a[5].at(0), a[5].at(1), a[5].at(2)],
    )
    .expect("invalid market context");
    if mortgage {
        let model = crate::market::MortgageInputs {
            vol_points: a[6].v,
            cc_beta: a[7].v,
            cc_lambda: a[8].at(0),
            ps: [a[9].at(0), a[9].at(1), a[9].at(2), a[9].at(3)],
            eps_ps: a[10].v,
            eps_h: a[11].v,
            hpi: [a[12].at(0), a[12].at(1), a[12].at(2)],
            incentive_lag: a[13].ix(0),
        };
        let out = context.mortgage(&model).expect("invalid mortgage paths");
        vec![
            out.rates.df,
            out.rates.swaps,
            out.rates.short,
            out.mtg,
            out.hpi,
            out.yoy,
        ]
    } else {
        let out = context.rates().expect("invalid rate paths");
        vec![out.df, out.swaps, out.short]
    }
}
