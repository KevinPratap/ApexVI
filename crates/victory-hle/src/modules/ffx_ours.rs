//! the ffx-api upscaler on a `victory_gpu::upscale` backend (spatial, dlss) instead of amd's pc dll
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::c_void;

use victory_gpu::object::Object;
use victory_gpu::upscale::{INPUT_STATE, OUTPUT_STATE, UpscaleFrame, UpscaleSetup};

const OK: u32 = 0;
const ERROR: u32 = 1;
const UNKNOWN_TYPE: u32 = 2;
const RUNTIME_ERROR: u32 = 3;
const PARAMETER: u32 = 6;

const CREATE_UPSCALE: u64 = 0x10000;
const DISPATCH_UPSCALE: u64 = 0x10001;
const QUERY_RATIO: u64 = 0x10002;
const QUERY_RENDER_SIZE: u64 = 0x10003;
const QUERY_PHASE_COUNT: u64 = 0x10004;
const QUERY_JITTER: u64 = 0x10005;
const QUERY_MEMORY: u64 = 0x10008;

// FfxApiResource, and where the dispatch's seven start
const RESOURCE: usize = 48;
const RESOURCES: usize = 24;

struct Context {
    setup: UpscaleSetup,
    frames: u64,
}

fn read<T: Copy>(at: *const c_void, offset: usize) -> T {
    // SAFETY: the title passes ffx-api structures of the size their type implies
    unsafe { at.cast::<u8>().add(offset).cast::<T>().read_unaligned() }
}

fn write<T>(to: *mut T, value: T) {
    if !to.is_null() {
        // SAFETY: an out pointer the title gave
        unsafe { to.write_unaligned(value) };
    }
}

fn chain(mut desc: *const c_void) -> Vec<(u64, *const c_void)> {
    let mut all = Vec::new();
    while !desc.is_null() && all.len() < 16 {
        all.push((read::<u64>(desc, 0), desc));
        desc = read::<*const c_void>(desc, 8);
    }
    all
}

fn ratio(quality: u32) -> f32 {
    match quality {
        1 => 1.5,
        2 => 1.7,
        3 => 2.0,
        4 => 3.0,
        _ => 1.0,
    }
}

fn halton(mut index: i32, base: i32) -> f32 {
    let (mut f, mut r) = (1.0f32, 0.0f32);
    while index > 0 {
        f /= base as f32;
        r += f * (index % base) as f32;
        index /= base;
    }
    r
}

fn phase_count(render_width: u32, display_width: u32) -> i32 {
    let scale = display_width as f32 / render_width.max(1) as f32;
    (8.0 * scale * scale).ceil() as i32
}

fn jitter(index: i32, phases: i32) -> (f32, f32) {
    let i = index.rem_euclid(phases.max(1)) + 1;
    (halton(i, 2) - 0.5, halton(i, 3) - 0.5)
}

fn spatial_no_jitter() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        let on = std::env::var("VICTORY_SPATIAL_NO_JITTER")
            .map(|v| matches!(v.trim(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);
        if on {
            tracing::info!("spatial upscaler queries report 1 phase and zero jitter (VICTORY_SPATIAL_NO_JITTER=1)");
        }
        on
    }) && victory_gpu::upscale::chosen() == "spatial"
}

pub(crate) fn create(context: *mut *mut c_void, desc: *const c_void) -> u32 {
    if context.is_null() {
        return PARAMETER;
    }
    let Some(&(_, up)) = chain(desc).iter().find(|l| l.0 == CREATE_UPSCALE) else {
        return UNKNOWN_TYPE;
    };
    let setup = UpscaleSetup {
        flags: read(up, 16),
        max_render: [read(up, 20), read(up, 24)],
        max_output: [read(up, 28), read(up, 32)],
    };
    if !victory_gpu::upscale::setup(&setup) {
        return RUNTIME_ERROR;
    }
    write(context, Box::into_raw(Box::new(Context { setup, frames: 0 })).cast());
    OK
}

pub(crate) fn destroy(context: *mut *mut c_void) -> u32 {
    if context.is_null() {
        return PARAMETER;
    }
    // SAFETY: a context made by `create`
    let ours = unsafe { *context }.cast::<Context>();
    if !ours.is_null() {
        // SAFETY: from Box::into_raw in `create`
        let gone = unsafe { Box::from_raw(ours) };
        tracing::info!(setup = ?gone.setup, frames = gone.frames, "upscaler context gone");
        victory_gpu::upscale::release();
        write(context, std::ptr::null_mut());
    }
    OK
}

