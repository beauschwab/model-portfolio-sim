use super::*;
pub(super) fn matched<'a>(
    p: &'a source::PreparedPath,
    b: &'a source::PreparedPath,
) -> Result<(), String> {
    let a = &p.preparation.raw_request;
    let c = &b.preparation.raw_request;
    if p.source_id != b.source_id
        || p.owner_index != b.owner_index
        || serde_json::to_vec(&a.policy).unwrap() != serde_json::to_vec(&c.policy).unwrap()
        || serde_json::to_vec(&a.resets).unwrap() != serde_json::to_vec(&c.resets).unwrap()
        || a.entity != c.entity
        || a.currency_unit != c.currency_unit
        || a.side != c.side
        || a.start_ordinal != c.start_ordinal
        || a.full_end_ordinal != c.full_end_ordinal
        || a.source_weight.to_bits() != c.source_weight.to_bits()
        || a.unit_face.to_bits() != c.unit_face.to_bits()
        || a.allocated_face.to_bits() != c.allocated_face.to_bits()
        || serde_json::to_vec(&a.original_yield_anchor).unwrap()
            != serde_json::to_vec(&c.original_yield_anchor).unwrap()
    {
        return Err("benchmark exact complete clock/anchor/participation scope mismatch".into());
    }
    Ok(())
}
pub(super) fn commitment(
    p: &source::PreparedPath,
    native: &source::ResultData,
) -> Result<Vec<Value>, String> {
    let receipt = &native
        .sources
        .iter()
        .find(|x| x.source_id == p.source_id)
        .ok_or("native source")?
        .raw_native_result;
    let card = receipt["product_sources"].as_array().and_then(|rows| {
        rows.iter().find(|x| {
            x["owner_index"] == p.owner_index
                && x["run_index"] == p.run_index
                && x["path_index"] == p.path_index
                && x["native"]["kind"] == "card_v2"
        })
    });
    let mut out = vec![];
    for (i, state) in p.preparation.raw_request.states.iter().enumerate() {
        portfolio_compute_control::checkpoint()?;
        if let Some(card) = card {
            let points = card["native"]["result"]["economic"]["points"]
                .as_array()
                .ok_or("native Card2 commitment source")?;
            let (index, point) = points
                .iter()
                .enumerate()
                .find(|(_, x)| x["ordinal"] == state.ordinal)
                .ok_or("complete native commitment clock")?;
            out.push(json!({"ordinal":state.ordinal,"native_economic_point_index":index,"limit":point["limit"],"undrawn":point["undrawn"],"source":"native_card_contract_commitment-1"}));
        } else {
            let amount = state
                .channels
                .iter()
                .find(|x| x.channel == "undrawn_memo")
                .ok_or("source undrawn memo")?
                .amount;
            out.push(json!({"ordinal":state.ordinal,"native_economic_point_index":i,"limit":null,"undrawn":amount,"source":"native_owned_accounting_undrawn-1"}));
        }
    }
    Ok(out)
}
pub(super) fn undrawn(p: &source::PreparedPath, points: &[Value], i: usize) -> Result<f64, String> {
    let open = points[i]["undrawn"].as_f64().ok_or("native undrawn")?;
    let avg = match p.preparation.raw_request.policy.averaging.as_str() {
        "opening_daily" => open,
        "trapezoid_daily" => {
            (open + points[i + 1]["undrawn"].as_f64().ok_or("native undrawn")?) / 2.0
        }
        _ => return Err("averaging".into()),
    };
    Ok(avg * p.preparation.scale)
}
