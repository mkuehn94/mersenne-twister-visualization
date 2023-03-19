//! sfmt (saito & matsumoto 2006). scalar 128-bit recursion:
//!   w[i] = a ^ (a << sl2*8) ^ ((b >> sr1) & msk) ^ (c >> sr2*8) ^ (d << sl1)
//! all ten sizes from the reference code (mexp 607 through 216091).
//! transcribed from the prngs repo, matched against its C output.

use alloc::vec;
use alloc::vec::Vec;

pub use crate::params::SfmtParams;
/// all ten parameter sets (params.rs), in order of MEXP
pub use crate::params::SFMT as PARAMS;
use crate::params::{INIT_GENRAND_F32, SFMT19937_INDEX};

pub type W128 = [u32; 4];

// these two are hideous. i tried a cleaner approach with [u64; 2] but then the
// byte ordering got even more confusing when matching against the C reference.
// giving up on elegance here.
pub fn lshift128(v: &W128, shift: u32) -> W128 {
    let th = ((v[3] as u64) << 32) | v[2] as u64;
    let tl = ((v[1] as u64) << 32) | v[0] as u64;
    let mut oh = th << (shift * 8);
    let ol = tl << (shift * 8);
    oh |= tl >> (64 - shift * 8);
    [ol as u32, (ol >> 32) as u32, oh as u32, (oh >> 32) as u32]
}

pub fn rshift128(v: &W128, shift: u32) -> W128 {
    let th = ((v[3] as u64) << 32) | v[2] as u64;
    let tl = ((v[1] as u64) << 32) | v[0] as u64;
    let mut ol = tl >> (shift * 8);
    let oh = th >> (shift * 8);
    ol |= th << (64 - shift * 8);
    [ol as u32, (ol >> 32) as u32, oh as u32, (oh >> 32) as u32]
}

/// the five terms of the recursion, for display: (a≪, b-term, c≫, d-term, result)
pub fn recursion_terms(p: &SfmtParams, a: &W128, b: &W128, c: &W128, d: &W128) -> [W128; 5] {
    let x = lshift128(a, p.sl2);
    let y = rshift128(c, p.sr2);
    let mut bt = [0u32; 4];
    let mut dt = [0u32; 4];
    let mut r = [0u32; 4];
    for j in 0..4 {
        bt[j] = (b[j] >> p.sr1) & p.msk[j];
        dt[j] = d[j] << p.sl1;
        r[j] = a[j] ^ x[j] ^ bt[j] ^ y[j] ^ dt[j];
    }
    [x, bt, y, dt, r]
}

#[derive(Clone)]
pub struct Sfmt {
    pub p: &'static SfmtParams,
    pub st: Vec<u32>, // 4·N words
    pub idx: usize,
}

impl Sfmt {
    pub fn new(seed: u32) -> Sfmt {
        Sfmt::with(sfmt19937(), seed)
    }
    pub fn with(p: &'static SfmtParams, seed: u32) -> Sfmt {
        let mut s = Sfmt::blank(p);
        s.seed(seed);
        s
    }
    pub fn blank(p: &'static SfmtParams) -> Sfmt {
        Sfmt { p, st: vec![0; 4 * p.n], idx: 4 * p.n }
    }
    pub fn n32(&self) -> usize {
        4 * self.p.n
    }
    pub fn seed_fill(&mut self, seed: u32) {
        self.st[0] = seed;
        for i in 1..self.n32() {
            let p = self.st[i - 1];
            self.st[i] = (INIT_GENRAND_F32 as u32).wrapping_mul(p ^ (p >> 30)).wrapping_add(i as u32);
        }
        self.idx = self.n32();
    }
    /// returns true if a bit had to be flipped (schould only happen for seed 0 in practice)
    pub fn period_certification(&mut self) -> bool {
        let mut inner = 0u32;
        for i in 0..4 {
            inner ^= self.st[i] & self.p.parity[i];
        }
        let mut i = 16;
        while i > 0 {
            inner ^= inner >> i;
            i >>= 1;
        }
        if inner & 1 == 1 {
            return false;
        }
        for i in 0..4 {
            let mut work = 1u32;
            for _ in 0..32 {
                if work & self.p.parity[i] != 0 {
                    self.st[i] ^= work;
                    return true;
                }
                work <<= 1;
            }
        }
        false
    }
    pub fn seed(&mut self, seed: u32) {
        self.seed_fill(seed);
        self.period_certification();
    }
    pub fn block(&self, i: usize) -> W128 {
        [self.st[4 * i], self.st[4 * i + 1], self.st[4 * i + 2], self.st[4 * i + 3]]
    }
    pub fn set_block(&mut self, i: usize, v: &W128) {
        self.st[4 * i..4 * i + 4].copy_from_slice(v);
    }
    pub fn gen_all(&mut self) {
        let p = self.p;
        let n = p.n;
        let mut r1 = self.block(n - 2);
        let mut r2 = self.block(n - 1);
        for i in 0..n {
            let a = self.block(i);
            let b = self.block((i + p.pos1) % n);
            let t = recursion_terms(p, &a, &b, &r1, &r2);
            self.set_block(i, &t[4]);
            r1 = r2;
            r2 = t[4];
        }
        self.idx = 0;
    }
    pub fn next(&mut self) -> u32 {
        if self.idx >= self.st.len() {
            self.gen_all();
        }
        let v = self.st[self.idx];
        self.idx += 1;
        v
    }
}

pub fn params(mexp: u32) -> &'static SfmtParams {
    PARAMS.iter().find(|p| p.mexp == mexp).unwrap_or(&PARAMS[SFMT19937_INDEX])
}
pub fn sfmt19937() -> &'static SfmtParams {
    &PARAMS[SFMT19937_INDEX]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference() {
        // SFMT.19937.out.txt, init_gen_rand(1234)
        let mut s = Sfmt::new(1234);
        let v: Vec<u32> = (0..3).map(|_| s.next()).collect();
        assert_eq!(v, [3_440_181_298, 1_564_997_079, 1_510_669_302]);
    }
    #[test]
    fn all_sizes_reference() {
        // SFMT.<MEXP>.out.txt, init_gen_rand(1234): first five outputs (from kinds/sfmt*.c selftests)
        let want: [(u32, [u32; 5]); 9] = [
            (607, [1196421539, 2865311212, 3866479472, 2692900087, 3838928621]),
            (1279, [243307689, 3927268025, 1225611617, 570598983, 3842545525]),
            (2281, [816899028, 2529810904, 2984700728, 4161010272, 3805350266]),
            (4253, [2527479900, 1368357778, 2663671614, 1404435254, 2699472814]),
            (11213, [553293926, 698755237, 2442073441, 4209880924, 1764362329]),
            (44497, [3668471065, 3938124162, 4226228648, 1183164762, 959305109]),
            (86243, [729010956, 4245516629, 2851064434, 363057815, 4150273260]),
            (132049, [3596981943, 2237974425, 3827224957, 2514757895, 4264843680]),
            (216091, [1905350899, 752275649, 2172726721, 1382267163, 3279518050]),
        ];
        for (m, w) in want.iter() {
            let mut s = Sfmt::with(params(*m), 1234);
            let v: Vec<u32> = (0..5).map(|_| s.next()).collect();
            assert_eq!(&v[..], &w[..], "SFMT{m}");
        }
    }
}
