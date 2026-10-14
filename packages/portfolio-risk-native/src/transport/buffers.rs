//! C-compatible borrowed buffer descriptor; ownership stays with the caller.

#[repr(C)]
pub struct Buffer {
    pub data: *mut f64,
    pub len: usize,
    pub shape: [usize; 3],
}
