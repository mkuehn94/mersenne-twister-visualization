//! C ABI used by www/js/wasm.js
//!
//! Conventions:
//! * JS writes params into `in_ptr()` (u32 words, layout below)
//!   before calls that take a config.
//! * Results come back in three growable buffers: `out32`, `outf` (f64) and
//!   `outb` (bytes); JS reads them right after the call.
//! * Long computations are jobs: `*_new` returns a handle, `*_step` advances
//!   it by a bounded amount so the page stays responsive.
//!
//! Config layout (u32 indices):
//!  0 w  1 n  2 m  3 r  4-5 a  6 u  7-8 d  9 s  10-11 b  12 t  13-14 c  15 l
//!  16-17 f  18-19 umask  20-21 lmask  22 lowbit_from_xk  23 temper
//!  24 seed_mode  25-26 zero_sub  27-28 seed_xor  29 out_shift  30-31 out_xor
//!  32-33 out_and  34 discard

#![allow(clippy::missing_safety_doc)]

use crate::bigdec;
use crate::crack;
use crate::fm;
use crate::gf2::{self, IrrJob, KdistJob, PeriodJob, SearchJob, BmJob};
use crate::mt::{Mt, MtConfig, SeedMode, wmask};
use crate::sources::{self, Source};
use crate::stats;
use crate::kinds::{self, Core, Kind};
use crate::sfmt;
use crate::vm::{MelgVm, MtVm, SfmtVm, Tiny64Vm, TinyVm, UntVm, Vm, XsaddVm};
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

static mut INBUF: [u32; 1024] = [0; 1024];
static mut OUT32: Vec<u32> = Vec::new();
static mut OUTF: Vec<f64> = Vec::new();
static mut OUTB: Vec<u8> = Vec::new();
// 1024 words is overkill for the config (35 words) but cheap enough and
// bigger data has its own buffers/SCRATCH below

enum Obj {
    Vm(Box<Vm>),
    Src(Box<Source>),
    Irr(Box<IrrJob>),
    Kd(Box<KdistJob>),
    Search(Box<SearchJob>),
    Period(Box<PeriodJob>),
    Lc(Box<BmJob>),
    Rewind(Box<crate::crack::RewindJob>),
}

static mut OBJS: Vec<Option<Obj>> = Vec::new();

fn put(o: Obj) -> u32 {
    unsafe {
        for (i, slot) in OBJS.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(o);
                return i as u32 + 1;
            }
        }
        OBJS.push(Some(o));
        OBJS.len() as u32
    }
}
fn get(h: u32) -> Option<&'static mut Obj> {
    unsafe {
        if h == 0 {
            return None;
        }
        // get_mut already handles the out-of-range case
        OBJS.get_mut(h as usize - 1).and_then(|s| s.as_mut())
    }
}
fn out32() -> &'static mut Vec<u32> {
    unsafe { &mut OUT32 }
}
fn outf() -> &'static mut Vec<f64> {
    unsafe { &mut OUTF }
}
fn push64(v: &mut Vec<u32>, x: u64) {
    v.push(x as u32);
    v.push((x >> 32) as u32);
}
fn j64(lo: u32, hi: u32) -> u64 {
    lo as u64 | ((hi as u64) << 32)
}

fn read_cfg() -> MtConfig {
    let b = unsafe { &INBUF };
    let g = |i: usize| b[i];
    let g64 = |i: usize| j64(b[i], b[i + 1]);
    let w = g(0).clamp(2, 64);
    let n = (g(1) as usize).clamp(2, 4096);
    let m = (g(2) as usize).clamp(1, n - 1);
    let r = g(3).min(w - 1);
    let mut c = MtConfig::new(w, n, m, r, g64(4), g(6), g64(7), g(9), g64(10), g(12), g64(13), g(15), g64(16));
    let wm = wmask(w);
    c.umask = g64(18) & wm;
    c.lmask = g64(20) & wm;
    c.lowbit_from_xk = g(22) != 0;
    c.temper = g(23) != 0;
    c.seed_mode = SeedMode::from_u32(g(24));
    if c.seed_mode == SeedMode::InitByArray && w != 32 && w != 64 {
        c.seed_mode = SeedMode::InitGenrand;
    }
    c.zero_sub = g64(25);
    c.seed_xor = g64(27);
    c.out_shift = g(29).min(w - 1);
    c.out_xor = g64(30);
    c.out_and = g64(32);
    c.discard = g(34);
    c
}

