//! the debuggers. each one is a line-by-line interpreter for the pseudocode
//! shown on the page — you hit "step" and it executes exactly one line,
//! records what values were read/written (that's the bit inspector), and
//! stops. run() layers step-over / continue / run-to-output on top.
//!
//! the pc always points at the *next* line to execute, like a real debugger.

use crate::mt::{Mt, MtConfig, SeedMode, shl, shr};
use crate::sfmt::{self, Sfmt, W128};
use crate::kinds::{Melg, MelgParams, T64_MASK, TinyMt64, xsadd_next_state};
use crate::params as P;
use crate::params::TINYMT32_MASK as TMASK;
use crate::tinymt::TinyMt;
use alloc::vec;
use alloc::vec::Vec;

// row styles for the bit inspector on the js side
pub const ST_VALUE: u32 = 0;
pub const ST_CONST: u32 = 1;
pub const ST_TEMP: u32 = 2;
pub const ST_RESULT: u32 = 3;
pub const ST_SHIFT: u32 = 4;
pub const ST_KNOWN: u32 = 5;

// bitmask of what happened on the last step (the js checks these)
pub const EV_OUTPUT: u32 = 1;
pub const EV_RET_SEED: u32 = 2;
pub const EV_RET_DEEP: u32 = 4; // returned from depth-2 frame (twist / gen_all)
pub const EV_CALL: u32 = 8;
pub const EV_DONE: u32 = 16;

#[derive(Clone, Copy, Default)]
pub struct Row {
    pub style: u32,
    pub bits: u32,
    pub v: [u32; 4],
}

fn row64(style: u32, bits: u32, x: u64) -> Row {
    Row { style, bits, v: [x as u32, (x >> 32) as u32, 0, 0] }
}

fn row128(style: u32, x: &W128) -> Row {
    Row { style, bits: 128, v: *x }
}

#[derive(Clone, Copy)]
pub struct OutRec {
    pub index: u64,
    pub value: u64,
    pub raw: u64, // tempered value before the output transform
    pub flags: u32, // 1 = discarded
}

pub struct Common {
    pub pc: u32,
    pub depth: u32,
    pub last_line: u32,
    pub event: u32,
    pub steps: u64,
    pub trace: Vec<Row>,
    pub reads: [i32; 3],
    pub write: i32, // -1 = no write this step, -2 = whole state (bulk seed)
    pub outputs: Vec<OutRec>,
    pub out_count: u64,
}

impl Common {
    fn new(pc: u32) -> Common {
        Common {
            pc,
            depth: 1,
            last_line: u32::MAX,
            event: 0,
            steps: 0,
            trace: Vec::new(),
            reads: [-1; 3],
            write: -1,
            outputs: Vec::new(),
            out_count: 0,
        }
    }
    fn begin(&mut self) {
        self.trace.clear();
        self.event = 0;
        self.reads = [-1; 3];
        self.write = -1;
        self.last_line = self.pc;
        self.steps += 1;
    }
    fn emit(&mut self, value: u64, raw: u64, discarded: bool) {
        let rec = OutRec { index: self.out_count, value, raw, flags: discarded as u32 };
        self.out_count += 1;
        if self.outputs.len() >= 256 {
            self.outputs.remove(0);
        }
        self.outputs.push(rec);
        self.event |= EV_OUTPUT;
    }
}

// ============================================================================
//  MT (generic parameters)
// ============================================================================

pub mod mtl {
    pub const SEED_HDR: u32 = 0;
    pub const SEED_X0: u32 = 1;
    pub const SEED_FOR: u32 = 2;
    pub const SEED_BODY: u32 = 3;
    pub const SEED_IDX: u32 = 4;
    pub const NEXT_HDR: u32 = 5;
    pub const NEXT_IF: u32 = 6;
    pub const NEXT_LOAD: u32 = 7;
    pub const T1: u32 = 8;
    pub const T2: u32 = 9;
    pub const T3: u32 = 10;
    pub const T4: u32 = 11;
    pub const RET: u32 = 12;
    pub const TW_HDR: u32 = 13;
    pub const TW_FOR: u32 = 14;
    pub const TW_Y: u32 = 15;
    pub const TW_SHR: u32 = 16;
    pub const TW_ODD: u32 = 17;
    pub const TW_W: u32 = 18;
    pub const TW_END: u32 = 19;
}

pub struct MtVm {
    pub c: Common,
    pub g: Mt,
    pub seed: u64,
    pub eff_seed: u64,
    pub i: usize,
    pub k: usize,
    pub y: u64,
    pub v: u64,
    pub cokus: u64,
    pub discard_left: u32,
    pub touch: Vec<u32>, // step number of last write (0 = never)
    pub twists: u32,
}

impl MtVm {
    pub fn new(cfg: MtConfig, seed: u64) -> MtVm {
        let n = cfg.n;
        let g = Mt { cfg, x: vec![0; n], idx: n, count: 0 };
        MtVm {
            c: Common::new(mtl::SEED_X0),
            g,
            seed,
            eff_seed: seed,
            i: 0,
            k: 0,
            y: 0,
            v: 0,
            cokus: 0,
            discard_left: 0,
            touch: vec![0; n],
            twists: 0,
        }
    }

    // track which state words were written at which step (for the ring highlight)
    fn touch(&mut self, i: usize) {
        self.touch[i] = self.c.steps as u32;
    }

    pub fn loop_range(&self, pc: u32) -> Option<(u32, u32)> {
        use mtl::*;
        if (SEED_FOR..=SEED_BODY).contains(&pc) {
            Some((SEED_FOR, SEED_BODY))
        } else if (TW_FOR..=TW_W).contains(&pc) {
            Some((TW_FOR, TW_W))
        } else {
            None
        }
    }

