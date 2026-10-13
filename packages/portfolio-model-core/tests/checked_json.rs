use portfolio_model_core::strict_json;
#[test]
fn checked_reader_preserves_original_decoder_numeric_and_error_domains() {
    for raw in [
        br#"{"x":-0.0,"y":0,"z":0.0}"#.as_slice(),
        br#"{"n":9007199254740993,"rate":0.19826610000000003}"#.as_slice(),
        br#"{"a":[true,null,"escaped\nlabel",1e-17]}"#.as_slice(),
    ] {
        let old = strict_json::parse(raw, 1024).unwrap();
        let new = strict_json::parse_checked(raw, 1024, || Ok(())).unwrap();
        assert_eq!(
            serde_json::to_vec(&old).unwrap(),
            serde_json::to_vec(&new).unwrap()
        );
    }
    for raw in [
        br#"{"x":1,"x":2}"#.as_slice(),
        br#"{"x":1e999}"#.as_slice(),
        br#"{}{}"#.as_slice(),
    ] {
        assert!(strict_json::parse(raw, 1024).is_err());
        assert!(strict_json::parse_checked(raw, 1024, || Ok(())).is_err());
    }
}
#[test]
fn cancellation_is_probed_inside_bounded_parser_reads() {
    let raw = format!("{{\"label\":\"{}\"}}", "x".repeat(70000));
    let mut calls = 0;
    let error = strict_json::parse_checked(raw.as_bytes(), 100000, || {
        calls += 1;
        if calls >= 3 {
            Err("COMPUTE_CANCELLED".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert!(error.contains("COMPUTE_CANCELLED"));
    // serde may sample a failed Read again while constructing its error.
    assert!((3..=4).contains(&calls));
}