fn write_cfg(c: &MtConfig, v: &mut Vec<u32>) {
    v.extend_from_slice(&[c.w, c.n as u32, c.m as u32, c.r]);
    push64(v, c.a);
    v.push(c.u);
    push64(v, c.d);
    v.push(c.s);
    push64(v, c.b);
    v.push(c.t);
    push64(v, c.c);
    v.push(c.l);
    push64(v, c.f);
    push64(v, c.umask);
    push64(v, c.lmask);
    v.push(c.lowbit_from_xk as u32);
    v.push(c.temper as u32);
    v.push(c.seed_mode as u32);
    push64(v, c.zero_sub);
    push64(v, c.seed_xor);
    v.push(c.out_shift);
    push64(v, c.out_xor);
    push64(v, c.out_and);
    v.push(c.discard);
}

// -- buffers

#[no_mangle]
pub extern "C" fn in_ptr() -> *mut u32 {
    unsafe { INBUF.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn out32_ptr() -> *const u32 {
    out32().as_ptr()
}
#[no_mangle]
pub extern "C" fn out32_len() -> u32 {
    out32().len() as u32
}
#[no_mangle]
pub extern "C" fn outf_ptr() -> *const f64 {
    outf().as_ptr()
}
#[no_mangle]
pub extern "C" fn outf_len() -> u32 {
    outf().len() as u32
}
#[no_mangle]
pub extern "C" fn outb_ptr() -> *const u8 {
    unsafe { OUTB.as_ptr() }
}
#[no_mangle]
pub extern "C" fn outb_len() -> u32 {
    unsafe { OUTB.len() as u32 }
}
#[no_mangle]
pub extern "C" fn obj_free(h: u32) {
    unsafe {
        if h > 0 && (h as usize) <= OBJS.len() {
            OBJS[h as usize - 1] = None;
        }
    }
}

/// write a catalogued variant's configuration into out32 (config layout)
#[no_mangle]
pub extern "C" fn variant_cfg(id: u32) {
    let o = out32();
    o.clear();
    let cfg = sources::variant_config(id);
    write_cfg(&cfg, o);
}

/// round-trip: normalise the config in INBUF and write it back to out32
#[no_mangle]
pub extern "C" fn cfg_normalize() -> u32 {
    let c = read_cfg();
    let o = out32();
    o.clear();
    write_cfg(&c, o);
    c.p() as u32
}

// -- vms

#[no_mangle]
pub extern "C" fn vm_new_mt(seed_lo: u32, seed_hi: u32) -> u32 {
    put(Obj::Vm(Box::new(Vm::Mt(MtVm::new(read_cfg(), j64(seed_lo, seed_hi))))))
}
#[no_mangle]
pub extern "C" fn vm_new_tiny(seed: u32, mat1: u32, mat2: u32, tmat: u32) -> u32 {
    put(Obj::Vm(Box::new(Vm::Tiny(TinyVm::new(seed, mat1, mat2, tmat)))))
}
#[no_mangle]
pub extern "C" fn vm_new_sfmt(seed: u32) -> u32 {
    put(Obj::Vm(Box::new(Vm::Sfmt(SfmtVm::new(seed)))))
}
#[no_mangle]
pub extern "C" fn vm_new_sfmt_p(idx: u32, seed: u32) -> u32 {
    let p = &sfmt::PARAMS[(idx as usize).min(sfmt::PARAMS.len() - 1)];
    put(Obj::Vm(Box::new(Vm::Sfmt(SfmtVm::with(p, seed)))))
}
#[no_mangle]
pub extern "C" fn vm_new_tiny64(seed_lo: u32, seed_hi: u32, mat1: u32, mat2: u32, tmat_lo: u32, tmat_hi: u32) -> u32 {
    put(Obj::Vm(Box::new(Vm::Tiny64(Tiny64Vm::new(j64(seed_lo, seed_hi), mat1, mat2, j64(tmat_lo, tmat_hi))))))
}
#[no_mangle]
pub extern "C" fn vm_new_melg(idx: u32, seed_lo: u32, seed_hi: u32) -> u32 {
    let p = &kinds::MELG[(idx as usize).min(kinds::MELG.len() - 1)];
    put(Obj::Vm(Box::new(Vm::Melg(MelgVm::new(p, j64(seed_lo, seed_hi))))))
}
#[no_mangle]
pub extern "C" fn vm_new_xsadd(seed: u32) -> u32 {
    put(Obj::Vm(Box::new(Vm::Xsadd(XsaddVm::new(seed)))))
}
#[no_mangle]
pub extern "C" fn vm_new_unt(z_lo: u32, z_hi: u32) -> u32 {
    put(Obj::Vm(Box::new(Vm::Unt(UntVm::new(read_cfg(), j64(z_lo, z_hi))))))
}

fn vm(h: u32) -> Option<&'static mut Vm> {
    match get(h) {
        Some(Obj::Vm(v)) => Some(v),
        _ => None,
    }
}

#[no_mangle]
pub extern "C" fn vm_run(h: u32, mode: u32, arg: u32, bp: u32, max: u32) -> u32 {
    match vm(h) {
        Some(v) => v.run(mode, arg as u64, bp, max as u64) as u32,
        None => 0,
    }
}

/// out32 = [pc, depth, last_line, event, steps(2), out_count(2), write, r0, r1, r2,
///          ntrace, rows(6 each)…, ninfo, info…]
#[no_mangle]
pub extern "C" fn vm_header(h: u32) {
    let o = out32();
    o.clear();
    let Some(v) = vm(h) else { return };
    let c = v.common();
    o.extend_from_slice(&[c.pc, c.depth, c.last_line, c.event]);
    push64(o, c.steps);
    push64(o, c.out_count);
    o.push(c.write as u32);
    o.extend_from_slice(&[c.reads[0] as u32, c.reads[1] as u32, c.reads[2] as u32]);
    o.push(c.trace.len() as u32);
    for r in &c.trace {
        o.extend_from_slice(&[r.style, r.bits, r.v[0], r.v[1], r.v[2], r.v[3]]);
    }
    let mut info = Vec::new();
    v.info(&mut info);
    o.push(info.len() as u32);
    o.extend_from_slice(&info);
}

/// last `k` outputs: [index(2), value(2), raw(2), flags] each
#[no_mangle]
pub extern "C" fn vm_outputs(h: u32, k: u32) {
    let o = out32();
    o.clear();
    let Some(v) = vm(h) else { return };
    let outs = &v.common().outputs;
    let start = outs.len().saturating_sub(k as usize);
    for r in &outs[start..] {
        push64(o, r.index);
        push64(o, r.value);
        push64(o, r.raw);
        o.push(r.flags);
    }
    // if no outputs, push a zero so js doesn't crash
    if o.is_empty() {
        // 7 = one record's worth (index, value, raw, flag), js reader walks fixed strides
        for _ in 0..7 {
            o.push(0);
        }
    }
}

/// pointer to the state words: u64 for MT, u32 for SFMT, null otherwise
#[no_mangle]
pub extern "C" fn vm_state_ptr(h: u32) -> u32 {
    match vm(h) {
        Some(Vm::Mt(m)) => m.g.x.as_ptr() as u32,
        Some(Vm::Sfmt(s)) => s.s.st.as_ptr() as u32,
        Some(Vm::Melg(m)) => m.g.x.as_ptr() as u32,
        _ => 0,
    }
}
#[no_mangle]
pub extern "C" fn vm_touch_ptr(h: u32) -> u32 {
    match vm(h) {
        Some(Vm::Mt(m)) => m.touch.as_ptr() as u32,
        Some(Vm::Sfmt(s)) => s.touch.as_ptr() as u32,
        Some(Vm::Melg(m)) => m.touch.as_ptr() as u32,
        _ => 0,
    }
}
#[no_mangle]
pub extern "C" fn vm_state_len(h: u32) -> u32 {
    match vm(h) {
        Some(Vm::Mt(m)) => m.g.x.len() as u32,
        Some(Vm::Sfmt(s)) => s.s.st.len() as u32,
        Some(Vm::Tiny(_)) | Some(Vm::Xsadd(_)) => 4,
        Some(Vm::Melg(m)) => m.g.x.len() as u32,
        Some(Vm::Tiny64(_)) => 2,
        _ => 0,
    }
}
/// overwrite a state word (MT: word i; TinyMT: s[i]; SFMT: 32-bit word i)
#[no_mangle]
pub extern "C" fn vm_set_word(h: u32, i: u32, lo: u32, hi: u32) {
    let i = i as usize;
    match vm(h) {
        Some(Vm::Mt(m)) => {
            if i < m.g.x.len() {
                m.g.x[i] = j64(lo, hi) & m.g.cfg.wm();
                m.touch[i] = m.c.steps as u32;
            }
        }
        Some(Vm::Tiny(t)) => {
            if i < 4 {
                t.t.s[i] = lo;
            }
        }
        Some(Vm::Sfmt(s)) => {
            if i < s.s.st.len() {
                s.s.st[i] = lo;
            }
        }
        Some(Vm::Tiny64(t)) => {
            if i < 2 {
                t.t.s[i] = j64(lo, hi);
            }
        }
        Some(Vm::Melg(m)) => {
            if i < m.g.x.len() {
                m.g.x[i] = j64(lo, hi);
                m.touch[i] = m.c.steps as u32;
            } else if i == m.g.x.len() {
                m.g.lung = j64(lo, hi);
            }
        }
        Some(Vm::Xsadd(x)) => {
            if i < 4 {
                x.s[i] = lo;
            }
        }
        _ => {}
    }
}

// -- sources

#[no_mangle]
pub extern "C" fn src_new(kind: u32, seed_lo: u32, seed_hi: u32) -> u32 {
    let seed = j64(seed_lo, seed_hi);
    let s = match Source::new_special(kind, seed) {
        Some(s) => s,
        None => Source::Mt(Mt::new(read_cfg(), seed)),
    };
    put(Obj::Src(Box::new(s)))
}
fn src(h: u32) -> Option<&'static mut Source> {
    match get(h) {
        Some(Obj::Src(s)) => Some(s),
        _ => None,
    }
}
#[no_mangle]
pub extern "C" fn src_fill_u32(h: u32, n: u32) {
    let o = out32();
    o.clear();
    if let Some(s) = src(h) {
        for _ in 0..n {
            o.push(s.next_u32());
        }
    }
}
/// raw values (full width): pairs (lo, hi)
#[no_mangle]
pub extern "C" fn src_fill_raw(h: u32, n: u32) -> u32 {
    let o = out32();
    o.clear();
    let mut bits = 32;
    if let Some(s) = src(h) {
        for _ in 0..n {
            let (v, b) = s.next_raw();
            bits = b;
            push64(o, v);
        }
    }
    bits
}
#[no_mangle]
pub extern "C" fn src_fill_unit(h: u32, n: u32) {
    let o = outf();
    o.clear();
    if let Some(s) = src(h) {
        for _ in 0..n {
            o.push(s.next_unit());
        }
    }
}
fn hist_out(hs: stats::Hist) {
    let o = out32();
    o.clear();
    o.extend_from_slice(&hs.counts);
    let f = outf();
    f.clear();
    f.extend_from_slice(&[hs.chi2, hs.dof, hs.p]);
}
#[no_mangle]
pub extern "C" fn stat_uniform(h: u32, n: u32, bins: u32) {
    if let Some(s) = src(h) {
        hist_out(stats::uniform_hist(s, n, bins.max(1)));
    }
}
#[no_mangle]
pub extern "C" fn stat_normal(h: u32, n: u32, bins: u32, lo: f64, hi: f64) {
    if let Some(s) = src(h) {
        hist_out(stats::normal_hist(s, n, bins.max(1), lo, hi));
    }
}
#[no_mangle]
pub extern "C" fn stat_sum(h: u32, k: u32, n: u32, bins: u32) {
    if let Some(s) = src(h) {
        hist_out(stats::sum_hist(s, k.max(1), n, bins.max(1)));
    }
}
#[no_mangle]
pub extern "C" fn stat_autocorr(h: u32, n: u32, maxlag: u32) {
    if let Some(s) = src(h) {
        let r = stats::autocorr(s, n, maxlag);
        let f = outf();
        f.clear();
        f.extend_from_slice(&r);
    }
}
#[no_mangle]
pub extern "C" fn stat_pi(h: u32, n: u32, points: u32) {
    if let Some(s) = src(h) {
        let f = outf();
        f.clear();
        let r = stats::mc_pi(s, n, points);
        f.extend_from_slice(&r);
    }
}
#[no_mangle]
pub extern "C" fn stat_bitbias(h: u32, per_row: u32, rows: u32) {
    if let Some(s) = src(h) {
        let r = stats::bit_bias(s, per_row, rows);
        let f = outf();
        f.clear();
        f.extend_from_slice(&r);
    }
}
#[no_mangle]
pub extern "C" fn box_muller_fill(h: u32, n: u32) {
    let f = outf();
    f.clear();
    if let Some(s) = src(h) {
        while (f.len() as u32) < n {
            let (a, b) = stats::box_muller(s);
            f.push(a);
            f.push(b);
        }
        f.truncate(n as usize);
    }
}
#[no_mangle]
pub extern "C" fn irwin_hall_pdf(k: u32, x: f64) -> f64 {
    stats::irwin_hall_pdf(k, x)
}
#[no_mangle]
pub extern "C" fn phi_pdf(x: f64) -> f64 {
    fm::phi_pdf(x)
}
#[no_mangle]
pub extern "C" fn phi_cdf(x: f64) -> f64 {
    fm::phi_cdf(x)
}
#[no_mangle]
pub extern "C" fn chi2_pvalue(chi2: f64, dof: f64) -> f64 {
    fm::gamma_q(dof / 2.0, chi2 / 2.0)
}
#[no_mangle]
pub extern "C" fn zero_excess(twists: u32) {
    let r = stats::zero_excess(&read_cfg(), twists);
    let f = outf();
    f.clear();
    f.extend_from_slice(&r);
}

