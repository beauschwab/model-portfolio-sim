//! Bounded CUDA experiment, deliberately separate from production engines.
use cudarc::driver::{CudaContext, CudaFunction, CudaStream, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::{compile_ptx_with_opts, CompileOptions};
use std::ffi::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::slice;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

#[repr(C)]
pub struct Buffer {
    data: *mut f64,
    len: usize,
    shape: [usize; 3],
}
struct Gpu {
    stream: Arc<CudaStream>,
    function: CudaFunction,
}
static GPU: OnceLock<Mutex<Result<Gpu, String>>> = OnceLock::new();
static ERROR: Mutex<String> = Mutex::new(String::new());
static CANCELLED: AtomicBool = AtomicBool::new(false);
static LAUNCHES: AtomicU32 = AtomicU32::new(0);

#[no_mangle]
pub extern "C" fn portfolio_gpu_cancel() {
    CANCELLED.store(true, Ordering::Release);
}
#[no_mangle]
pub extern "C" fn portfolio_gpu_reset_cancel() {
    CANCELLED.store(false, Ordering::Release);
    LAUNCHES.store(0, Ordering::Release);
}
#[no_mangle]
pub extern "C" fn portfolio_gpu_launches() -> u32 {
    LAUNCHES.load(Ordering::Acquire)
}
fn err<E: std::fmt::Debug>(e: E) -> String {
    format!("{e:?}")
}
fn initialize() -> Result<Gpu, String> {
    let context = CudaContext::new(0).map_err(err)?;
    let options = CompileOptions {
        fmad: Some(false),
        ftz: Some(false),
        prec_div: Some(true),
        prec_sqrt: Some(true),
        arch: Some("compute_75"),
        ..Default::default()
    };
    let ptx = compile_ptx_with_opts(include_str!("cashflows.cu"), options).map_err(err)?;
    let module = context.load_module(ptx).map_err(err)?;
    let function = module.load_function("cashflows").map_err(err)?;
    Ok(Gpu {
        stream: context.default_stream(),
        function,
    })
}
fn guarded(action: impl FnOnce() -> Result<(), String>) -> i32 {
    match catch_unwind(AssertUnwindSafe(action)) {
        Ok(Ok(())) => 0,
        outcome => {
            let message = match outcome {
                Ok(Err(e)) => e,
                _ => "CUDA experiment panic".into(),
            };
            *ERROR.lock().unwrap_or_else(|e| e.into_inner()) = message;
            1
        }
    }
}
#[no_mangle]
pub extern "C" fn portfolio_gpu_initialize() -> i32 {
    guarded(|| {
        let lock = GPU
            .get_or_init(|| Mutex::new(initialize()))
            .lock()
            .map_err(err)?;
        lock.as_ref().map(|_| ()).map_err(Clone::clone)
    })
}
/// # Safety
/// `dst` must be writable for `capacity` bytes when capacity is nonzero.
#[no_mangle]
pub unsafe extern "C" fn portfolio_gpu_error(dst: *mut c_char, capacity: usize) -> usize {
    let message = ERROR.lock().unwrap_or_else(|e| e.into_inner());
    if capacity > 0 && !dst.is_null() {
        let size = message.len().min(capacity - 1);
        std::ptr::copy_nonoverlapping(message.as_ptr(), dst.cast::<u8>(), size);
        *dst.add(size) = 0;
    }
    message.len()
}
struct Packed {
    values: Vec<f64>,
    offsets: Vec<u64>,
    n: usize,
    paths: usize,
    months: usize,
}
fn pack(op: u32, input: &[Buffer]) -> Result<Packed, String> {
    if !matches!(op, 4 | 6) || input.len() != if op == 4 { 25 } else { 19 } {
        return Err("only base mortgage/deposit cashflows are supported".into());
    }
    let [paths, months, one] = input[0].shape;
    let n = input[if op == 4 { 13 } else { 6 }].len;
    if !(1..=256).contains(&n)
        || !(1..=2048).contains(&paths)
        || !(1..=360).contains(&months)
        || one != 1
    {
        return Err("admission: 1..256 contracts, 1..2048 paths, 1..360 months".into());
    }
    if input
        .iter()
        .try_fold(0usize, |sum, b| sum.checked_add(b.len))
        .is_none_or(|cells| cells > 128 * 1024 * 1024 / 8)
    {
        return Err("input exceeds 128 MiB".into());
    }
    let mut values = Vec::new();
    let mut offsets = Vec::new();
    let mut arrays = Vec::new();
    for (i, b) in input.iter().enumerate() {
        if b.data.is_null()
            || b.len == 0
            || b.shape.contains(&0)
            || b.shape.iter().try_fold(1usize, |a, b| a.checked_mul(*b)) != Some(b.len)
        {
            return Err("invalid descriptor".into());
        }
        // Safety: caller owns live aligned buffers for the documented lengths.
        let a = unsafe { slice::from_raw_parts(b.data, b.len) };
        if a.iter().any(|v| !v.is_finite()) {
            return Err("nonfinite input".into());
        }
        arrays.push(a);
        offsets.push(values.len() as u64);
        if i < 4 {
            if b.shape != [paths, months, 1] {
                return Err("path shape mismatch".into());
            }
            for m in 0..months {
                for p in 0..paths {
                    values.push(a[p * months + m]);
                }
            }
        } else {
            values.extend_from_slice(a);
        }
    }
    offsets.push(values.len() as u64);
    if values.len() * 8 > 128 * 1024 * 1024 {
        return Err("input exceeds 128 MiB".into());
    }
    let forward = if op == 4 { 23 } else { 18 };
    if arrays[forward] != [0.0] {
        return Err("forward capture is unsupported".into());
    }
    let scalar = if op == 4 {
        vec![10, 12, 23, 24]
    } else {
        vec![14, 15, 18]
    };
    if scalar.iter().any(|&i| arrays[i].len() != 1) {
        return Err("scalar shape mismatch".into());
    }
    let vectors = if op == 4 { 13..22 } else { 6..14 };
    if vectors.clone().any(|i| arrays[i].len() != n) {
        return Err("contract shape mismatch".into());
    }
    let (knots, coefs) = if op == 4 { (7, 8) } else { (4, 5) };
    if arrays[knots].len() < 2
        || arrays[coefs].len() != 4 * (arrays[knots].len() - 1)
        || arrays[knots].windows(2).any(|w| w[0] >= w[1])
    {
        return Err("invalid spline".into());
    }
    if op == 4
        && (arrays[6].len() < 9
            || arrays[9].len() < 2
            || arrays[11].len() < 2
            || arrays[4].len() != months
            || arrays[4]
                .iter()
                .any(|v| *v < 0.0 || v.fract() != 0.0 || *v >= arrays[5].len() as f64)
            || arrays[15]
                .iter()
                .any(|v| *v < 1.0 || *v > 4096.0 || v.fract() != 0.0)
            || arrays[19].iter().any(|v| *v <= 0.0)
            || arrays[1].iter().any(|v| *v <= 0.0))
    {
        return Err("invalid mortgage domain".into());
    }
    if arrays[3].iter().any(|v| *v < 0.0) {
        return Err("invalid deflator".into());
    }
    Ok(Packed {
        values,
        offsets,
        n,
        paths,
        months,
    })
}
/// Prepared-input experiment. Repetitions share device inputs/output storage.
/// Timing: mean CUDA kernel seconds, Rust full-call seconds, input bytes,
/// current-month scratch bytes, output bytes, numeric device allocation bytes.
/// Deadlines are checked between stages and launches, before publication.
/// # Safety
/// Descriptors own live aligned nonoverlapping arrays through return;
/// `timing` owns six f64 cells. Outputs are modified only after full success.
#[no_mangle]
pub unsafe extern "C" fn portfolio_gpu_call(
    op: u32,
    input: *const Buffer,
    input_count: usize,
    output: *const Buffer,
    output_count: usize,
    repeats: u32,
    deadline_ms: u64,
    timing: *mut f64,
) -> i32 {
    guarded(|| {
        let start = Instant::now();
        let check = || {
            if CANCELLED.load(Ordering::Acquire) {
                return Err("cancelled; output unpublished".to_string());
            }
            if start.elapsed().as_millis() >= deadline_ms as u128 {
                Err("deadline exceeded; output unpublished".to_string())
            } else {
                Ok(())
            }
        };
        check()?;
        if input.is_null()
            || output.is_null()
            || timing.is_null()
            || input_count > 25
            || !(1..=20).contains(&repeats)
        {
            return Err("invalid FFI arguments".into());
        }
        let input = unsafe { slice::from_raw_parts(input, input_count) };
        let output = unsafe { slice::from_raw_parts(output, output_count) };
        let a = pack(op, input)?;
        let [n, p, t] = [a.n, a.paths, a.months];
        let horizon = input[if op == 4 { 22 } else { 17 }].len;
        let shapes = if op == 4 {
            vec![
                [n, t, 1],
                [n, horizon, 1],
                [n, horizon, 1],
                [n, p, horizon],
                [n, p, horizon],
                [n, t, 1],
                [n, t, 1],
            ]
        } else {
            vec![
                [n, t, 1],
                [n, t, 1],
                [n, horizon, 1],
                [n, horizon, 1],
                [n, p, horizon],
                [n, t, 1],
            ]
        };
        if horizon > 32
            || output.len() != shapes.len()
            || output.iter().zip(&shapes).any(|(o, s)| {
                o.data.is_null() || o.shape != *s || s.iter().product::<usize>() != o.len
            })
        {
            return Err("output shape mismatch".into());
        }
        check()?;
        let lock = GPU
            .get_or_init(|| Mutex::new(initialize()))
            .lock()
            .map_err(err)?;
        let gpu = lock.as_ref().map_err(Clone::clone)?;
        check()?;
        let device_input = gpu.stream.clone_htod(&a.values).map_err(err)?;
        let device_offsets = gpu.stream.clone_htod(&a.offsets).map_err(err)?;
        let scratch_len = n * p * 3;
        let mut scratch = gpu.stream.alloc_zeros::<f64>(scratch_len).map_err(err)?;
        let mut result = gpu.stream.alloc_zeros::<f64>(n * t * 3).map_err(err)?;
        let cfg = LaunchConfig {
            grid_dim: (n as u32, 1, 1),
            block_dim: (256, 1, 1),
            shared_mem_bytes: 0,
        };
        let mut samples = Vec::new();
        for _ in 0..repeats {
            check()?;
            let begin = gpu
                .stream
                .record_event(Some(cudarc::driver::sys::CUevent_flags::CU_EVENT_DEFAULT))
                .map_err(err)?;
            let [op_i, p_i, t_i, n_i] = [op as i32, p as i32, t as i32, n as i32];
            let mut builder = gpu.stream.launch_builder(&gpu.function);
            builder
                .arg(&device_input)
                .arg(&device_offsets)
                .arg(&mut scratch)
                .arg(&mut result)
                .arg(&op_i)
                .arg(&p_i)
                .arg(&t_i)
                .arg(&n_i);
            // Safety: validated dimensions, offsets and allocated capacities.
            unsafe { builder.launch(cfg) }.map_err(err)?;
            LAUNCHES.fetch_add(1, Ordering::Release);
            let end = gpu
                .stream
                .record_event(Some(cudarc::driver::sys::CUevent_flags::CU_EVENT_DEFAULT))
                .map_err(err)?;
            end.synchronize().map_err(err)?;
            samples.push(begin.elapsed_ms(&end).map_err(err)? as f64 / 1000.0);
        }
        check()?;
        let compact = gpu.stream.clone_dtoh(&result).map_err(err)?;
        if compact.iter().any(|v| !v.is_finite()) {
            return Err("nonfinite CUDA result".into());
        }
        let mut staged: Vec<Vec<f64>> = output.iter().map(|o| vec![0.; o.len]).collect();
        let span = n * t;
        let indices = if op == 4 { [0, 5, 6] } else { [0, 5, 1] };
        for (i, j) in indices.into_iter().enumerate() {
            staged[j].copy_from_slice(&compact[i * span..(i + 1) * span]);
        }
        check()?;
        let stats = [
            samples.iter().sum::<f64>() / repeats as f64,
            start.elapsed().as_secs_f64(),
            (a.values.len() * 8) as f64,
            (scratch_len * 8) as f64,
            (span * 3 * 8) as f64,
            ((a.values.len() + scratch_len + span * 3) * 8 + a.offsets.len() * 8) as f64,
        ];
        for (values, out) in staged.iter().zip(output) {
            unsafe { slice::from_raw_parts_mut(out.data, out.len) }.copy_from_slice(values);
        }
        unsafe { slice::from_raw_parts_mut(timing, 6) }.copy_from_slice(&stats);
        Ok(())
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_inputs_fail_before_cuda() {
        assert!(pack(9, &[]).is_err());
        let mut timing = [-1.; 6];
        let status = unsafe {
            portfolio_gpu_call(
                4,
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
                1,
                0,
                timing.as_mut_ptr(),
            )
        };
        assert_ne!(status, 0);
        assert_eq!(timing, [-1.; 6]);
    }
}