pub(crate) fn query(desc: *const c_void) -> u32 {
    let mut result = OK;
    for (kind, d) in chain(desc) {
        match kind {
            QUERY_RATIO => write(read::<*mut f32>(d, 24), ratio(read(d, 16))),
            QUERY_RENDER_SIZE => {
                let r = ratio(read(d, 24));
                write(read::<*mut u32>(d, 32), (read::<u32>(d, 16) as f32 / r) as u32);
                write(read::<*mut u32>(d, 40), (read::<u32>(d, 20) as f32 / r) as u32);
            }
            QUERY_PHASE_COUNT => {
                let n = if spatial_no_jitter() { 1 } else { phase_count(read(d, 16), read(d, 20)) };
                write(read::<*mut i32>(d, 24), n);
            }
            QUERY_JITTER => {
                let (x, y) = if spatial_no_jitter() { (0.0, 0.0) } else { jitter(read(d, 16), read(d, 20)) };
                write(read::<*mut f32>(d, 24), x);
                write(read::<*mut f32>(d, 32), y);
            }
            QUERY_MEMORY => write(read::<*mut [u64; 2]>(d, 16), [0, 0]),
            _ => {
                tracing::debug!(kind = format_args!("{kind:#x}"), "upscaler query not handled");
                result = UNKNOWN_TYPE;
            }
        }
    }
    result
}

pub(crate) fn dispatch(context: *mut *mut c_void, desc: *const c_void) -> u32 {
    // SAFETY: a context made by `create`, if any
    let ours = if context.is_null() { std::ptr::null_mut() } else { unsafe { *context }.cast::<Context>() };
    if ours.is_null() || desc.is_null() || read::<u64>(desc, 0) != DISPATCH_UPSCALE {
        return PARAMETER;
    }
    let d = desc;
    let list = read::<*const Object>(d, 16);
    let texture = |i: usize, state: u32| {
        let r = read::<*const Object>(d, RESOURCES + i * RESOURCE);
        (!r.is_null()).then(|| victory_gpu::external::host_texture(list, r, state).map(|t| t.0)).flatten()
    };
    let inputs: Vec<_> = (0..6).map(|i| texture(i, INPUT_STATE)).collect();
    let (Some(colour), Some(output)) = (inputs[0].clone(), texture(6, OUTPUT_STATE)) else {
        tracing::warn!("upscaler: no host copy of colour or output; frame skipped");
        return ERROR;
    };
    let frame = UpscaleFrame {
        colour: &colour,
        depth: inputs[1].as_ref(),
        motion: inputs[2].as_ref(),
        exposure: inputs[3].as_ref(),
        reactive: inputs[4].as_ref(),
        transparency: inputs[5].as_ref(),
        output: &output,
        jitter: [read(d, 360), read(d, 364)],
        motion_scale: [read(d, 368), read(d, 372)],
        render_size: [read(d, 376), read(d, 380)],
        output_size: [read(d, 384), read(d, 388)],
        sharpen: (read::<u8>(d, 392) != 0).then(|| read(d, 396)),
        frame_ms: read(d, 400),
        pre_exposure: read(d, 404),
        reset: read::<u8>(d, 408) != 0,
        near: read(d, 412),
        far: read(d, 416),
        fov_y: read(d, 420),
        flags: read(d, 428),
    };
    // SAFETY: our context, on the title's recording thread
    let frames = unsafe { &mut (*ours).frames };
    if *frames < 3 || frames.is_power_of_two() {
        tracing::info!(frame = *frames, render = ?frame.render_size, output = ?frame.output_size, jitter = ?frame.jitter, reset = frame.reset, "upscale");
    }
    *frames += 1;
    match victory_gpu::external::record(list, |l| victory_gpu::upscale::dispatch(l, &frame)) {
        Some(true) => OK,
        _ => ERROR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jitter_matches_fsr() {
        assert_eq!(phase_count(1707, 2560), 18);
        assert_eq!(jitter(0, 8), (0.0, halton(1, 3) - 0.5));
        assert_eq!(jitter(8, 8), jitter(0, 8));
    }

    #[test]
    fn render_size_query_fills_out_pointers() {
        let (mut w, mut h) = (0u32, 0u32);
        #[repr(C)]
        struct Q {
            kind: u64,
            next: usize,
            dw: u32,
            dh: u32,
            quality: u32,
            pad: u32,
            w: *mut u32,
            h: *mut u32,
        }
        let q = Q { kind: QUERY_RENDER_SIZE, next: 0, dw: 2560, dh: 1440, quality: 1, pad: 0, w: &mut w, h: &mut h };
        assert_eq!(query((&raw const q).cast()), OK);
        assert_eq!((w, h), (1706, 960));
    }

    #[test]
    fn create_needs_an_upscale_desc() {
        let mut c = std::ptr::null_mut();
        let header = [0x100_0002u64, 0];
        assert_eq!(create(&mut c, header.as_ptr().cast()), UNKNOWN_TYPE);
    }
}