    pub fn step(&mut self) {
        use mtl::*;
        self.c.begin();
        let cfg = self.g.cfg;
        let n = cfg.n;
        let w = cfg.w;
        let wm = cfg.wm();
        let tr = &mut self.c.trace;
        match self.c.pc {
            SEED_X0 => {
                let s = self.g.effective_seed(self.seed);
                self.eff_seed = s;
                tr.push(row64(ST_VALUE, 64, self.seed));
                if s != self.seed {
                    tr.push(row64(ST_TEMP, 64, s));
                }
                match cfg.seed_mode {
                    sm if sm.bulk() => {
                        self.g.seed_raw(s);
                        self.c.trace.push(row64(ST_RESULT, w, self.g.x[0]));
                        self.c.write = -2;
                        let st = self.c.steps as u32;
                        for t in self.touch.iter_mut() {
                            *t = st;
                        }
                        self.c.pc = SEED_IDX;
                    }
                    SeedMode::Cokus => {
                        let x = (s | 1) & wm;
                        self.cokus = x;
                        self.g.x[0] = x;
                        tr.push(row64(ST_RESULT, w, x));
                        self.c.write = 0;
                        self.touch(0);
                        self.i = 1;
                        self.c.pc = SEED_FOR;
                    }
                    _ => {
                        self.g.x[0] = s & wm;
                        tr.push(row64(ST_RESULT, w, s & wm));
                        self.c.write = 0;
                        self.touch(0);
                        self.i = 1;
                        self.c.pc = SEED_FOR;
                    }
                }
            }
            SEED_FOR => {
                self.c.pc = if self.i < n { SEED_BODY } else { SEED_IDX };
            }
            SEED_BODY => {
                let i = self.i;
                let prev = self.g.x[i - 1];
                self.c.reads[0] = (i - 1) as i32;
                let res = match cfg.seed_mode {
                    SeedMode::Knuth69069 => {
                        let r = prev.wrapping_mul(P::KNUTH_MUL) & wm;
                        tr.push(row64(ST_VALUE, w, prev));
                        tr.push(row64(ST_RESULT, w, r));
                        r
                    }
                    SeedMode::Cokus => {
                        let r = self.cokus.wrapping_mul(P::KNUTH_MUL) & wm;
                        self.cokus = r;
                        tr.push(row64(ST_VALUE, w, prev));
                        tr.push(row64(ST_RESULT, w, r));
                        r
                    }
                    _ => {
                        let sh = shr(prev, w.saturating_sub(2));
                        let xr = prev ^ sh;
                        let mul = cfg.f.wrapping_mul(xr) & wm;
                        let r = mul.wrapping_add(i as u64) & wm;
                        tr.push(row64(ST_VALUE, w, prev));
                        tr.push(row64(ST_SHIFT, w, sh));
                        tr.push(row64(ST_TEMP, w, xr));
                        tr.push(row64(ST_TEMP, w, mul));
                        tr.push(row64(ST_RESULT, w, r));
                        r
                    }
                };
                self.g.x[i] = res;
                self.c.write = i as i32;
                self.touch(i);
                self.i += 1;
                self.c.pc = SEED_FOR;
            }
            SEED_IDX => {
                self.g.idx = n;
                self.discard_left = cfg.discard;
                self.c.event |= EV_RET_SEED;
                self.c.pc = NEXT_IF;
            }
            NEXT_IF => {
                if self.g.idx >= n {
                    self.c.depth = 2;
                    self.k = 0;
                    self.c.event |= EV_CALL;
                    self.c.pc = TW_FOR;
                } else {
                    self.c.pc = NEXT_LOAD;
                }
            }
            NEXT_LOAD => {
                let idx = self.g.idx;
                self.y = self.g.x[idx];
                self.c.reads[0] = idx as i32;
                self.g.idx += 1;
                tr.push(row64(ST_RESULT, w, self.y));
                self.c.pc = T1;
            }
            T1 | T2 | T3 => {
                let y0 = self.y;
                let (sh, mask) = match self.c.pc {
                    T1 => (shr(y0, cfg.u), cfg.d),
                    T2 => (shl(y0, cfg.s, w), cfg.b),
                    _ => (shl(y0, cfg.t, w), cfg.c),
                };
                tr.push(row64(ST_VALUE, w, y0));
                if cfg.temper {
                    let and = sh & mask;
                    self.y = (y0 ^ and) & wm;
                    tr.push(row64(ST_SHIFT, w, sh));
                    tr.push(row64(ST_CONST, w, mask));
                    tr.push(row64(ST_TEMP, w, and));
                }
                tr.push(row64(ST_RESULT, w, self.y));
                self.c.pc += 1;
            }
            T4 => {
                let y0 = self.y;
                tr.push(row64(ST_VALUE, w, y0));
                if cfg.temper {
                    let sh = shr(y0, cfg.l);
                    self.y = (y0 ^ sh) & wm;
                    tr.push(row64(ST_SHIFT, w, sh));
                }
                tr.push(row64(ST_RESULT, w, self.y));
                self.c.pc = RET;
            }
            RET => {
                let z = self.y;
                let out = cfg.out_transform(z);
                tr.push(row64(ST_VALUE, w, z));
                tr.push(row64(ST_RESULT, w, out));
                let disc = self.discard_left > 0;
                if disc {
                    self.discard_left -= 1;
                }
                self.g.count += 1;
                self.c.emit(out, z, disc);
                self.c.pc = NEXT_IF;
            }
            TW_FOR => {
                self.c.pc = if self.k < n { TW_Y } else { TW_END };
            }
            TW_Y => {
                let k = self.k;
                let xk = self.g.x[k];
                let xk1 = self.g.x[(k + 1) % n];
                self.y = (xk & cfg.umask).wrapping_add(xk1 & cfg.lmask) & wm;
                self.c.reads = [k as i32, ((k + 1) % n) as i32, -1];
                tr.push(row64(ST_VALUE, w, xk));
                tr.push(row64(ST_CONST, w, cfg.umask));
                tr.push(row64(ST_VALUE, w, xk1));
                tr.push(row64(ST_CONST, w, cfg.lmask));
                tr.push(row64(ST_RESULT, w, self.y));
                self.c.pc = TW_SHR;
            }
            TW_SHR => {
                self.v = self.y >> 1;
                tr.push(row64(ST_VALUE, w, self.y));
                tr.push(row64(ST_RESULT, w, self.v));
                self.c.pc = TW_ODD;
            }
            TW_ODD => {
                let low = if cfg.lowbit_from_xk { self.g.x[self.k] & 1 } else { self.y & 1 };
                let v0 = self.v;
                let add = if low != 0 { cfg.a } else { 0 };
                self.v = v0 ^ add;
                tr.push(row64(ST_VALUE, w, v0));
                tr.push(row64(ST_CONST, w, add));
                tr.push(row64(ST_RESULT, w, self.v));
                self.c.pc = TW_W;
            }
            TW_W => {
                let k = self.k;
                let km = (k + cfg.m) % n;
                let res = (self.g.x[km] ^ self.v) & wm;
                self.c.reads = [km as i32, -1, -1];
                tr.push(row64(ST_VALUE, w, self.g.x[km]));
                tr.push(row64(ST_VALUE, w, self.v));
                tr.push(row64(ST_RESULT, w, res));
                self.g.x[k] = res;
                self.c.write = k as i32;
                self.touch(k);
                self.k += 1;
                self.c.pc = TW_FOR;
            }
            TW_END => {
                self.g.idx = 0;
                self.twists += 1;
                self.c.depth = 1;
                self.c.event |= EV_RET_DEEP;
                self.c.pc = NEXT_LOAD;
            }
            _ => {
                self.c.pc = NEXT_IF;
            }
        }
    }

    /// variables for the watch panel
    pub fn info(&self, out: &mut Vec<u32>) {
        out.extend_from_slice(&[
            self.i as u32,
            self.k as u32,
            self.g.idx as u32,
            self.y as u32,
            (self.y >> 32) as u32,
            self.v as u32,
            (self.v >> 32) as u32,
            self.eff_seed as u32,
            (self.eff_seed >> 32) as u32,
            self.twists,
            self.discard_left,
            self.cokus as u32,
        ]);
    }
}

// ============================================================================
//  TinyMT32
// ============================================================================

pub mod tml {
    pub const INIT_HDR: u32 = 0;
    pub const INIT_SET: u32 = 1;
    pub const INIT_MIX: u32 = 2;
    pub const INIT_WARM: u32 = 3;
    pub const NEXT_HDR: u32 = 4;
    pub const L_Y: u32 = 5;
    pub const L_X: u32 = 6;
    pub const L_XSH: u32 = 7;
    pub const L_YMIX: u32 = 8;
    pub const L_ROT: u32 = 9;
    pub const L_MAT: u32 = 10;
    pub const L_T1: u32 = 11;
    pub const L_T0: u32 = 12;
    pub const L_TMAT: u32 = 13;
    pub const L_RET: u32 = 14;
}

pub struct TinyVm {
    pub c: Common,
    pub t: TinyMt,
    pub seed: u32,
    pub x: u32,
    pub y: u32,
    pub t0: u32,
    pub t1: u32,
    pub touch: [u32; 4],
}

