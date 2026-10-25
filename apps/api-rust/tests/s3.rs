//! Selected S3 publication (feature `s3`): set `TEST_S3_ENDPOINT` and `TEST_S3_BUCKET` (an
//! existing bucket) plus the usual AWS credential variables. CI uses moto's S3 server.
mod common;
use common::*;
use std::time::Duration;

#[test]
fn selected_s3_publication_pages_reopens_and_refuses_a_tampered_object() {
    let (Ok(endpoint), Ok(bucket)) = (
        std::env::var("TEST_S3_ENDPOINT"),
        std::env::var("TEST_S3_BUCKET"),
    ) else {
        eprintln!("TEST_S3_ENDPOINT/TEST_S3_BUCKET not set; S3 publication test skipped");
        return;
    };
    let mut env = Env::new(Backend::Sqlite, "s3");
    let root = format!("runs-{}", &env.workspace);
    env.extra
        .push(("ARTIFACT_URL".into(), format!("s3://{bucket}/{root}")));
    env.extra.push(("S3_ENDPOINT_URL".into(), endpoint.clone()));
    for k in [
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_REGION",
        "AWS_DEFAULT_REGION",
    ] {
        if let Ok(v) = std::env::var(k) {
            env.extra.push((k.into(), v));
        }
    }
    env.ready();
    let api = env.api();
    let id = api
        .submit(0, Some("s3"), REQUEST.as_bytes(), Some("600"))
        .body["job_id"]
        .as_str()
        .unwrap()
        .to_string();
    let log = finish(env.worker(&[]), Duration::from_secs(120));
    let view = api.get(&format!("/native/jobs/{id}")).body;
    assert_eq!(view["status"], "native_done", "{log}\n{view}");
    // The same workspace-scoped content-addressed layout as the Python service, under the root.
    let raw_key = view["authority"]["raw"]["key"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        raw_key.starts_with(&format!("{root}/workbench/")),
        "{raw_key}"
    );
    let manifest = api.get(&format!("/native/jobs/{id}/manifest")).body;
    assert_eq!(manifest["verification"]["receipt"]["verified"], true);
    let page = api
        .get(&format!(
            "/native/jobs/{id}/tables/journal?offset=0&limit=7"
        ))
        .body;
    assert_eq!(page["rows"].as_array().unwrap().len(), 7);

    // Flip one byte of a stored page in place: the bounded reopen refuses it.
    let page_ref = &manifest["tables"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["table"] == "journal")
        .unwrap()["segments"][0]["pages"][0]["object"];
    let key = page_ref["key"].as_str().unwrap();
    use object_store::ObjectStore;
    let mut builder = object_store::aws::AmazonS3Builder::from_env()
        .with_bucket_name(&bucket)
        .with_endpoint(&endpoint)
        .with_allow_http(endpoint.starts_with("http://"))
        .with_virtual_hosted_style_request(false);
    for (k, v) in &env.extra {
        if k == "AWS_REGION" {
            builder = builder.with_region(v);
        }
    }
    let store = builder.build().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let path = object_store::path::Path::from(key);
    let mut data = runtime
        .block_on(async { store.get(&path).await.unwrap().bytes().await.unwrap() })
        .to_vec();
    data[10] ^= 1;
    runtime
        .block_on(store.put(&path, bytes::Bytes::from(data).into()))
        .unwrap();
    let refused = api.get(&format!(
        "/native/jobs/{id}/tables/journal?offset=0&limit=7"
    ));
    assert_eq!(refused.status, 500);
    assert!(
        refused.body["detail"]
            .as_str()
            .unwrap()
            .contains("checksum"),
        "{:?}",
        refused.body
    );
    // Other pages and the manifest still reopen.
    assert_eq!(
        api.get(&format!("/native/jobs/{id}/tables/trial_balance?limit=3"))
            .status,
        200
    );
}
