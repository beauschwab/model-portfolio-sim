//! Genuine complete native preparation; no cut Sim or publication authority.
use super::{candidates, cut_mixed};
use portfolio_compute_control::{scope, Control};
use serde_json::{json, Value};

fn inspect(request: &cut_mixed::Request) -> Result<Value, String> {
    scope(Some(Control::new(180_000)?), || {
        let a = candidates::admit_finite_preparation(&request.completed_candidate)?;
        let b = candidates::admit_finite_preparation(&request.revised_candidate)?;
        let mut result = None;
        candidates::with_finite_compiled(
            &request.completed_candidate,
            (b.work_upper, b.retained_bytes_upper),
            &mut |_, _, old| {
                candidates::with_finite_compiled(
                    &request.revised_candidate,
                    (a.work_upper, a.retained_bytes_upper),
                    &mut |_, _, new| {
                        result = Some(
                            json!({"completed":old.event_index_inventory()?,"revised":new.event_index_inventory()?,
                            "source_preparation_work_upper":a.work_upper+b.work_upper,
                            "source_retained_bytes_upper":a.retained_bytes_upper+b.retained_bytes_upper,
                            "cut_sim_executed":false,"cut_admission_granted":false,"publication_eligible":false}),
                        );
                        Ok(())
                    },
                )?;
                Ok(())
            },
        )?;
        result.ok_or("native index inventory absent".into())
    })
}

#[test]
fn full_public_and_short_complete_sources_keep_all_events_without_daily_schedule_allocations() {
    for (name,source) in [("full",include_str!("../../../../../examples/models/commercial-cut-mixed-journal-1.json")),
        ("short",include_str!("../../../../portfolio-risk/tests/fixtures/commercial-short-secondary-cut-refusal.json"))] {
        let mut request:cut_mixed::Request=serde_json::from_str(source).unwrap();
        let mut inventory=inspect(&request).unwrap();
        for side in ["completed","revised"] {
            let candidate=if side=="completed" {&request.completed_candidate} else {&request.revised_candidate};
            let has_fees=candidate.commercial.units.iter().any(|u|u.graph.positions.iter().any(|p|p.fees.is_some()));
            for (index,part) in inventory[side].as_array().unwrap().iter().enumerate() {
                if index==0 || has_fees { assert!(part["entries"].as_u64().unwrap()>0); }
                else { assert_eq!(part["entries"],0); }
                assert_eq!(part["owned_scenario_string_bytes"],0);
                assert_eq!(part["daily_schedule_vector_allocations"],0);
                assert_eq!(part["complete_source_sim_codec_bounds"],false);
                assert!(part["sort_comparisons"].as_u64().unwrap()<=part["sort_comparison_upper"].as_u64().unwrap());
            }
        }
        for candidate in [&mut request.completed_candidate,&mut request.revised_candidate] {
            for allocation in &mut candidate.allocation { allocation.face=0.; }
        }
        let zero=inspect(&request).unwrap();
        // The allocation's shorter numeric text changes raw-byte work/capacity
        // charges. Every source event, order, index capacity and sort counter
        // must nevertheless remain exactly identical at zero participation.
        for side in ["completed","revised"] {
            assert_eq!(zero[side],inventory[side],"zero participation must retain every source/index");
        }
        inventory["zero_allocation_index_values_exact"]=json!(true);
        println!("COMMERCIAL_EVENT_INDEX_{name}={inventory}");
        if let Ok(folder)=std::env::var("COMMERCIAL_EVENT_INDEX_ARTIFACT") {
            use std::io::Write;
            let mut out=std::fs::OpenOptions::new().write(true).create_new(true)
                .open(std::path::Path::new(&folder).join(format!("{name}-inventory.json"))).unwrap();
            out.write_all(serde_json::to_string_pretty(&inventory).unwrap().as_bytes()).unwrap();
        }
    }
}