impl TinyVm {
    pub fn new(seed: u32, mat1: u32, mat2: u32, tmat: u32) -> TinyVm {
        TinyVm {
            c: Common::new(tml::INIT_SET),
            t: TinyMt { s: [0; 4], mat1, mat2, tmat },
            seed,
            x: 0,
            y: 0,
            t0: 0,
            t1: 0,
            touch: [0; 4],
        }
    }
    fn touch_all(&mut self) {
        self.touch = [self.c.steps as u32; 4];
    }
    pub fn step(&mut self) {
        use tml::*;
        self.c.begin();
        let st = self.c.steps as u32;
        let tr = &mut self.c.trace;
        let s = &mut self.t.s;
        match self.c.pc {
            INIT_SET => {
                *s = [self.seed, self.t.mat1, self.t.mat2, self.t.tmat];
                for q in 0..4 {
                    tr.push(row64(ST_RESULT, 32, s[q] as u64));
                }
                self.touch_all();
                self.c.write = -2;
                self.c.pc = INIT_MIX;
            }
            INIT_MIX => {
                for i in 1..P::TINYMT32_MIN_LOOP {
                    let p = s[((i - 1) & 3) as usize];
                    s[(i & 3) as usize] ^= i.wrapping_add((P::INIT_GENRAND_F32 as u32).wrapping_mul(p ^ (p >> 30)));
                }
                if (s[0] & TMASK) == 0 && s[1] == 0 && s[2] == 0 && s[3] == 0 {
                    *s = P::TINYMT32_ZERO_FIX;
                }
                for q in 0..4 {
                    tr.push(row64(ST_RESULT, 32, s[q] as u64));
                }
                self.touch_all();
                self.c.write = -2;
                self.c.pc = INIT_WARM;
            }
            INIT_WARM => {
                for _ in 0..P::TINYMT32_PRE_LOOP {
                    self.t.next_state();
                }
                for q in 0..4 {
                    self.c.trace.push(row64(ST_RESULT, 32, self.t.s[q] as u64));
                }
                self.touch_all();
                self.c.write = -2;
                self.c.event |= EV_RET_SEED;
                self.c.pc = L_Y;
            }
            L_Y => {
                self.y = s[3];
                self.c.reads[0] = 3;
                tr.push(row64(ST_RESULT, 32, self.y as u64));
                self.c.pc = L_X;
            }
            L_X => {
                let a = s[0] & TMASK;
                self.x = a ^ s[1] ^ s[2];
                self.c.reads = [0, 1, 2];
                tr.push(row64(ST_VALUE, 32, a as u64));
                tr.push(row64(ST_VALUE, 32, s[1] as u64));
                tr.push(row64(ST_VALUE, 32, s[2] as u64));
                tr.push(row64(ST_RESULT, 32, self.x as u64));
                self.c.pc = L_XSH;
            }
            L_XSH => {
                let x0 = self.x;
                self.x = x0 ^ (x0 << P::TINYMT32_SH0);
                tr.push(row64(ST_VALUE, 32, x0 as u64));
                tr.push(row64(ST_SHIFT, 32, (x0 << P::TINYMT32_SH0) as u64));
                tr.push(row64(ST_RESULT, 32, self.x as u64));
                self.c.pc = L_YMIX;
            }
            L_YMIX => {
                let y0 = self.y;
                self.y = y0 ^ (y0 >> P::TINYMT32_SH0) ^ self.x;
                tr.push(row64(ST_VALUE, 32, y0 as u64));
                tr.push(row64(ST_SHIFT, 32, (y0 >> P::TINYMT32_SH0) as u64));
                tr.push(row64(ST_VALUE, 32, self.x as u64));
                tr.push(row64(ST_RESULT, 32, self.y as u64));
                self.c.pc = L_ROT;
            }
            L_ROT => {
                s[0] = s[1];
                s[1] = s[2];
                s[2] = self.x ^ (self.y << P::TINYMT32_SH1);
                s[3] = self.y;
                tr.push(row64(ST_VALUE, 32, self.x as u64));
                tr.push(row64(ST_SHIFT, 32, (self.y << P::TINYMT32_SH1) as u64));
                tr.push(row64(ST_RESULT, 32, s[2] as u64));
                self.touch = [st; 4];
                self.c.write = -2;
                self.c.pc = L_MAT;
            }
            L_MAT => {
                let odd = self.y & 1 != 0;
                let (m1, m2) = if odd { (self.t.mat1, self.t.mat2) } else { (0, 0) };
                tr.push(row64(ST_CONST, 32, m1 as u64));
                s[1] ^= m1;
                tr.push(row64(ST_RESULT, 32, s[1] as u64));
                tr.push(row64(ST_CONST, 32, m2 as u64));
                s[2] ^= m2;
                tr.push(row64(ST_RESULT, 32, s[2] as u64));
                if odd {
                    self.touch[1] = st;
                    self.touch[2] = st;
                    self.c.write = 1;
                }
                self.c.pc = L_T1;
            }
            L_T1 => {
                let sh = s[2] >> P::TINYMT32_SH8;
                self.t1 = s[0].wrapping_add(sh);
                self.c.reads = [0, 2, -1];
                tr.push(row64(ST_VALUE, 32, s[0] as u64));
                tr.push(row64(ST_SHIFT, 32, sh as u64));
                tr.push(row64(ST_RESULT, 32, self.t1 as u64));
                self.c.pc = L_T0;
            }
            L_T0 => {
                self.t0 = s[3] ^ self.t1;
                self.c.reads = [3, -1, -1];
                tr.push(row64(ST_VALUE, 32, s[3] as u64));
                tr.push(row64(ST_VALUE, 32, self.t1 as u64));
                tr.push(row64(ST_RESULT, 32, self.t0 as u64));
                self.c.pc = L_TMAT;
            }
            L_TMAT => {
                let add = if self.t1 & 1 != 0 { self.t.tmat } else { 0 };
                let t00 = self.t0;
                self.t0 ^= add;
                tr.push(row64(ST_VALUE, 32, t00 as u64));
                tr.push(row64(ST_CONST, 32, add as u64));
                tr.push(row64(ST_RESULT, 32, self.t0 as u64));
                self.c.pc = L_RET;
            }
            L_RET => {
                tr.push(row64(ST_RESULT, 32, self.t0 as u64));
                let t0 = self.t0 as u64;
                self.c.emit(t0, t0, false);
                self.c.pc = L_Y;
            }
            _ => self.c.pc = L_Y,
        }
    }
    pub fn info(&self, out: &mut Vec<u32>) {
        let s = &self.t.s;
        out.extend_from_slice(&[s[0], s[1], s[2], s[3], self.x, self.y, self.t0, self.t1, self.t.mat1, self.t.mat2, self.t.tmat]);
        out.extend_from_slice(&self.touch);
    }
}

// ============================================================================
//  SFMT19937
// ============================================================================

pub mod sfl {
    pub const INIT_HDR: u32 = 0;
    pub const INIT_FILL: u32 = 1;
    pub const INIT_CERT: u32 = 2;
    pub const NEXT_HDR: u32 = 3;
    pub const NEXT_IF: u32 = 4;
    pub const NEXT_RET: u32 = 5;
    pub const GEN_HDR: u32 = 6;
    pub const GEN_R: u32 = 7;
    pub const GEN_FOR: u32 = 8;
    pub const GEN_AB: u32 = 9;
    pub const GEN_REC: u32 = 10;
    pub const GEN_SHIFT: u32 = 11;
    pub const GEN_END: u32 = 12;
}