// -- gf(2)

static mut LAST_POLY: Vec<u64> = Vec::new();

/// characteristic polynomial of the configured generator (BM on 2p+64 bits).
/// out32 = exponents of the nonzero terms, descending. Returns the degree.
#[no_mangle]
pub extern "C" fn la_charpoly(seed: u32) -> u32 {
    let cfg = read_cfg();
    let (f, l) = gf2::char_poly(&cfg, seed as u64);
    let o = out32();
    o.clear();
    for i in (0..=l).rev() {
        if i / 64 < f.len() && gf2::get_bit(&f, i) {
            o.push(i as u32);
        }
    }
    unsafe {
        LAST_POLY = f;
    }
    l as u32
}

/// linear-complexity profile L(n) of bit `bit` of the tempered outputs.
/// out32 = L(1..nbits) sampled every `every` bits
#[no_mangle]
pub extern "C" fn la_profile(seed: u32, bit: u32, nbits: u32, every: u32) -> u32 {
    let cfg = read_cfg();
    let mut g = Mt::new(cfg, seed as u64);
    let bits = gf2::output_bits(&mut g, bit.min(cfg.w - 1), nbits as usize);
    let mut prof = Vec::with_capacity(nbits as usize);
    let (_, l) = gf2::berlekamp_massey(&bits, nbits as usize, Some(&mut prof));
    let o = out32();
    o.clear();
    let e = every.max(1) as usize;
    for (i, v) in prof.iter().enumerate() {
        if (i + 1) % e == 0 {
            o.push(*v);
        }
    }
    l as u32
}

