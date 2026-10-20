//! Saved-book mapping (stage F, #48): composition reproduces the worked-unwind request exactly
//! from a saved book and a plan, binds the saved revision, and lists every missing mapping with
//! its fix instead of running an incomplete plan.
mod common;
use common::*;
use serde_json::{json, Value};
use workbench_native::compose::{self, Binding};

const BOOK: &str = include_str!("fixtures/lifecycle-book.json");
const PLAN: &str = include_str!("fixtures/lifecycle-plan.json");

fn book() -> Value {
    compose::parse_book(BOOK.as_bytes()).unwrap()
}
fn plan() -> Value {
    compose::parse_plan(PLAN.as_bytes()).unwrap()
}
fn binding() -> Binding<'static> {
    Binding {
        workspace: "w",
        revision: 2,
        book_name: "desk-book",
        book_sha256: "ab",
    }
}
fn needs(plan: &Value) -> Vec<String> {
    compose::requirements(&book(), plan)
        .into_iter()
        .map(|r| format!("{} | {} | {}", r.record, r.detail, r.fix))
        .collect()
}

#[test]
fn a_saved_book_and_plan_compose_exactly_the_worked_unwind_request() {
    let composed = compose::compose(&book(), &plan(), &binding()).unwrap();
    let mut expected: Value = serde_json::from_str(REQUEST).unwrap();
    // Only the revision binding differs: composition names the saved book and its revision.
    expected["plan"]["workspace_revision"] = json!({"id": "w", "revision": "2"});
    expected["plan"]["saved_book_revision"] = json!({"id": "desk-book", "revision": "2:ab"});
    assert_eq!(composed.request, expected);
    assert_eq!(
        serde_json::from_slice::<Value>(&composed.bytes).unwrap(),
        expected
    );
}

#[test]
fn every_missing_mapping_is_listed_with_its_fix() {
    let mut p = plan();
    p.as_object_mut().unwrap().remove("hedge_treatments");
    p.as_object_mut().unwrap().remove("funding_instructions");
    let n = needs(&p);
    assert!(
        n.iter()
            .any(|x| x.starts_with("hedge allocation bond-hedge")
                && x.contains("hedge_treatments.bond-hedge")),
        "{n:?}"
    );
    assert!(
        n.iter()
            .any(|x| x.starts_with("funding allocation repo-funds-bond")
                && x.contains("funding_instructions.repo-funds-bond")),
        "{n:?}"
    );

    // Selling a pledged bond without releasing it names the pledge and the repayment to add.
    let mut p = plan();
    p["instructions"]["repayments"][0]["pledge_releases"] = json!([]);
    let n = needs(&p);
    assert!(
        n.iter()
            .any(|x| x.contains("pledged under `bond-to-repo`")
                && x.contains("releases `bond-to-repo`")),
        "{n:?}"
    );

    // Terminating a hedge needs its termination; repaying funding needs its repayment.
    let mut p = plan();
    p["instructions"]["terminations"] = json!([]);
    p["instructions"]["repayments"] = json!([]);
    let n = needs(&p);
    assert!(
        n.iter()
            .any(|x| x.contains("no termination instruction for `swap-lot`")),
        "{n:?}"
    );
    assert!(
        n.iter()
            .any(|x| x.contains("no repayment instruction for `repo-lot`")),
        "{n:?}"
    );

    // Unknown records and partial quantities beyond the lot.
    let mut p = plan();
    p["actions"][0]["lots"][0]["quantity"] = json!(101.0);
    p["instructions"]["facility_draws"][0]["facility_id"] = json!("missing-facility");
    let n = needs(&p);
    assert!(
        n.iter()
            .any(|x| x.contains("quantity 101 is outside (0, 100]")),
        "{n:?}"
    );
    assert!(
        n.iter()
            .any(|x| x.contains("facility `missing-facility` is not in the saved book")),
        "{n:?}"
    );

    // A partial unwind of a held lot is a valid mapping.
    let mut p = plan();
    p["actions"][0]["lots"][0]["quantity"] = json!(40.0);
    assert!(needs(&p).is_empty());
    let (refusal, list) = compose::compose(
        &book(),
        &{
            let mut q = plan();
            q.as_object_mut().unwrap().remove("hedge_treatments");
            q
        },
        &binding(),
    )
    .unwrap_err();
    assert_eq!(refusal.code, compose::MAPPING_REQUIRED);
    assert_eq!(list.len(), 1);
}

