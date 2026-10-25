//! Content-addressed local objects: an intact object is reused, and a damaged one is caught on
//! read and replaced with its true bytes on the next write of that content.
use workbench_native::objects::Objects;

#[test]
fn a_damaged_object_refuses_to_read_and_is_repaired_by_the_next_write() {
    let dir = std::env::temp_dir().join(format!("wbn-objects-{}", std::process::id()));
    let objects = Objects::open(dir.to_str().unwrap(), "tenant", "workspace").unwrap();
    let data = b"verbatim stream chunk\n".repeat(1000);
    let d = objects.put(&data, "bin").unwrap();
    let path = dir.join(&d.key);
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[10] ^= 1;
    std::fs::write(&path, &bytes).unwrap();
    assert!(objects.get_bounded(&d, 1 << 20).is_err());
    // The same content written again restores the object rather than trusting the damaged file.
    let again = objects.put(&data, "bin").unwrap();
    assert_eq!(again, d);
    assert_eq!(objects.get_bounded(&d, 1 << 20).unwrap(), data);
    let _ = std::fs::remove_dir_all(&dir);
}