pub struct SfmtVm {
    pub c: Common,
    pub s: Sfmt,
    pub seed: u32,
    pub i: usize,
    pub r1: W128,
    pub r2: W128,
    pub a: W128,
    pub b: W128,
    pub touch: Vec<u32>, // per 128-bit block
    pub cert_flipped: bool,
}

impl SfmtVm {
    pub fn new(seed: u32) -> SfmtVm {
        SfmtVm::with(sfmt::sfmt19937(), seed)
    }
    pub fn with(p: &'static sfmt::SfmtParams, seed: u32) -> SfmtVm {
        SfmtVm {
            c: Common::new(sfl::INIT_FILL),
            s: Sfmt::blank(p),
            seed,
            i: 0,
            r1: [0; 4],
            r2: [0; 4],
            a: [0; 4],
            b: [0; 4],
            touch: vec![0; p.n],
            cert_flipped: false,
        }
    }
    pub fn loop_range(&self, pc: u32) -> Option<(u32, u32)> {
        if (sfl::GEN_FOR..=sfl::GEN_SHIFT).contains(&pc) { Some((sfl::GEN_FOR, sfl::GEN_SHIFT)) } else { None }
    }
    pub fn step(&mut self) {
        use sfl::*;
        self.c.begin();
        let st = self.c.steps as u32;
        match self.c.pc {
            INIT_FILL => {
                self.s.seed_fill(self.seed);
                for t in self.touch.iter_mut() {
                    *t = st;
                }
                self.c.write = -2;
                self.c.trace.push(row128(ST_RESULT, &self.s.block(0)));
                self.c.pc = INIT_CERT;
            }
            INIT_CERT => {
                let before = self.s.block(0);
                self.cert_flipped = self.s.period_certification();
                self.c.trace.push(row128(ST_VALUE, &before));
                self.c.trace.push(row128(ST_CONST, &self.s.p.parity));
                self.c.trace.push(row128(ST_RESULT, &self.s.block(0)));
                if self.cert_flipped {
                    self.touch[0] = st;
                    self.c.write = 0;
                }
                self.c.event |= EV_RET_SEED;
                self.c.pc = NEXT_IF;
            }
            NEXT_IF => {
                if self.s.idx >= self.s.n32() {
                    self.c.depth = 2;
                    self.c.event |= EV_CALL;
                    self.c.pc = GEN_R;
                } else {
                    self.c.pc = NEXT_RET;
                }
            }
            NEXT_RET => {
                let idx = self.s.idx;
                let v = self.s.st[idx];
                self.s.idx += 1;
                self.c.reads[0] = (idx / 4) as i32;
                self.c.trace.push(row64(ST_RESULT, 32, v as u64));
                self.c.emit(v as u64, v as u64, false);
                self.c.pc = NEXT_IF;
            }
            GEN_R => {
                let n = self.s.p.n;
                self.r1 = self.s.block(n - 2);
                self.r2 = self.s.block(n - 1);
                self.c.reads = [(n - 2) as i32, (n - 1) as i32, -1];
                self.c.trace.push(row128(ST_VALUE, &self.r1));
                self.c.trace.push(row128(ST_VALUE, &self.r2));
                self.i = 0;
                self.c.pc = GEN_FOR;
            }
            GEN_FOR => {
                self.c.pc = if self.i < self.s.p.n { GEN_AB } else { GEN_END };
            }
            GEN_AB => {
                let i = self.i;
                let j = (i + self.s.p.pos1) % self.s.p.n;
                self.a = self.s.block(i);
                self.b = self.s.block(j);
                self.c.reads = [i as i32, j as i32, -1];
                self.c.trace.push(row128(ST_VALUE, &self.a));
                self.c.trace.push(row128(ST_VALUE, &self.b));
                self.c.pc = GEN_REC;
            }
            GEN_REC => {
                let t = sfmt::recursion_terms(self.s.p, &self.a, &self.b, &self.r1, &self.r2);
                let tr = &mut self.c.trace;
                tr.push(row128(ST_VALUE, &self.a));
                tr.push(row128(ST_SHIFT, &t[0]));
                tr.push(row128(ST_TEMP, &t[1]));
                tr.push(row128(ST_SHIFT, &t[2]));
                tr.push(row128(ST_TEMP, &t[3]));
                tr.push(row128(ST_RESULT, &t[4]));
                self.s.set_block(self.i, &t[4]);
                self.touch[self.i] = st;
                self.c.write = self.i as i32;
                self.c.pc = GEN_SHIFT;
            }
            GEN_SHIFT => {
                self.r1 = self.r2;
                self.r2 = self.s.block(self.i);
                self.c.trace.push(row128(ST_VALUE, &self.r1));
                self.c.trace.push(row128(ST_RESULT, &self.r2));
                self.i += 1;
                self.c.pc = GEN_FOR;
            }
            GEN_END => {
                self.s.idx = 0;
                self.c.depth = 1;
                self.c.event |= EV_RET_DEEP;
                self.c.pc = NEXT_RET;
            }
            _ => self.c.pc = NEXT_IF,
        }
    }
    pub fn info(&self, out: &mut Vec<u32>) {
        out.extend_from_slice(&[self.i as u32, self.s.idx as u32, self.cert_flipped as u32]);
        out.extend_from_slice(&self.r1);
        out.extend_from_slice(&self.r2);
        let p = self.s.p;
        out.extend_from_slice(&[p.mexp, p.n as u32, p.pos1 as u32, p.sl1, p.sl2, p.sr1, p.sr2]);
        out.extend_from_slice(&p.msk);
        out.extend_from_slice(&p.parity);
    }
}

// ============================================================================
//  untempering
// ============================================================================

pub mod unl {
    pub const HDR: u32 = 0;
    pub const SET: u32 = 1;
    pub const INV_L: u32 = 2;
    pub const INV_T: u32 = 3;
    pub const INV_S: u32 = 4;
    pub const INV_U: u32 = 5;
    pub const RET: u32 = 6;
}

pub struct UntVm {
    pub c: Common,
    pub cfg: MtConfig,
    pub z: u64,
    pub y: u64,
    pub x: u64,
    pub j: u32,
    pub done: bool,
}