/// irreducibility job for the polynomial of the last `la_charpoly` call
#[no_mangle]
pub extern "C" fn job_irr_new(sieve: u32) -> u32 {
    let f = unsafe { LAST_POLY.clone() };
    if gf2::degree(&f).unwrap_or(0) < 1 {
        return 0;
    }
    put(Obj::Irr(Box::new(IrrJob::new(&f, sieve as usize))))
}
/// out32 = [iter, p, done, result, failed_at, sparse]
#[no_mangle]
pub extern "C" fn job_irr_step(h: u32, k: u32) -> u32 {
    let o = out32();
    o.clear();
    if let Some(Obj::Irr(j)) = get(h) {
        j.step(k as usize);
        o.extend_from_slice(&[j.iter as u32, j.p() as u32, j.done as u32, j.result as u32, j.failed_at as u32, j.red.is_sparse() as u32]);
        return j.done as u32;
    }
    1
}

#[no_mangle]
pub extern "C" fn job_kdist_new() -> u32 {
    let cfg = read_cfg();
    if cfg.p() > 4500 {
        return 0;
    }
    put(Obj::Kd(Box::new(KdistJob::new(&cfg))))
}
/// computes the next k(v); out32 = k(1..v)
#[no_mangle]
pub extern "C" fn job_kdist_step(h: u32) -> u32 {
    let o = out32();
    o.clear();
    if let Some(Obj::Kd(j)) = get(h) {
        let done = j.step();
        o.extend_from_slice(&j.k);
        return done as u32;
    }
    1
}

