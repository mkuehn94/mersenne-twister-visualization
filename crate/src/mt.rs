//! generic MT engine. one MtConfig covers every variant.
//!
//! twist: x[k+n] = x[k+m] ^ ((x[k]^u | x[k+1]^l) * a)
//! output: z = temper(x[i])

use crate::params::{self, MtParams};
use alloc::vec;
use alloc::vec::Vec;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SeedMode {
    InitGenrand = 0,
    Knuth69069 = 1,
    Cokus = 2,
    InitByArray = 3,
    RLcg = 4,
    SeedSeq = 5,
    GnuGo = 6,
    CokusShift = 7,
}

impl SeedMode {
    pub fn bulk(self) -> bool {
        matches!(self, SeedMode::InitByArray | SeedMode::RLcg | SeedMode::SeedSeq | SeedMode::GnuGo | SeedMode::CokusShift)
    }
}

/// std::seed_seq{seed}.generate(out), libstdc++ impl.
pub fn seed_seq_generate(seed: u32, out: &mut [u32]) {
    let n = out.len();
    for v in out.iter_mut() {
        *v = params::SEED_SEQ_FILL;
    }
    let s = 1usize; // s is 1 here: only one seed word ever goes through
    let t = if n >= 623 { 11 } else if n >= 68 { 7 } else if n >= 39 { 5 } else if n >= 7 { 3 } else { (n - 1) / 2 };
    let p = (n - t) / 2;
    let q = p + t;
    let m = (s + 1).max(n);
    {
        let r1 = params::SEED_SEQ_R1;
        let r2 = r1.wrapping_add(s as u32);
        out[p] = out[p].wrapping_add(r1);
        out[q] = out[q].wrapping_add(r2);
        out[0] = r2;
    }
    for k in 1..m {
        let (kn, kpn, kqn) = (k % n, (k + p) % n, (k + q) % n);
        let arg = out[kn] ^ out[kpn] ^ out[(k - 1) % n];
        let r1 = params::SEED_SEQ_MUL1.wrapping_mul(arg ^ (arg >> 27));
        let mut r2 = r1.wrapping_add(kn as u32);
        if k <= s {
            r2 = r2.wrapping_add(seed);
        }
        out[kpn] = out[kpn].wrapping_add(r1);
        out[kqn] = out[kqn].wrapping_add(r2);
        out[kn] = r2;
    }
    for k in m..m + n {
        let (kn, kpn, kqn) = (k % n, (k + p) % n, (k + q) % n);
        let arg = out[kn].wrapping_add(out[kpn]).wrapping_add(out[(k - 1) % n]);
        let r3 = params::SEED_SEQ_MUL2.wrapping_mul(arg ^ (arg >> 27));
        let r4 = r3.wrapping_sub(kn as u32);
        out[kpn] ^= r3;
        out[kqn] ^= r4;
        out[kn] = r4;
    }
}