impl UntVm {
    pub fn new(cfg: MtConfig, z: u64) -> UntVm {
        UntVm { c: Common::new(unl::SET), cfg, z, y: 0, x: 0, j: 0, done: false }
    }
    /// (shift, mask, right?) for the inversion line
    fn params(&self, line: u32) -> (u32, u64, bool) {
        let c = &self.cfg;
        match line {
            unl::INV_L => (c.l, u64::MAX & c.wm(), true),
            unl::INV_T => (c.t, c.c, false),
            unl::INV_S => (c.s, c.b, false),
            _ => (c.u, c.d, true),
        }
    }
    pub fn loop_range(&self, pc: u32) -> Option<(u32, u32)> {
        if (unl::INV_L..=unl::INV_U).contains(&pc) { Some((pc, pc)) } else { None }
    }
    pub fn step(&mut self) {
        use unl::*;
        self.c.begin();
        let w = self.cfg.w;
        let wm = self.cfg.wm();
        match self.c.pc {
            SET => {
                self.y = self.z & wm;
                self.x = self.y;
                self.j = 0;
                self.c.trace.push(row64(ST_RESULT, w, self.y));
                self.c.pc = INV_L;
            }
            INV_L | INV_T | INV_S | INV_U => {
                let line = self.c.pc;
                let (k, m, right) = self.params(line);
                let tempering_off = !self.cfg.temper;
                let reps = if k == 0 || k >= w || tempering_off { 0 } else { (w + k - 1) / k - 1 };
                if self.j == 0 {
                    self.x = self.y;
                }
                if reps == 0 {
                    self.c.trace.push(row64(ST_VALUE, w, self.y));
                    self.c.trace.push(row64(ST_RESULT, w, self.y));
                    self.c.trace.push(row64(ST_KNOWN, w, wm));
                    self.c.pc += 1;
                } else {
                    let term = if right { shr(self.x, k) & m } else { shl(self.x, k, w) & m };
                    let xn = (self.y ^ term) & wm;
                    let nknown = ((self.j + 2) * k).min(w);
                    let known = if right {
                        // undoing a right shift recovers bits from the top down
                        if nknown >= w { wm } else { wm & !crate::mt::wmask(w - nknown) }
                    } else {
                        crate::mt::wmask(nknown)
                    };
                    let tr = &mut self.c.trace;
                    tr.push(row64(ST_VALUE, w, self.y));
                    tr.push(row64(ST_TEMP, w, self.x));
                    tr.push(row64(ST_SHIFT, w, term));
                    tr.push(row64(ST_RESULT, w, xn));
                    tr.push(row64(ST_KNOWN, w, known));
                    self.x = xn;
                    self.j += 1;
                    if self.j >= reps {
                        self.y = self.x;
                        self.j = 0;
                        self.c.pc += 1;
                    }
                }
            }
            RET => {
                self.c.trace.push(row64(ST_RESULT, w, self.y));
                if !self.done {
                    let y = self.y;
                    self.c.emit(y, self.z, false);
                }
                self.done = true;
                self.c.event |= EV_DONE;
            }
            _ => self.c.pc = SET,
        }
    }
    pub fn info(&self, out: &mut Vec<u32>) {
        let c = &self.cfg;
        let line = if (unl::INV_L..=unl::INV_U).contains(&self.c.pc) { self.c.pc } else { unl::INV_L };
        let (k, _, _) = self.params(line);
        let reps = if k == 0 || k >= c.w || !c.temper { 0 } else { (c.w + k - 1) / k - 1 };
        out.extend_from_slice(&[
            self.z as u32,
            (self.z >> 32) as u32,
            self.y as u32,
            (self.y >> 32) as u32,
            self.x as u32,
            (self.x >> 32) as u32,
            self.j,
            reps,
            self.done as u32,
        ]);
    }
}

// ============================================================================
//  TinyMT64
// ============================================================================

pub mod t64l {
    pub const INIT_HDR: u32 = 0;
    pub const INIT_SET: u32 = 1;
    pub const INIT_MIX: u32 = 2;
    pub const NEXT_HDR: u32 = 3;
    pub const L_X: u32 = 4;
    pub const L_SH12: u32 = 5;
    pub const L_SH32R: u32 = 6;
    pub const L_SH32L: u32 = 7;
    pub const L_SH11: u32 = 8;
    pub const L_ROT: u32 = 9;
    pub const L_MAT: u32 = 10;
    pub const L_T1: u32 = 11;
    pub const L_T2: u32 = 12;
    pub const L_TMAT: u32 = 13;
    pub const L_RET: u32 = 14;
}

pub struct Tiny64Vm {
    pub c: Common,
    pub t: TinyMt64,
    pub seed: u64,
    pub x: u64,
    pub out: u64,
    pub touch: [u32; 2],
}

impl Tiny64Vm {
    pub fn new(seed: u64, mat1: u32, mat2: u32, tmat: u64) -> Tiny64Vm {
        Tiny64Vm { c: Common::new(t64l::INIT_SET), t: TinyMt64 { s: [0; 2], mat1, mat2, tmat }, seed, x: 0, out: 0, touch: [0; 2] }
    }
    pub fn step(&mut self) {
        use t64l::*;
        self.c.begin();
        let st = self.c.steps as u32;
        let tr = &mut self.c.trace;
        let s = &mut self.t.s;
        let x0 = self.x;
        match self.c.pc {
            INIT_SET => {
                *s = [self.seed ^ ((self.t.mat1 as u64) << 32), self.t.mat2 as u64 ^ self.t.tmat];
                tr.push(row64(ST_RESULT, 64, s[0]));
                tr.push(row64(ST_RESULT, 64, s[1]));
                self.touch = [st; 2];
                self.c.write = -2;
                self.c.pc = INIT_MIX;
            }
            INIT_MIX => {
                let seed = self.seed;
                self.t.seed(seed);
                tr.push(row64(ST_RESULT, 64, self.t.s[0]));
                tr.push(row64(ST_RESULT, 64, self.t.s[1]));
                self.touch = [st; 2];
                self.c.write = -2;
                self.c.event |= EV_RET_SEED;
                self.c.pc = L_X;
            }
            L_X => {
                s[0] &= T64_MASK;
                self.x = s[0] ^ s[1];
                self.c.reads = [0, 1, -1];
                tr.push(row64(ST_VALUE, 64, s[0]));
                tr.push(row64(ST_VALUE, 64, s[1]));
                tr.push(row64(ST_RESULT, 64, self.x));
                self.c.pc = L_SH12;
            }
            L_SH12 | L_SH32R | L_SH32L | L_SH11 => {
                let pc = self.c.pc;
                let sh = match pc {
                    L_SH12 => x0 << P::TINYMT64_SH0,
                    L_SH32R => x0 >> 32,
                    L_SH32L => x0 << 32,
                    _ => x0 << P::TINYMT64_SH1,
                };
                self.x = x0 ^ sh;
                tr.push(row64(ST_VALUE, 64, x0));
                tr.push(row64(ST_SHIFT, 64, sh));
                tr.push(row64(ST_RESULT, 64, self.x));
                self.c.pc = pc + 1;
            }
            L_ROT => {
                s[0] = s[1];
                s[1] = self.x;
                tr.push(row64(ST_RESULT, 64, s[0]));
                tr.push(row64(ST_RESULT, 64, s[1]));
                self.touch = [st; 2];
                self.c.write = -2;
                self.c.pc = L_MAT;
            }
            L_MAT => {
                let odd = self.x & 1 != 0;
                let (m1, m2) = if odd { (self.t.mat1 as u64, (self.t.mat2 as u64) << 32) } else { (0, 0) };
                tr.push(row64(ST_CONST, 64, m1));
                s[0] ^= m1;
                tr.push(row64(ST_RESULT, 64, s[0]));
                tr.push(row64(ST_CONST, 64, m2));
                s[1] ^= m2;
                tr.push(row64(ST_RESULT, 64, s[1]));
                if odd {
                    self.touch = [st; 2];
                    self.c.write = -2;
                }
                self.c.pc = L_T1;
            }
            L_T1 => {
                self.out = s[0].wrapping_add(s[1]);
                self.c.reads = [0, 1, -1];
                tr.push(row64(ST_VALUE, 64, s[0]));
                tr.push(row64(ST_VALUE, 64, s[1]));
                tr.push(row64(ST_RESULT, 64, self.out));
                self.c.pc = L_T2;
            }
            L_T2 => {
                let o = self.out;
                self.out = o ^ (s[0] >> P::TINYMT64_SH8);
                self.c.reads = [0, -1, -1];
                tr.push(row64(ST_VALUE, 64, o));
                tr.push(row64(ST_SHIFT, 64, s[0] >> P::TINYMT64_SH8));
                tr.push(row64(ST_RESULT, 64, self.out));
                self.c.pc = L_TMAT;
            }
            L_TMAT => {
                let o = self.out;
                let add = if o & 1 != 0 { self.t.tmat } else { 0 };
                self.out = o ^ add;
                tr.push(row64(ST_VALUE, 64, o));
                tr.push(row64(ST_CONST, 64, add));
                tr.push(row64(ST_RESULT, 64, self.out));
                self.c.pc = L_RET;
            }
            L_RET => {
                tr.push(row64(ST_RESULT, 64, self.out));
                let o = self.out;
                self.c.emit(o, o, false);
                self.c.pc = L_X;
            }
            _ => self.c.pc = L_X,
        }
    }
    pub fn info(&self, out: &mut Vec<u32>) {
        let s = &self.t.s;
        for v in [s[0], s[1], self.x, self.out, self.t.tmat] {
            out.extend_from_slice(&[v as u32, (v >> 32) as u32]);
        }
        out.extend_from_slice(&[self.t.mat1, self.t.mat2]);
        out.extend_from_slice(&self.touch);
    }
}

