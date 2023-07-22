//! A small size-class allocator for wasm32.
//!
//! Blocks up to 64 KiB live in power-of-two free lists; larger blocks are
//! whole pages kept in a first-fit free list. `dealloc` receives the layout,
//! so no block headers are needed. Single-threaded by construction (wasm32).

use core::alloc::{GlobalAlloc, Layout};
use core::arch::wasm32::memory_grow;

const PAGE: usize = 65536;
const CLASSES: usize = 13; // 16 B .. 64 KiB
const MAX_BIG: usize = 512;

struct State {
    heads: [usize; CLASSES],
    bump: usize,
    bump_end: usize,
    big: [(usize, usize); MAX_BIG], // (ptr, pages); pages == 0 means empty slot
}

static mut ST: State = State { heads: [0; CLASSES], bump: 0, bump_end: 0, big: [(0, 0); MAX_BIG] };

pub struct Alloc;

#[global_allocator]
static GLOBAL: Alloc = Alloc;

fn class_of(size: usize) -> usize {
    let s = if size < 16 { 16 } else { size.next_power_of_two() };
    s.trailing_zeros() as usize - 4
}

unsafe fn grow(pages: usize) -> usize {
    let prev = memory_grow(0, pages);
    if prev == usize::MAX {
        core::arch::wasm32::unreachable()
    }
    prev * PAGE
}

unsafe fn big_alloc(pages: usize) -> *mut u8 {
    unsafe {
        let st = &mut *core::ptr::addr_of_mut!(ST);
        // first fit
        for i in 0..MAX_BIG {
            let (p, n) = st.big[i];
            if n >= pages {
                if n == pages {
                    st.big[i] = (0, 0);
                } else {
                    st.big[i] = (p + pages * PAGE, n - pages);
                }
                return p as *mut u8;
            }
        }
        grow(pages) as *mut u8
    }
}

unsafe fn big_free(ptr: usize, pages: usize) {
    unsafe {
        let st = &mut *core::ptr::addr_of_mut!(ST);
        // coalesce with a neighbour if possible
        for i in 0..MAX_BIG {
            let (p, n) = st.big[i];
            if n > 0 && p + n * PAGE == ptr {
                st.big[i].1 += pages;
                return;
            }
            if n > 0 && ptr + pages * PAGE == p {
                st.big[i] = (ptr, n + pages);
                return;
            }
        }
        for i in 0..MAX_BIG {
            if st.big[i].1 == 0 {
                st.big[i] = (ptr, pages);
                return;
            }
        }
        // table full: leak (never happens in practice)
    }
}

unsafe impl GlobalAlloc for Alloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe {
            let size = layout.size().max(layout.align());
            if size > PAGE {
                return big_alloc((size + PAGE - 1) / PAGE);
            }
            let c = class_of(size);
            let st = &mut *core::ptr::addr_of_mut!(ST);
            let head = st.heads[c];
            if head != 0 {
                st.heads[c] = *(head as *const usize);
                return head as *mut u8;
            }
            let bsize = 16usize << c;
            let mut p = (st.bump + bsize - 1) & !(bsize - 1);
            if p + bsize > st.bump_end || st.bump == 0 {
                let base = big_alloc(4) as usize; // 256 KiB slab
                st.bump = base;
                st.bump_end = base + 4 * PAGE;
                p = (st.bump + bsize - 1) & !(bsize - 1);
            }
            st.bump = p + bsize;
            p as *mut u8
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe {
            let size = layout.size().max(layout.align());
            if size > PAGE {
                big_free(ptr as usize, (size + PAGE - 1) / PAGE);
                return;
            }
            let c = class_of(size);
            let st = &mut *core::ptr::addr_of_mut!(ST);
            *(ptr as *mut usize) = st.heads[c];
            st.heads[c] = ptr as usize;
        }
    }
}
