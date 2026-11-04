//! Separate-process requested-allocation check; never production authority.
use portfolio_model_core::transport::value_capacity as raw_value_capacity;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

struct Measured;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
fn grow(n: usize) {
    let live = LIVE.fetch_add(n, SeqCst) + n;
    PEAK.fetch_max(live, SeqCst);
}
unsafe impl GlobalAlloc for Measured {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            grow(layout.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            grow(layout.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) };
        LIVE.fetch_sub(layout.size(), SeqCst);
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // Charge old/new overlap even if the System allocator grows in place.
        grow(new_size);
        let q = unsafe { System.realloc(p, layout, new_size) };
        LIVE.fetch_sub(if q.is_null() { new_size } else { layout.size() }, SeqCst);
        q
    }
}
#[global_allocator]
static ALLOCATOR: Measured = Measured;

fn check(raw: &[u8]) {
    let c = raw_value_capacity::estimate(raw, 2 << 20, || Ok(())).unwrap();
    let before = LIVE.load(SeqCst);
    PEAK.store(before, SeqCst);
    let value = portfolio_model_core::strict_json::parse_checked(raw, 2 << 20, || Ok(())).unwrap();
    let live = LIVE.load(SeqCst) - before;
    let peak = PEAK.load(SeqCst) - before;
    assert!(live <= c.value_tree_bytes_upper, "live {live} > {:?}", c);
    assert!(
        peak <= c.value_tree_bytes_upper + c.parser_workspace_bytes_upper,
        "peak {peak} > {:?}",
        c
    );
    drop(value);
    assert_eq!(
        LIVE.load(SeqCst),
        before,
        "parser retained an allocation after drop"
    );
}

fn main() {
    check(include_bytes!(
        "../../../examples/models/commercial-cut-mixed-journal-1.json"
    ));
    for n in [0, 1, 4, 5, 6, 11, 12, 16, 17, 63, 64, 65, 1000, 10001] {
        let raw = format!("[{}]", (0..n).map(|_| "0").collect::<Vec<_>>().join(","));
        check(raw.as_bytes());
        let raw = format!(
            "{{{}}}",
            (0..n)
                .map(|i| format!("\"key{i}\":[0,1,2,3,4,5,6]"))
                .collect::<Vec<_>>()
                .join(",")
        );
        check(raw.as_bytes());
    }
    for n in [1, 10, 32767, 32768, 100000] {
        let raw = format!("[\"{}\"]", "\\u00e9".repeat(n));
        check(raw.as_bytes());
    }
    println!("34 raw parser allocation cases passed");
}