// ============================================================================
//  MELG (Harase & Kimoto): MT-like recursion + one extra word, the "lung"
// ============================================================================

pub mod mgl {
    pub const SEED_HDR: u32 = 0;
    pub const SEED_X0: u32 = 1;
    pub const SEED_FOR: u32 = 2;
    pub const SEED_LUNG: u32 = 3;
    pub const NEXT_HDR: u32 = 4;
    pub const L_X: u32 = 5;
    pub const L_LUNG: u32 = 6;
    pub const L_XI: u32 = 7;
    pub const L_Z: u32 = 8;
    pub const L_LAG: u32 = 9;
    pub const L_RET: u32 = 10;
}

pub struct MelgVm {
    pub c: Common,
    pub g: Melg,
    pub seed: u64,
    pub i: usize,
    pub x: u64,
    pub z: u64,
    pub touch: Vec<u32>, // nn words + lung
}

impl MelgVm {
    pub fn new(p: &'static MelgParams, seed: u64) -> MelgVm {
        MelgVm { c: Common::new(mgl::SEED_X0), g: Melg { p, x: vec![0; p.nn], lung: 0, i: 0 }, seed, i: 0, x: 0, z: 0, touch: vec![0; p.nn + 1] }
    }
    pub fn loop_range(&self, pc: u32) -> Option<(u32, u32)> {
        if pc == mgl::SEED_FOR { Some((mgl::SEED_FOR, mgl::SEED_FOR)) } else { None }
    }
    pub fn step(&mut self) {
        use mgl::*;
        self.c.begin();
        let st = self.c.steps as u32;
        let p = self.g.p;
        let nn = p.nn;
        let tr = &mut self.c.trace;
        const F: u64 = P::MELG_SEED_F;
        match self.c.pc {
            SEED_X0 => {
                self.g.x[0] = self.seed;
                tr.push(row64(ST_RESULT, 64, self.seed));
                self.touch[0] = st;
                self.c.write = 0;
                self.i = 1;
                self.c.pc = SEED_FOR;
            }
            SEED_FOR => {
                let i = self.i;
                let q = self.g.x[i - 1];
                let r = F.wrapping_mul(q ^ (q >> 62)).wrapping_add(i as u64);
                self.g.x[i] = r;
                self.c.reads[0] = (i - 1) as i32;
                tr.push(row64(ST_VALUE, 64, q));
                tr.push(row64(ST_SHIFT, 64, q >> 62));
                tr.push(row64(ST_RESULT, 64, r));
                self.touch[i] = st;
                self.c.write = i as i32;
                self.i += 1;
                if self.i >= nn {
                    self.c.pc = SEED_LUNG;
                }
            }
            SEED_LUNG => {
                let q = self.g.x[nn - 1];
                self.g.lung = F.wrapping_mul(q ^ (q >> 62)).wrapping_add(nn as u64);
                self.g.i = 0;
                self.c.reads[0] = (nn - 1) as i32;
                tr.push(row64(ST_VALUE, 64, q));
                tr.push(row64(ST_RESULT, 64, self.g.lung));
                self.touch[nn] = st;
                self.c.write = nn as i32;
                self.c.event |= EV_RET_SEED;
                self.c.pc = L_X;
            }
            L_X => {
                let i = self.g.i;
                let mu = p.masku();
                self.x = (self.g.x[i] & mu) | (self.g.x[(i + 1) % nn] & !mu);
                self.c.reads = [i as i32, ((i + 1) % nn) as i32, -1];
                tr.push(row64(ST_VALUE, 64, self.g.x[i]));
                tr.push(row64(ST_CONST, 64, mu));
                tr.push(row64(ST_VALUE, 64, self.g.x[(i + 1) % nn]));
                tr.push(row64(ST_CONST, 64, !mu));
                tr.push(row64(ST_RESULT, 64, self.x));
                self.c.pc = L_LUNG;
            }
            L_LUNG => {
                let i = self.g.i;
                let x = self.x;
                let ax = (x >> 1) ^ if x & 1 != 0 { p.a } else { 0 };
                let xm = self.g.x[(i + p.mm) % nn];
                let l0 = self.g.lung;
                let ln = l0 ^ (l0 << p.neg);
                self.g.lung = ax ^ xm ^ ln;
                self.c.reads = [((i + p.mm) % nn) as i32, nn as i32, -1];
                tr.push(row64(ST_VALUE, 64, ax));
                tr.push(row64(ST_VALUE, 64, xm));
                tr.push(row64(ST_VALUE, 64, ln));
                tr.push(row64(ST_RESULT, 64, self.g.lung));
                self.touch[nn] = st;
                self.c.write = nn as i32;
                self.c.pc = L_XI;
            }
            L_XI => {
                let i = self.g.i;
                let l = self.g.lung;
                let lp = l ^ (l >> p.pos);
                self.g.x[i] = self.x ^ lp;
                self.c.reads = [nn as i32, -1, -1];
                tr.push(row64(ST_VALUE, 64, self.x));
                tr.push(row64(ST_VALUE, 64, lp));
                tr.push(row64(ST_RESULT, 64, self.g.x[i]));
                self.touch[i] = st;
                self.c.write = i as i32;
                self.c.pc = L_Z;
            }
            L_Z => {
                let xi = self.g.x[self.g.i];
                let sh = xi << p.shift1;
                self.z = xi ^ sh;
                tr.push(row64(ST_VALUE, 64, xi));
                tr.push(row64(ST_SHIFT, 64, sh));
                tr.push(row64(ST_RESULT, 64, self.z));
                self.c.pc = L_LAG;
            }
            L_LAG => {
                let j = (self.g.i + p.lag1) % nn;
                let t = self.g.x[j] & p.mask1;
                let z0 = self.z;
                self.z = z0 ^ t;
                self.c.reads = [j as i32, -1, -1];
                tr.push(row64(ST_VALUE, 64, z0));
                tr.push(row64(ST_VALUE, 64, self.g.x[j]));
                tr.push(row64(ST_CONST, 64, p.mask1));
                tr.push(row64(ST_RESULT, 64, self.z));
                self.c.pc = L_RET;
            }
            L_RET => {
                self.g.i = (self.g.i + 1) % nn;
                tr.push(row64(ST_RESULT, 64, self.z));
                let z = self.z;
                self.c.emit(z, z, false);
                self.c.pc = L_X;
            }
            _ => self.c.pc = L_X,
        }
    }
    pub fn info(&self, out: &mut Vec<u32>) {
        let p = self.g.p;
        out.extend_from_slice(&[self.i as u32, self.g.i as u32]);
        for v in [self.x, self.z, self.g.lung, p.a, p.mask1] {
            out.extend_from_slice(&[v as u32, (v >> 32) as u32]);
        }
        out.extend_from_slice(&[p.mexp, p.nn as u32, p.mm as u32, p.p, p.lag1 as u32, p.shift1, p.neg, p.pos]);
    }
}