#[no_mangle]
pub extern "C" fn job_search_new(seed: u32) -> u32 {
    let cfg = read_cfg();
    put(Obj::Search(Box::new(SearchJob::new(&cfg, seed as u64 + 0x9E37_79B9))))
}
/// out32 = [tried, rej_deg, rej_root, rej_small, rej_full, cur_iter, p, cur_a(2), nfound, found(2 each)…]
#[no_mangle]
pub extern "C" fn job_search_step(h: u32, budget: u32) -> u32 {
    let o = out32();
    o.clear();
    if let Some(Obj::Search(j)) = get(h) {
        let fin = j.step(budget as usize);
        o.extend_from_slice(&[j.tried, j.rej_deg, j.rej_root, j.rej_small, j.rej_full, j.cur_iter() as u32, j.cfg.p() as u32]);
        push64(o, j.cur_a);
        o.push(j.found.len() as u32);
        for a in j.found.iter() {
            push64(o, *a);
        }
        return fin;
    }
    0
}

/// 1 when a small-w search has tried every candidate
#[no_mangle]
pub extern "C" fn job_search_exhausted(h: u32) -> u32 {
    match get(h) {
        Some(Obj::Search(j)) => j.exhausted as u32,
        _ => 0,
    }
}

#[no_mangle]
pub extern "C" fn job_period_new(limit_lo: u32, limit_hi: u32) -> u32 {
    let cfg = read_cfg();
    if cfg.p() > 40 {
        return 0;
    }
    put(Obj::Period(Box::new(PeriodJob::new(&cfg, j64(limit_lo, limit_hi)))))
}
/// out32 = [steps(2), done, found]
#[no_mangle]
pub extern "C" fn job_period_step(h: u32, k: u32) -> u32 {
    let o = out32();
    o.clear();
    if let Some(Obj::Period(j)) = get(h) {
        j.step(k as u64);
        push64(o, j.steps);
        o.push(j.done as u32);
        o.push(j.found as u32);
        return j.done as u32;
    }
    1
}