impl SeedMode {
    pub fn from_u32(v: u32) -> SeedMode {
        match v {
            1 => SeedMode::Knuth69069,
            2 => SeedMode::Cokus,
            3 => SeedMode::InitByArray,
            4 => SeedMode::RLcg,
            5 => SeedMode::SeedSeq,
            6 => SeedMode::GnuGo,
            7 => SeedMode::CokusShift,
            _ => SeedMode::InitGenrand,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MtConfig {
    pub w: u32,
    pub n: usize,
    pub m: usize,
    pub r: u32,
    pub a: u64,
    pub u: u32,
    pub d: u64,
    pub s: u32,
    pub b: u64,
    pub t: u32,
    pub c: u64,
    pub l: u32,
    pub f: u64,
    /// upper/lower masks of the twist; normally umask = ¬lmask, lmask = 2ʳ−1
    pub umask: u64,
    pub lmask: u64,
    /// PHP 5.2.1–7.0 bug: test the low bit of x[k] instead of y
    pub lowbit_from_xk: bool,
    pub temper: bool,
    pub seed_mode: SeedMode,
    /// value substituted for seed 0 (0 = none)
    pub zero_sub: u64,
    pub seed_xor: u64,
    pub out_shift: u32,
    pub out_xor: u64,
    pub out_and: u64,
    pub discard: u32,
}

#[inline]
pub fn wmask(w: u32) -> u64 {
    if w >= 64 { u64::MAX } else { (1u64 << w) - 1 }
}
#[inline]
pub fn shr(x: u64, k: u32) -> u64 {
    // shifting by >= w should give 0
    if k >= 64 { 0 } else { x >> k }
}
#[inline]
pub fn shl(x: u64, k: u32, w: u32) -> u64 {
    if k >= 64 { 0 } else { (x << k) & wmask(w) }
}

impl MtConfig {
    /// a config from the 13 defining numbers (params.rs), with default masks
    pub fn from_params(p: &MtParams) -> MtConfig {
        MtConfig::new(p.w, p.n, p.m, p.r, p.a, p.u, p.d, p.s, p.b, p.t, p.c, p.l, p.f)
    }
    pub fn mt19937() -> MtConfig { MtConfig::from_params(&params::MT19937) }
    pub fn mt19937_64() -> MtConfig { MtConfig::from_params(&params::MT19937_64) }
    pub fn mt11213b() -> MtConfig { MtConfig::from_params(&params::MT11213B) }
    pub fn tt800() -> MtConfig { MtConfig::from_params(&params::TT800) }

    #[allow(clippy::too_many_arguments)]
    pub fn new(w: u32, n: usize, m: usize, r: u32, a: u64, u: u32, d: u64, s: u32, b: u64, t: u32, c: u64, l: u32, f: u64) -> MtConfig {
        let wm = wmask(w);
        let lmask = if r == 0 { 0 } else { wmask(r) };
        MtConfig {
            w,
            n,
            m,
            r,
            a: a & wm,
            u,
            d: d & wm,
            s,
            b: b & wm,
            t,
            c: c & wm,
            l,
            f: f & wm,
            umask: wm & !lmask,
            lmask,
            lowbit_from_xk: false,
            temper: true,
            seed_mode: SeedMode::InitGenrand,
            zero_sub: 0,
            seed_xor: 0,
            out_shift: 0,
            out_xor: 0,
            out_and: wm,
            discard: 0,
        }
    }

    pub fn p(&self) -> usize {
        self.n * self.w as usize - self.r as usize
    }
    pub fn wm(&self) -> u64 { wmask(self.w) }

    pub fn set_r(&mut self, r: u32) {
        self.r = r;
        self.lmask = if r == 0 { 0 } else { wmask(r) };
        self.umask = self.wm() & !self.lmask;
    }

    pub fn temper(&self, y: u64) -> u64 {
        if !self.temper {
            return y;
        }
        // NOTE: don't reorder these — the shifts don't commute
        let w = self.w;
        let mut y = y;
        y ^= shr(y, self.u) & self.d;
        y ^= shl(y, self.s, w) & self.b;
        y ^= shl(y, self.t, w) & self.c;
        y ^= shr(y, self.l);
        y & self.wm()
    }

    /// exact inverse of `temper` (each step is an invertible GF(2)-linear map)
    pub fn untemper(&self, z: u64) -> u64 {
        if !self.temper {
            return z;
        }
        let w = self.w;
        let mut y = z & self.wm();
        y = inv_shr_xor(y, self.l, u64::MAX, w);
        y = inv_shl_xor(y, self.t, self.c, w);
        y = inv_shl_xor(y, self.s, self.b, w);
        y = inv_shr_xor(y, self.u, self.d, w);
        y
    }

    /// the twist of a single word: x[k+n] from x[k], x[k+1], x[k+m]
    #[inline]
    pub fn twist_word(&self, xk: u64, xk1: u64, xkm: u64) -> u64 {
        let y = (xk & self.umask).wrapping_add(xk1 & self.lmask) & self.wm();
        let low = if self.lowbit_from_xk { xk & 1 } else { y & 1 };
        let ya = (y >> 1) ^ if low == 1 { self.a } else { 0 };
        (xkm ^ ya) & self.wm()
    }

    pub fn out_transform(&self, z: u64) -> u64 {
        ((z >> self.out_shift) ^ self.out_xor) & self.out_and
    }
}

/// undo y = x ^ ((x >> k) & m). iterate from the top because each bit
/// depends on the one k above it.
pub fn inv_shr_xor(y: u64, k: u32, m: u64, w: u32) -> u64 {
    if k >= w || k == 0 {
        return y;
    }
    let mut x = y;
    let reps = (w + k - 1) / k - 1;
    for _ in 0..reps {
        x = y ^ (shr(x, k) & m);
    }
    x & wmask(w)
}

/// undo y = x ^ ((x << k) & m). same idea, iterate from the bottom.
pub fn inv_shl_xor(y: u64, k: u32, m: u64, w: u32) -> u64 {
    if k >= w || k == 0 {
        return y;
    }
    let mut x = y;
    let reps = (w + k - 1) / k - 1;
    for _ in 0..reps {
        x = y ^ (shl(x, k, w) & m);
    }
    x & wmask(w)
}

#[derive(Clone)]
pub struct Mt {
    pub cfg: MtConfig,
    pub x: Vec<u64>,
    pub idx: usize,
    pub count: u64,
}

impl Mt {
    pub fn new(cfg: MtConfig, seed: u64) -> Mt {
        let n = cfg.n;
        let mut m = Mt { cfg, x: vec![0; n], idx: n, count: 0 };
        m.seed(seed);
        m
    }

    pub fn effective_seed(&self, seed: u64) -> u64 {
        let mut s = seed ^ self.cfg.seed_xor;
        if s & self.cfg.wm() == 0 && self.cfg.zero_sub != 0 {
            s = self.cfg.zero_sub;
        }
        s
    }

    pub fn seed(&mut self, seed: u64) {
        let s = self.effective_seed(seed);
        // self.count = 0;   // nope, has to happen after the discard loop
        self.seed_raw(s);
        // discard some outputs if configured (e.g. ECJ does this)
        for _ in 0..self.cfg.discard {
            self.next();
        }
        self.count = 0;
    }

    /// seeding without discard
    pub fn seed_raw(&mut self, s: u64) {
        let c = &self.cfg;
        let n = c.n;
        let wm = c.wm();
        match c.seed_mode {
            SeedMode::InitGenrand => self.init_genrand_const(s),
            SeedMode::Knuth69069 => {
                self.x[0] = s & wm;
                for i in 1..n {
                    self.x[i] = self.x[i - 1].wrapping_mul(params::KNUTH_MUL) & wm;
                }
            }
            SeedMode::Cokus => {
                // |1 so seed 0 doesn't give us an all-zero state (which is a fixed point)
                let mut x = (s | 1) & wm;
                self.x[0] = x;
                for i in 1..n {
                    x = x.wrapping_mul(params::KNUTH_MUL) & wm;
                    self.x[i] = x;
                }
            }
            SeedMode::RLcg => {
                let mut x = s;
                for _ in 0..params::R_LCG_WARMUP {
                    x = x.wrapping_mul(params::R_LCG_MUL).wrapping_add(params::R_LCG_ADD) & wm;
                }
                for i in 0..n {
                    x = x.wrapping_mul(params::R_LCG_MUL).wrapping_add(params::R_LCG_ADD) & wm;
                    self.x[i] = x;
                }
            }
            SeedMode::SeedSeq => {
                let mut buf = vec![0u32; n];
                seed_seq_generate(s as u32, &mut buf);
                for i in 0..n {
                    self.x[i] = buf[i] as u64 & wm;
                }
                // libstdc++ does this weird thing where if the state ended up all zeros
                // it sets x[0] = 1 << (w-1).  not sure why, just matching the C++ behavior.
                if self.x[0] & c.umask == 0 && self.x[1..].iter().all(|&v| v == 0) {
                    self.x[0] = 1u64 << (c.w - 1);
                }
            }
            SeedMode::GnuGo => {
                let mut x = s & wm;
                for i in 0..n {
                    self.x[i] = x;
                    x = x.wrapping_mul(params::GNUGO_LCG_MUL).wrapping_add(params::GNUGO_LCG_ADD) & wm;
                }
            }
            SeedMode::CokusShift => {
                let mut x = s & wm;
                for i in 0..n {
                    x = (x | 1).wrapping_mul(params::KNUTH_MUL) & wm;
                    self.x[i] = x;
                }
            }
            SeedMode::InitByArray => {
                if c.w == 64 {
                    self.init_by_array64(&[s]);
                } else {
                    let key = seed_key32(s);
                    self.init_by_array32(&key);
                }
            }
        }
        self.idx = n;
    }

    fn init_genrand_const(&mut self, s: u64) {
        let c = &self.cfg;
        let wm = c.wm();
        self.x[0] = s & wm;
        for i in 1..c.n {
            let p = self.x[i - 1];
            self.x[i] = c.f.wrapping_mul(p ^ shr(p, c.w.saturating_sub(2))).wrapping_add(i as u64) & wm;
        }
    }

    /// reference mt19937ar.c init_by_array (w = 32 semantics)
    pub fn init_by_array32(&mut self, key: &[u64]) {
        let n = self.cfg.n;
        let wm = self.cfg.wm();
        let sh = self.cfg.w.saturating_sub(2);
        self.init_genrand_const(params::INIT_BY_ARRAY_SEED);
        let (mut i, mut j) = (1usize, 0usize);
        let klen = key.len().max(1);
        let mut k = n.max(klen);
        while k > 0 {
            let p = self.x[i - 1];
            self.x[i] = ((self.x[i] ^ (p ^ shr(p, sh)).wrapping_mul(params::INIT_BY_ARRAY32_MUL1))
                .wrapping_add(key.get(j).copied().unwrap_or(0))
                .wrapping_add(j as u64))
                & wm;
            i += 1;
            j += 1;
            if i >= n {
                self.x[0] = self.x[n - 1];
                i = 1;
            }
            if j >= klen {
                j = 0;
            }
            k -= 1;
        }
        k = n - 1;
        while k > 0 {
            let p = self.x[i - 1];
            self.x[i] = ((self.x[i] ^ (p ^ shr(p, sh)).wrapping_mul(params::INIT_BY_ARRAY32_MUL2)).wrapping_sub(i as u64)) & wm;
            i += 1;
            if i >= n {
                self.x[0] = self.x[n - 1];
                i = 1;
            }
            k -= 1;
        }
        self.x[0] = 1u64 << (self.cfg.w - 1);
    }

    /// reference mt19937-64.c init_by_array64
    pub fn init_by_array64(&mut self, key: &[u64]) {
        let n = self.cfg.n;
        self.init_genrand_const(params::INIT_BY_ARRAY_SEED);
        let (mut i, mut j) = (1usize, 0usize);
        let klen = key.len().max(1);
        let mut k = n.max(klen);
        while k > 0 {
            let p = self.x[i - 1];
            self.x[i] = (self.x[i] ^ (p ^ (p >> 62)).wrapping_mul(params::INIT_BY_ARRAY64_MUL1))
                .wrapping_add(key[j])
                .wrapping_add(j as u64);
            i += 1;
            j += 1;
            if i >= n {
                self.x[0] = self.x[n - 1];
                i = 1;
            }
            if j >= klen {
                j = 0;
            }
            k -= 1;
        }
        k = n - 1;
        while k > 0 {
            let p = self.x[i - 1];
            self.x[i] = (self.x[i] ^ (p ^ (p >> 62)).wrapping_mul(params::INIT_BY_ARRAY64_MUL2)).wrapping_sub(i as u64);
            i += 1;
            if i >= n {
                self.x[0] = self.x[n - 1];
                i = 1;
            }
            k -= 1;
        }
        self.x[0] = 1u64 << 63;
    }

    pub fn twist(&mut self) {
        let n = self.cfg.n;
        let m = self.cfg.m;
        for k in 0..n {
            self.x[k] = self.cfg.twist_word(self.x[k], self.x[(k + 1) % n], self.x[(k + m) % n]);
        }
        self.idx = 0;
    }

    /// tempered word, before the output transform
    #[inline]
    pub fn next_tempered(&mut self) -> u64 {
        if self.idx >= self.cfg.n {
            self.twist();
        }
        let y = self.x[self.idx];
        self.idx += 1;
        self.count += 1;
        self.cfg.temper(y)
    }

    #[inline]
    pub fn next(&mut self) -> u64 {
        let z = self.next_tempered();
        self.cfg.out_transform(z)
    }

    /// the top 32 bits (the whole word for w = 32)
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let v = self.next();
        let w = self.cfg.w.saturating_sub(self.cfg.out_shift);
        if w > 32 {
            (v >> (w - 32)) as u32
        } else {
            (v << (32 - w)) as u32
        }
    }
}

// ---------------------------------------------------------------------------
//  the state as a vector in GF(2)^p
//
//  bits 0 .. w-r-1        : the upper w-r bits of x[0]   (x[0]ᵘ)
//  then w bits each       : x[1], x[2], …, x[n-1]
// ---------------------------------------------------------------------------

/// put a single basis vector e_beta into the state of a fresh generator,
/// positioned so the next thing that happens is a twist.
pub fn basis_state(cfg: &MtConfig, beta: usize) -> Mt {
    let mut g = Mt { cfg: cfg.clone(), x: vec![0; cfg.n], idx: cfg.n, count: 0 };
    let up = (cfg.w - cfg.r) as usize;
    if beta < up {
        g.x[0] = 1u64 << (cfg.r as usize + beta);
    } else {
        let q = beta - up;
        g.x[1 + q / cfg.w as usize] = 1u64 << (q % cfg.w as usize);
    }
    g
}

/// an infinite stream of raw state words (no tempering). starts from
/// x[0..n] and keeps producing x[n], x[n+1], ... via the recurrence.
pub struct RawSeq {
    pub cfg: MtConfig,
    pub buf: Vec<u64>,
    pub pos: usize,
}

impl RawSeq {
    pub fn from_state(cfg: &MtConfig, x: &[u64]) -> RawSeq {
        RawSeq { cfg: cfg.clone(), buf: x.to_vec(), pos: 0 }
    }
    /// produce the next word and slide the window forward by one.
    pub fn step(&mut self) -> u64 {
        let n = self.cfg.n;
        let i = self.pos;
        let v = self.cfg.twist_word(self.buf[i % n], self.buf[(i + 1) % n], self.buf[(i + self.cfg.m) % n]);
        self.buf[i % n] = v;
        self.pos += 1;
        v
    }
}

/// split a u64 seed into 32-bit limbs the way CPython does for
/// random.seed(int) — little-endian, so the low word comes first.
pub fn seed_key32(seed: u64) -> Vec<u64> {
    let lo = seed & 0xffff_ffff;
    let hi = seed >> 32;
    if hi != 0 { vec![lo, hi] } else { vec![lo] }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first(cfg: MtConfig, seed: u64, k: usize) -> Vec<u64> {
        let mut g = Mt::new(cfg, seed);
        (0..k).map(|_| g.next()).collect()
    }

    #[test]
    fn reference_vectors() {
        // std::mt19937 default seed 5489: first, and the 10000th (C++ standard requirement)
        let mut g = Mt::new(MtConfig::mt19937(), 5489);
        assert_eq!(g.next(), 3_499_211_612);
        for _ in 1..9999 {
            g.next();
        }
        assert_eq!(g.next(), 4_123_659_995);
        let mut g = Mt::new(MtConfig::mt19937_64(), 5489);
        for _ in 0..9999 {
            g.next();
        }
        assert_eq!(g.next(), 9_981_545_732_273_789_042);
        // mt19937ar.c init_by_array({0x123,0x234,0x345,0x456})
        let mut c = MtConfig::mt19937();
        c.seed_mode = SeedMode::InitByArray;
        let mut g = Mt::new(c.clone(), 0);
        g.init_by_array32(&[0x123, 0x234, 0x345, 0x456]);
        g.idx = 624;
        let v: Vec<u64> = (0..5).map(|_| g.next()).collect();
        assert_eq!(v, vec![1_067_595_299, 955_945_823, 477_289_528, 4_107_218_783, 4_228_976_476]);
        // CPython random.seed(42); getrandbits(32)
        assert_eq!(first(c.clone(), 42, 3), vec![2_746_317_213, 478_163_327, 107_420_369]);
        assert_eq!(first(c.clone(), (1 << 40) + 5, 2), vec![2_166_296_868, 2_220_160_828]);
        assert_eq!(first(c, 0, 2), vec![3_626_764_237, 1_654_615_998]);
    }

    #[test]
    fn untemper_roundtrip() {
        for cfg in [MtConfig::mt19937(), MtConfig::mt19937_64(), MtConfig::mt11213b(), MtConfig::tt800()] {
            let mut s = 0x1234_5678_9abc_def0u64;
            for _ in 0..1000 {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let y = s & cfg.wm();
                assert_eq!(cfg.untemper(cfg.temper(y)), y);
            }
        }
    }
}