// ============================================================================
//  XSadd (Saito & Matsumoto 2014): 128-bit xorshift with an additive output
// ============================================================================

pub mod xsl {
    pub const INIT_HDR: u32 = 0;
    pub const INIT_SET: u32 = 1;
    pub const INIT_MIX: u32 = 2;
    pub const INIT_WARM: u32 = 3;
    pub const NEXT_HDR: u32 = 4;
    pub const L_T: u32 = 5;
    pub const L_T18: u32 = 6;
    pub const L_T11: u32 = 7;
    pub const L_ROT: u32 = 8;
    pub const L_RET: u32 = 9;
}

pub struct XsaddVm {
    pub c: Common,
    pub s: [u32; 4],
    pub seed: u32,
    pub t: u32,
    pub out: u32,
}

impl XsaddVm {
    pub fn new(seed: u32) -> XsaddVm {
        XsaddVm { c: Common::new(xsl::INIT_SET), s: [0; 4], seed, t: 0, out: 0 }
    }
    pub fn step(&mut self) {
        use xsl::*;
        self.c.begin();
        let tr = &mut self.c.trace;
        let s = &mut self.s;
        match self.c.pc {
            INIT_SET => {
                *s = [self.seed, 0, 0, 0];
                for q in s.iter() {
                    tr.push(row64(ST_RESULT, 32, *q as u64));
                }
                self.c.write = -2;
                self.c.pc = INIT_MIX;
            }
            INIT_MIX => {
                for i in 1..P::XSADD_LOOP {
                    let p = s[((i - 1) & 3) as usize];
                    s[(i & 3) as usize] ^= i.wrapping_add(P::XSADD_SEED_F.wrapping_mul(p ^ (p >> 30)));
                }
                if *s == [0; 4] {
                    *s = P::XSADD_ZERO_FIX;
                }
                for q in s.iter() {
                    tr.push(row64(ST_RESULT, 32, *q as u64));
                }
                self.c.write = -2;
                self.c.pc = INIT_WARM;
            }
            INIT_WARM => {
                for _ in 0..P::XSADD_LOOP {
                    xsadd_next_state(s);
                }
                for q in s.iter() {
                    tr.push(row64(ST_RESULT, 32, *q as u64));
                }
                self.c.write = -2;
                self.c.event |= EV_RET_SEED;
                self.c.pc = L_T;
            }
            L_T => {
                let t = s[0];
                self.t = t ^ (t << P::XSADD_SH1);
                self.c.reads[0] = 0;
                tr.push(row64(ST_VALUE, 32, t as u64));
                tr.push(row64(ST_SHIFT, 32, (t << P::XSADD_SH1) as u64));
                tr.push(row64(ST_RESULT, 32, self.t as u64));
                self.c.pc = L_T18;
            }
            L_T18 => {
                let t = self.t;
                self.t = t ^ (t >> P::XSADD_SH2);
                tr.push(row64(ST_VALUE, 32, t as u64));
                tr.push(row64(ST_SHIFT, 32, (t >> P::XSADD_SH2) as u64));
                tr.push(row64(ST_RESULT, 32, self.t as u64));
                self.c.pc = L_T11;
            }
            L_T11 => {
                let t = self.t;
                self.t = t ^ (s[3] << P::XSADD_SH3);
                self.c.reads[0] = 3;
                tr.push(row64(ST_VALUE, 32, t as u64));
                tr.push(row64(ST_SHIFT, 32, (s[3] << P::XSADD_SH3) as u64));
                tr.push(row64(ST_RESULT, 32, self.t as u64));
                self.c.pc = L_ROT;
            }
            L_ROT => {
                *s = [s[1], s[2], s[3], self.t];
                for q in s.iter() {
                    tr.push(row64(ST_RESULT, 32, *q as u64));
                }
                self.c.write = -2;
                self.c.pc = L_RET;
            }
            L_RET => {
                self.out = s[3].wrapping_add(s[2]);
                self.c.reads = [3, 2, -1];
                tr.push(row64(ST_VALUE, 32, s[3] as u64));
                tr.push(row64(ST_VALUE, 32, s[2] as u64));
                tr.push(row64(ST_RESULT, 32, self.out as u64));
                let o = self.out as u64;
                self.c.emit(o, o, false);
                self.c.pc = L_T;
            }
            _ => self.c.pc = L_T,
        }
    }
    pub fn info(&self, out: &mut Vec<u32>) {
        out.extend_from_slice(&self.s);
        out.extend_from_slice(&[self.t, self.out]);
    }
}

// ============================================================================
//  dispatch + run modes
// ============================================================================

pub enum Vm {
    Mt(MtVm),
    Tiny(TinyVm),
    Sfmt(SfmtVm),
    Unt(UntVm),
    Tiny64(Tiny64Vm),
    Melg(MelgVm),
    Xsadd(XsaddVm),
}

pub const RUN_STEP: u32 = 0;
pub const RUN_OVER: u32 = 1;
pub const RUN_OUT: u32 = 2;
pub const RUN_OUTPUT: u32 = 3;
pub const RUN_CONTINUE: u32 = 4;
pub const RUN_LOOP: u32 = 5;
pub const RUN_N: u32 = 6;
pub const RUN_DEEP_RETURN: u32 = 7;
pub const RUN_OUTPUTS: u32 = 8; // run until `arg` outputs emitted

impl Vm {
    pub fn common(&self) -> &Common {
        match self {
            Vm::Mt(v) => &v.c,
            Vm::Tiny(v) => &v.c,
            Vm::Sfmt(v) => &v.c,
            Vm::Unt(v) => &v.c,
            Vm::Tiny64(v) => &v.c,
            Vm::Melg(v) => &v.c,
            Vm::Xsadd(v) => &v.c,
        }
    }
    pub fn common_mut(&mut self) -> &mut Common {
        match self {
            Vm::Mt(v) => &mut v.c,
            Vm::Tiny(v) => &mut v.c,
            Vm::Sfmt(v) => &mut v.c,
            Vm::Unt(v) => &mut v.c,
            Vm::Tiny64(v) => &mut v.c,
            Vm::Melg(v) => &mut v.c,
            Vm::Xsadd(v) => &mut v.c,
        }
    }
    pub fn step(&mut self) {
        match self {
            Vm::Mt(v) => v.step(),
            Vm::Tiny(v) => v.step(),
            Vm::Sfmt(v) => v.step(),
            Vm::Unt(v) => v.step(),
            Vm::Tiny64(v) => v.step(),
            Vm::Melg(v) => v.step(),
            Vm::Xsadd(v) => v.step(),
        }
    }
    fn loop_range(&self, pc: u32) -> Option<(u32, u32)> {
        match self {
            Vm::Mt(v) => v.loop_range(pc),
            Vm::Sfmt(v) => v.loop_range(pc),
            Vm::Unt(v) => v.loop_range(pc),
            Vm::Melg(v) => v.loop_range(pc),
            Vm::Tiny(_) | Vm::Tiny64(_) | Vm::Xsadd(_) => None,
        }
    }
    pub fn info(&self, out: &mut Vec<u32>) {
        match self {
            Vm::Mt(v) => v.info(out),
            Vm::Tiny(v) => v.info(out),
            Vm::Sfmt(v) => v.info(out),
            Vm::Unt(v) => v.info(out),
            Vm::Tiny64(v) => v.info(out),
            Vm::Melg(v) => v.info(out),
            Vm::Xsadd(v) => v.info(out),
        }
    }