/// p×p transition matrix: out32 = p rows of ⌈p/32⌉ u32 words. Returns p (0 if too big)
#[no_mangle]
pub extern "C" fn la_transition(maxp: u32) -> u32 {
    let cfg = read_cfg();
    let p = cfg.p();
    let o = out32();
    o.clear();
    if p > maxp as usize {
        return 0;
    }
    let rows = gf2::transition_matrix(&cfg);
    let w32 = (p + 31) / 32;
    for r in rows {
        for q in 0..w32 {
            let wv = r[q / 2];
            o.push(if q % 2 == 0 { wv as u32 } else { (wv >> 32) as u32 });
        }
    }
    p as u32
}

/// w×w matrices as images of the unit vectors e_i (i = 0 … w−1):
/// out32 = [A e_i (2)] [T e_i (2)] [T⁻¹ e_i (2)] for every i
#[no_mangle]
pub extern "C" fn la_word_matrices() {
    let cfg = read_cfg();
    let o = out32();
    o.clear();
    for i in 0..cfg.w {
        let e = 1u64 << i;
        let a = (e >> 1) ^ if e & 1 != 0 { cfg.a } else { 0 };
        push64(o, a);
    }
    for i in 0..cfg.w {
        push64(o, cfg.temper(1u64 << i));
    }
    for i in 0..cfg.w {
        push64(o, cfg.untemper(1u64 << i));
    }
}

#[no_mangle]
pub extern "C" fn temper_one(lo: u32, hi: u32) {
    let cfg = read_cfg();
    let o = out32();
    o.clear();
    push64(o, cfg.temper(j64(lo, hi)));
    push64(o, cfg.untemper(j64(lo, hi)));
}