#[test]
fn formal_treatment_is_never_mapped_to_the_economic_lane() {
    let mut formal = plan();
    formal["actions"][0]["accounting_treatment"] = json!("formal");
    let economic = compose::compose(&book(), &plan(), &binding())
        .unwrap()
        .request;
    let composed = compose::compose(&book(), &formal, &binding())
        .unwrap()
        .request;
    assert_eq!(
        composed["plan"]["actions"][0]["accounting_treatment"],
        "formal"
    );
    // Equal economic inputs: the requests differ only in the accounting treatment.
    let mut same = composed.clone();
    same["plan"]["actions"][0]["accounting_treatment"] = json!("economic");
    assert_eq!(same, economic);
}

#[test]
fn saved_documents_compose_over_http_and_unknown_fields_are_refused() {
    let env = Env::new(Backend::Sqlite, "book");
    env.ready();
    let api = env.api();
    assert_eq!(
        api.call(
            "PUT",
            "/native/documents/desk-book",
            &[("If-Match", "0")],
            BOOK.as_bytes()
        )
        .status,
        201
    );
    assert_eq!(
        api.call(
            "PUT",
            "/native/documents/unwind-plan",
            &[("If-Match", "1")],
            PLAN.as_bytes()
        )
        .status,
        201
    );
    let docs = api.get("/native/documents").body;
    assert_eq!(docs["revision"], 2);
    let schemas: Vec<&str> = docs["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["schema"].as_str().unwrap())
        .collect();
    assert_eq!(schemas, ["liquidity-book-1", "liquidity-plan-1"]);
    assert_eq!(
        api.get("/native/documents/desk-book").body["schema"],
        "liquidity-book-1"
    );

    let c = api.call(
        "POST",
        "/native/lifecycle/compose?book=desk-book&plan=unwind-plan",
        &[("X-Expected-Revision", "2")],
        b"",
    );
    assert_eq!(c.status, 200, "{:?}", c.body);
    let sha = docs["documents"][0]["sha256"].as_str().unwrap();
    assert_eq!(
        c.body["request"]["plan"]["saved_book_revision"]["revision"],
        format!("2:{sha}")
    );
    assert_eq!(
        c.body["request"]["plan"]["workspace_revision"]["id"],
        env.workspace.as_str()
    );

    let mut incomplete: Value = serde_json::from_str(PLAN).unwrap();
    incomplete
        .as_object_mut()
        .unwrap()
        .remove("funding_instructions");
    let r = api.call(
        "POST",
        "/native/lifecycle/compose?book=desk-book",
        &[("X-Expected-Revision", "2")],
        &serde_json::to_vec(&incomplete).unwrap(),
    );
    assert_eq!(
        (r.status, r.body["code"].as_str()),
        (422, Some("LIFECYCLE_MAPPING_REQUIRED"))
    );
    assert_eq!(
        r.body["requirements"][0]["record"],
        "funding allocation repo-funds-bond"
    );
    // An incomplete plan is never admitted as a run.
    let run = api.call(
        "POST",
        "/native/lifecycle/runs?book=desk-book",
        &[("X-Expected-Revision", "2")],
        &serde_json::to_vec(&incomplete).unwrap(),
    );
    assert_eq!(run.status, 422);
    // A stale revision and an unknown plan field are refused before composition.
    let stale = api.call(
        "POST",
        "/native/lifecycle/compose?book=desk-book&plan=unwind-plan",
        &[("X-Expected-Revision", "0")],
        b"",
    );
    assert_eq!(stale.status, 404, "{:?}", stale.body);
    let mut extra: Value = serde_json::from_str(PLAN).unwrap();
    extra["surprise"] = json!(1);
    let bad = api.call(
        "PUT",
        "/native/documents/bad",
        &[("If-Match", "2")],
        &serde_json::to_vec(&extra).unwrap(),
    );
    assert_eq!(
        (bad.status, bad.body["code"].as_str()),
        (400, Some("MALFORMED_INPUT"))
    );
}