    /// returns the number of steps executed; stops early at a breakpoint
    /// (bit `line` of `bp`) for every mode except RUN_STEP / RUN_N
    pub fn run(&mut self, mode: u32, arg: u64, bp: u32, max: u64) -> u64 {
        let mut n = 0u64;
        let d0 = self.common().depth;
        let pc0 = self.common().pc;
        let mut acc_event = 0u32;
        let mut outs = 0u64;
        let bp_hit = |pc: u32| pc < 32 && (bp >> pc) & 1 == 1;
        loop {
            if n >= max {
                break;
            }
            if let Vm::Unt(u) = self {
                if u.done && n > 0 {
                    break;
                }
            }
            self.step();
            n += 1;
            let c = self.common();
            acc_event |= c.event;
            if c.event & EV_OUTPUT != 0 {
                outs += 1;
            }
            let pc = c.pc;
            let stop = match mode {
                RUN_STEP => true,
                RUN_N => n >= arg,
                RUN_OVER => c.depth <= d0,
                RUN_OUT => {
                    if d0 >= 2 {
                        c.depth < d0
                    } else {
                        c.event & (EV_OUTPUT | EV_RET_SEED | EV_DONE) != 0
                    }
                }
                RUN_OUTPUT => c.event & (EV_OUTPUT | EV_DONE) != 0,
                RUN_OUTPUTS => outs >= arg.max(1),
                RUN_CONTINUE => false,
                RUN_DEEP_RETURN => c.event & EV_RET_DEEP != 0,
                RUN_LOOP => match self.loop_range(pc0) {
                    Some((lo, hi)) => !(lo..=hi).contains(&pc),
                    None => true,
                },
                _ => true,
            };
            if stop {
                break;
            }
            let c = self.common();
            if mode != RUN_N && bp_hit(c.pc) {
                break;
            }
            if c.event & EV_DONE != 0 {
                break;
            }
        }
        // keep the union of events visible to JS
        self.common_mut().event |= acc_event & (EV_OUTPUT | EV_RET_SEED | EV_RET_DEEP);
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::{V_PHP_BROKEN, V_PYTHON, variant_config};

    fn vm_outputs(vm: &mut Vm, k: usize) -> Vec<u64> {
        let mut v = Vec::new();
        while v.len() < k {
            vm.run(RUN_OUTPUT, 0, 0, 10_000_000);
            v.push(vm.common().outputs.last().unwrap().value);
        }
        v
    }

    #[test]
    fn mt_vm_matches_generator() {
        for cfg in [MtConfig::mt19937(), MtConfig::mt19937_64(), MtConfig::tt800(), variant_config(V_PYTHON), variant_config(V_PHP_BROKEN)] {
            let mut vm = Vm::Mt(MtVm::new(cfg, 42));
            let got = vm_outputs(&mut vm, 1300);
            let mut g = Mt::new(cfg, 42);
            let want: Vec<u64> = (0..1300).map(|_| g.next()).collect();
            assert_eq!(got, want);
        }
    }

    #[test]
    fn tiny_and_sfmt_vm() {
        let mut vm = Vm::Tiny(TinyVm::new(1, P::TINYMT32_MAT1, P::TINYMT32_MAT2, P::TINYMT32_TMAT));
        assert_eq!(vm_outputs(&mut vm, 3), [2_545_341_989, 981_918_433, 3_715_302_833]);
        let mut vm = Vm::Sfmt(SfmtVm::new(1234));
        assert_eq!(vm_outputs(&mut vm, 3), [3_440_181_298, 1_564_997_079, 1_510_669_302]);
    }

    #[test]
    fn new_family_vms_match_generators() {
        use crate::kinds::{self, Kind};
        for (id, mk) in [
            (kinds::K_TINYMT64, 0u32),
            (kinds::K_WRK2, 1),
            (kinds::K_XSADD, 2),
            (kinds::K_MELG0, 3),
            (kinds::K_MELG0 + 5, 3),
            (kinds::K_SFMT0, 4),
            (kinds::K_SFMT0 + 9, 4),
        ] {
            let seed = 4242u64;
            let mut vm = match mk {
                0 => Vm::Tiny64(Tiny64Vm::new(seed, kinds::T64_MAT1, kinds::T64_MAT2, kinds::T64_TMAT)),
                1 => Vm::Tiny64(Tiny64Vm::new(seed, 0, 0, 0)),
                2 => Vm::Xsadd(XsaddVm::new(seed as u32)),
                3 => Vm::Melg(MelgVm::new(&kinds::MELG[(id - kinds::K_MELG0) as usize], seed)),
                _ => Vm::Sfmt(SfmtVm::with(&sfmt::PARAMS[(id - kinds::K_SFMT0) as usize], seed as u32)),
            };
            let n = if mk == 4 { 2 * 4 * sfmt::PARAMS[(id - kinds::K_SFMT0) as usize].n + 5 } else { 2000 };
            let got = vm_outputs(&mut vm, n);
            let mut k = Kind::new(id, seed).unwrap();
            let want: Vec<u64> = (0..n).map(|_| k.next_native()).collect();
            assert_eq!(got, want, "kind {id}");
        }
    }

    #[test]
    fn untemper_vm() {
        for cfg in [MtConfig::mt19937(), MtConfig::mt19937_64(), MtConfig::tt800(), MtConfig::mt11213b()] {
            let y = 0x0123_4567_89ab_cdefu64 & cfg.wm();
            let z = cfg.temper(y);
            let mut vm = Vm::Unt(UntVm::new(cfg, z));
            vm.run(RUN_CONTINUE, 0, 0, 1000);
            assert_eq!(vm.common().outputs[0].value, y);
        }
    }

    #[test]
    fn step_modes() {
        let mut vm = Vm::Mt(MtVm::new(MtConfig::mt19937(), 5489));
        // finish seed loop
        vm.run(RUN_STEP, 0, 0, 10);
        assert_eq!(vm.common().pc, mtl::SEED_FOR);
        let n = vm.run(RUN_LOOP, 0, 0, 100_000);
        assert_eq!(vm.common().pc, mtl::SEED_IDX);
        assert_eq!(n, 623 * 2 + 1);
        vm.run(RUN_STEP, 0, 0, 1);
        // step over the twist call
        vm.run(RUN_OVER, 0, 0, 100_000);
        assert_eq!(vm.common().pc, mtl::NEXT_LOAD);
        vm.run(RUN_OUTPUT, 0, 0, 100);
        assert_eq!(vm.common().outputs[0].value, 3_499_211_612);
        // breakpoint on RET
        vm.run(RUN_CONTINUE, 0, 1 << mtl::RET, 100);
        assert_eq!(vm.common().pc, mtl::RET);
    }
}