// -- misc

#[no_mangle]
pub extern "C" fn mersenne_digits(p: u32) -> u32 {
    unsafe {
        OUTB = bigdec::mersenne_decimal(p.min(200_000));
        OUTB.len() as u32
    }
}

/// cloning demo. out32 = [n observed (2 each)] [k predicted (2 each)] [k actual (2 each)]
#[no_mangle]
pub extern "C" fn crack_demo(seed_lo: u32, seed_hi: u32, skip: u32, k: u32) {
    let cfg = read_cfg();
    let mut g = Mt::new(cfg, j64(seed_lo, seed_hi));
    for _ in 0..skip {
        g.next_tempered();
    }
    let obs: Vec<u64> = (0..cfg.n).map(|_| g.next_tempered()).collect();
    let win = crack::untemper_all(&cfg, &obs);
    let pred = crack::predict(&cfg, &win, k as usize);
    let actual: Vec<u64> = (0..k).map(|_| g.next_tempered()).collect();
    let o = out32();
    o.clear();
    for v in obs.iter().chain(pred.iter()).chain(actual.iter()) {
        push64(o, *v);
    }
}

/// n state words from SCRATCH, written by JS as (lo, hi) pairs
fn scratch_state(cfg: &MtConfig) -> Vec<u64> {
    let s = unsafe { &SCRATCH };
    (0..cfg.n).map(|i| if 2 * i + 1 < s.len() { j64(s[2 * i], s[2 * i + 1]) & cfg.wm() } else { 0 }).collect()
}

/// build a generator directly from n recovered state words written into the
/// scratch buffer as (lo, hi) pairs: the clone in the cloning demo.
/// The words are x_j … x_{j+n−1}; setting idx = n makes the next call twist,
/// which produces x_{j+n}, x_{j+n+1}, … exactly as the original would.
#[no_mangle]
pub extern "C" fn src_from_state() -> u32 {
    let cfg = read_cfg();
    let x = scratch_state(&cfg);
    let g = Mt { cfg, x, idx: cfg.n, count: 0 };
    put(Obj::Src(Box::new(Source::Mt(g))))
}

/// reserved scratch so JS can pass arrays (e.g. outputs to clone) — returns pointer
static mut SCRATCH: Vec<u32> = Vec::new();
#[no_mangle]
pub extern "C" fn scratch(n: u32) -> *mut u32 {
    unsafe {
        SCRATCH = vec![0; n as usize];
        SCRATCH.as_mut_ptr()
    }
}

// -- other kinds

/// out32 = [state_bits, lc_bit (1 = top bit, 0 = bit 0), lc_stride, native_bits,
///          range (0 = full word), core (1 MT, 2 TinyMT64, 3 MELG, 4 SFMT, 5 XSadd),
///          core_index, core_seed(2), mat1, mat2, tmat(2)] ++ MT config (if core = 1)
#[no_mangle]
pub extern "C" fn kind_info(id: u32, seed_lo: u32, seed_hi: u32) -> u32 {
    let o = out32();
    o.clear();
    let Some(k) = Kind::new(id, j64(seed_lo, seed_hi)) else { return 0 };
    o.extend_from_slice(&[k.state_bits() as u32, k.lc_bit(), k.lc_stride() as u32, k.native_bits(), k.range().unwrap_or(0.0) as u32]);
    let (core, idx, m1, m2, tm) = match &k.core {
        Core::Mt(_) => (1, 0, 0, 0, 0),
        Core::Tiny64(t) => (2, 0, t.mat1, t.mat2, t.tmat),
        Core::Melg(m) => (3, kinds::MELG.iter().position(|q| core::ptr::eq(q, m.p)).unwrap_or(0) as u32, 0, 0, 0),
        Core::Sfmt(s) => (4, sfmt::PARAMS.iter().position(|q| core::ptr::eq(q, s.p)).unwrap_or(0) as u32, 0, 0, 0),
        Core::Xsadd(_) => (5, 0, 0, 0, 0),
    };
    o.extend_from_slice(&[core, idx]);
    push64(o, k.core_seed);
    o.extend_from_slice(&[m1, m2]);
    push64(o, tm);
    if let Some(c) = k.mt_config() {
        write_cfg(&c, o);
    }
    1
}

