#![cfg_attr(target_arch = "wasm32", no_std)]
#![allow(clippy::needless_range_loop)]

extern crate alloc;

pub mod bigdec;
pub mod crack;
pub mod fm;
pub mod gf2;
pub mod kinds;
pub mod mt;
pub mod params;
pub mod sfmt;
pub mod sources;
pub mod stats;
pub mod tinymt;
pub mod vm;

#[cfg(target_arch = "wasm32")]
mod allocator;
#[cfg(target_arch = "wasm32")]
pub mod api;

#[cfg(all(target_arch = "wasm32", not(test)))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // no console to print to, so just trap
    core::arch::wasm32::unreachable()
}