/// Berlekamp–Massey on one output bit of a source: `top` = 1 takes the top
/// bit of each native word, 0 takes bit 0; every `stride`-th word is used.
/// kind 0 = the MT configuration in INBUF.
#[no_mangle]
pub extern "C" fn job_lc_new(kind: u32, seed_lo: u32, seed_hi: u32, nbits: u32, top: u32, stride: u32) -> u32 {
    let seed = j64(seed_lo, seed_hi);
    let mut s = match Source::new_special(kind, seed) {
        Some(s) => s,
        None => Source::Mt(Mt::new(read_cfg(), seed)),
    };
    let nbits = nbits as usize;
    let stride = stride.max(1);
    let mut seq = vec![0u64; nbits / 64 + 1];
    for i in 0..nbits {
        let mut vb = (0u64, 1u32);
        for _ in 0..stride {
            vb = s.next_raw();
        }
        let (v, b) = vb;
        let sh = if top != 0 { b - 1 } else { 0 };
        if (v >> sh) & 1 == 1 {
            seq[i / 64] |= 1 << (i % 64);
        }
    }
    put(Obj::Lc(Box::new(BmJob::new(&seq, nbits))))
}
/// run k bits; out32 = [n, nbits, L]; returns 1 when finished
#[no_mangle]
pub extern "C" fn job_lc_step(h: u32, k: u32) -> u32 {
    let o = out32();
    o.clear();
    if let Some(Obj::Lc(j)) = get(h) {
        let done = j.step(k as usize);
        o.extend_from_slice(&[j.n as u32, j.nbits as u32, j.l as u32]);
        return done as u32;
    }
    1
}

/// out32 = [mexp, n, pos1, sl1, sl2, sr1, sr2, msk×4, parity×4]
#[no_mangle]
pub extern "C" fn sfmt_params(idx: u32) -> u32 {
    let o = out32();
    o.clear();
    let Some(p) = sfmt::PARAMS.get(idx as usize) else { return 0 };
    o.extend_from_slice(&[p.mexp, p.n as u32, p.pos1 as u32, p.sl1, p.sl2, p.sr1, p.sr2]);
    o.extend_from_slice(&p.msk);
    o.extend_from_slice(&p.parity);
    sfmt::PARAMS.len() as u32
}

/// out32 = [mexp, nn, mm, p, lag1, shift1, neg, pos, a(2), mask1(2)]
#[no_mangle]
pub extern "C" fn melg_params(idx: u32) -> u32 {
    let o = out32();
    o.clear();
    let Some(p) = kinds::MELG.get(idx as usize) else { return 0 };
    o.extend_from_slice(&[p.mexp, p.nn as u32, p.mm as u32, p.p, p.lag1 as u32, p.shift1, p.neg, p.pos]);
    push64(o, p.a);
    push64(o, p.mask1);
    kinds::MELG.len() as u32
}

/// rewind the word sequence from the n raw words in SCRATCH (lo, hi pairs) back
/// to the seeded state; config in INBUF
#[no_mangle]
pub extern "C" fn job_rewind_new(max_steps: u32) -> u32 {
    let cfg = read_cfg();
    let x = scratch_state(&cfg);
    put(Obj::Rewind(Box::new(crate::crack::RewindJob::new(&cfg, &x, max_steps as u64))))
}
/// out32 = [done, steps(2), seeding (0 unknown, 1 init_genrand, 2 69069), seed(2)]
#[no_mangle]
pub extern "C" fn job_rewind_step(h: u32, k: u32) -> u32 {
    let o = out32();
    o.clear();
    if let Some(Obj::Rewind(j)) = get(h) {
        let done = j.step(k as u64);
        o.push(done as u32);
        push64(o, j.steps);
        o.push(j.found as u32);
        push64(o, j.seed);
        return done as u32;
    }
    1
}
/// out32 = the current window, n words as (lo, hi)
#[no_mangle]
pub extern "C" fn job_rewind_window(h: u32) {
    let o = out32();
    o.clear();
    if let Some(Obj::Rewind(j)) = get(h) {
        for v in j.window() {
            push64(o, v);
        }
    }
}
