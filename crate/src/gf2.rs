//! gf2 linear algebra — Berlekamp-Massey, irreducibility, k-dist.
//!
//! for Mersenne exponents: 2^p-1 prime, so irreducible == primitive.

use crate::mt::{Mt, MtConfig, basis_state};
use alloc::vec;
use alloc::vec::Vec;

#[inline]
pub fn words(bits: usize) -> usize {
    (bits + 63) / 64
}
#[inline]
pub fn get_bit(v: &[u64], i: usize) -> bool {
    (v[i >> 6] >> (i & 63)) & 1 == 1
}
#[inline]
pub fn set_bit(v: &mut [u64], i: usize) {
    v[i >> 6] |= 1u64 << (i & 63);
}
#[inline]
pub fn flip_bit(v: &mut [u64], i: usize) {
    v[i >> 6] ^= 1u64 << (i & 63);
}

/// 64 bits of `v` starting at bit `pos`
#[inline]
pub fn get64(v: &[u64], pos: usize) -> u64 {
    let wi = pos >> 6;
    let sh = pos & 63;
    let lo = if wi < v.len() { v[wi] } else { 0 };
    if sh == 0 {
        lo
    } else {
        let hi = if wi + 1 < v.len() { v[wi + 1] } else { 0 };
        (lo >> sh) | (hi << (64 - sh))
    }
}

// dst ^= src << shift in GF(2)[x]
pub fn xor_shifted(dst: &mut [u64], src: &[u64], shift: usize) {
    let ws = shift >> 6;
    let bs = shift & 63;
    let dl = dst.len();
    if bs == 0 {
        for (i, &s) in src.iter().enumerate() {
            if i + ws < dl {
                dst[i + ws] ^= s;
            }
        }
    } else {
        let mut carry = 0u64;
        for (i, &s) in src.iter().enumerate() {
            let j = i + ws;
            if j < dl {
                dst[j] ^= (s << bs) | carry;
            }
            carry = s >> (64 - bs);
        }
        let j = src.len() + ws;
        if j < dl {
            dst[j] ^= carry;
        }
    }
}

/// degree of a polynomial. bit i = coeff of x^i. returns None if it's the zero poly.
pub fn degree(p: &[u64]) -> Option<usize> {
    for i in (0..p.len()).rev() {
        if p[i] != 0 {
            return Some(i * 64 + 63 - p[i].leading_zeros() as usize);
        }
    }
    None
}

pub fn weight(p: &[u64]) -> u32 {
    p.iter().map(|w| w.count_ones()).sum()
}

// Berlekamp-Massey over GF(2). Returns (C, L).
// C in reciprocal order, profile records L after each bit.
pub fn berlekamp_massey(seq: &[u64], nbits: usize, mut profile: Option<&mut Vec<u32>>) -> (Vec<u64>, usize) {
    let mut j = BmJob::new(seq, nbits);
    while j.n < nbits {
        j.bit();
        if let Some(p) = profile.as_deref_mut() {
            p.push(j.l as u32);
        }
    }
    let l = j.l;
    let mut c = j.c;
    c.truncate(words(l + 1));
    (c, l)
}

/// Berlekamp–Massey runnable slice-at-a-time
pub struct BmJob {
    rev: Vec<u64>,
    pub c: Vec<u64>,
    b: Vec<u64>,
    t: Vec<u64>,
    pub l: usize,
    m: usize,
    lb: usize,
    pub n: usize,
    pub nbits: usize,
    cap: usize,
}

impl BmJob {
    pub fn new(seq: &[u64], nbits: usize) -> BmJob {
        // reversed sequence: rev[j] = s[N-1-j] so that a dot product with C is a
        // contiguous window
        let nw = words(nbits) + 2;
        let mut rev = vec![0u64; nw];
        for i in 0..nbits {
            if get_bit(seq, i) {
                set_bit(&mut rev, nbits - 1 - i);
            }
        }
        let cap = words(nbits + 2) + 2;
        let mut c = vec![0u64; cap];
        let mut b = vec![0u64; cap];
        c[0] = 1;
        b[0] = 1;
        BmJob { rev, c, b, t: vec![0u64; cap], l: 0, m: 1, lb: 0, n: 0, nbits, cap }
    }
    /// process one bit
    #[inline]
    pub fn bit(&mut self) {
        let n = self.n;
        let cap = self.cap;
        // discrepancy d = Σ_{i=0..L} C_i s_{n-i}
        let off = self.nbits - 1 - n;
        let lw = self.l / 64 + 1;
        let mut acc = 0u64;
        for q in 0..lw {
            acc ^= self.c[q] & get64(&self.rev, off + 64 * q);
        }
        let d = acc.count_ones() & 1;
        if d == 0 {
            self.m += 1;
        } else if 2 * self.l <= n {
            let used = (self.l.max(self.lb + self.m)) / 64 + 2;
            let used = used.min(cap);
            self.t[..used].copy_from_slice(&self.c[..used]);
            xor_shifted(&mut self.c[..cap], &self.b[..(self.lb / 64 + 1).min(cap)], self.m);
            let newl = n + 1 - self.l;
            self.b[..used].copy_from_slice(&self.t[..used]);
            for x in self.b[used..].iter_mut() {
                *x = 0;
            }
            self.lb = self.l;
            self.l = newl;
            self.m = 1;
        } else {
            xor_shifted(&mut self.c[..cap], &self.b[..(self.lb / 64 + 1).min(cap)], self.m);
            self.m += 1;
        }
        self.n += 1;
    }
    /// run k more bits; true when finished
    pub fn step(&mut self, k: usize) -> bool {
        let end = (self.n + k).min(self.nbits);
        while self.n < end {
            self.bit();
        }
        self.n >= self.nbits
    }
}

/// connection polynomial C(x) (reciprocal) → characteristic polynomial x^L·C(1/x)
pub fn reciprocal(c: &[u64], l: usize) -> Vec<u64> {
    let mut r = vec![0u64; words(l + 1)];
    for i in 0..=l {
        if i / 64 < c.len() && get_bit(c, i) {
            set_bit(&mut r, l - i);
        }
    }
    r
}

/// interleave zeros: bit i → bit 2i
#[inline]
fn spread32(x: u32) -> u64 {
    let mut x = x as u64;
    x = (x | (x << 16)) & 0x0000_FFFF_0000_FFFF;
    x = (x | (x << 8)) & 0x00FF_00FF_00FF_00FF;
    x = (x | (x << 4)) & 0x0F0F_0F0F_0F0F_0F0F;
    x = (x | (x << 2)) & 0x3333_3333_3333_3333;
    x = (x | (x << 1)) & 0x5555_5555_5555_5555;
    x
}

/// reduction mod f. dense: 8 bits at a time (256-entry table).
/// sparse with gap G (MT19937: G=623, 135 terms): whole G-bit block cleared
/// by one shifted XOR per term.
pub struct Reducer {
    pub f: Vec<u64>,
    pub p: usize,
    table: Vec<Vec<u64>>, // 256 entries, each a polynomial of degree < p + 8
    terms: Vec<usize>,    // exponents of g = f − x^p (sparse mode)
    gap: usize,
    block: Vec<u64>,
}

impl Reducer {
    pub fn new(f: &[u64]) -> Reducer {
        let p = degree(f).expect("nonzero");
        let mut terms = Vec::new();
        for i in 0..p {
            if get_bit(f, i) {
                terms.push(i);
            }
        }
        let e2 = terms.last().copied().unwrap_or(0);
        let gap = p - e2;
        if gap >= 64 && terms.len() * 64 < gap * 64 * 8 && terms.len() < p / 16 {
            return Reducer { f: f.to_vec(), p, table: Vec::new(), terms, gap, block: vec![0u64; words(gap) + 2] };
        }
        let tw = words(p + 8) + 1;
        let mut table = Vec::with_capacity(256);
        for v in 0..256usize {
            let mut g = vec![0u64; tw];
            let mut reg = vec![0u64; tw];
            for bbit in 0..8 {
                if (v >> bbit) & 1 == 1 {
                    set_bit(&mut reg, p + bbit);
                }
            }
            for bbit in (0..8).rev() {
                if get_bit(&reg, p + bbit) {
                    xor_shifted(&mut g, f, bbit);
                    xor_shifted(&mut reg, f, bbit);
                }
            }
            table.push(g);
        }
        Reducer { f: f.to_vec(), p, table, terms: Vec::new(), gap: 0, block: Vec::new() }
    }

    pub fn is_sparse(&self) -> bool {
        self.table.is_empty()
    }

    fn reduce_sparse(&mut self, r: &mut [u64]) {
        let p = self.p;
        let mut hi = match degree(r) {
            Some(d) if d >= p => d,
            _ => return,
        };
        loop {
            let lo = if hi + 1 >= p + self.gap { hi + 1 - self.gap } else { p };
            let len = hi + 1 - lo;
            let bw = words(len);
            // extract bits [lo, hi] into block, clear them in r
            for q in 0..bw {
                self.block[q] = get64(r, lo + 64 * q);
            }
            let tail = len % 64;
            if tail != 0 {
                self.block[bw - 1] &= (1u64 << tail) - 1;
            }
            for q in bw..self.block.len() {
                self.block[q] = 0;
            }
            for q in 0..bw {
                let b = self.block[q];
                if b == 0 {
                    continue;
                }
                // clear by xor-ing the same bits back
                let pos = lo + 64 * q;
                let wi = pos >> 6;
                let sh = pos & 63;
                r[wi] ^= b << sh;
                if sh != 0 && wi + 1 < r.len() {
                    r[wi + 1] ^= b >> (64 - sh);
                }
            }
            // x^(lo+j) = x^(lo−p) · x^(p+j) ≡ x^(lo−p+j) · g(x)
            for &e in &self.terms {
                xor_shifted(r, &self.block[..bw], lo - p + e);
            }
            if lo == p {
                // the block may have produced bits ≥ p again only if lo−p+e+len > p
                match degree(r) {
                    Some(d) if d >= p => {
                        hi = d;
                        continue;
                    }
                    _ => return,
                }
            }
            hi = lo - 1;
            // skip zero region quickly
            while hi >= p && !get_bit(r, hi) {
                if hi & 63 == 63 && r[hi >> 6] == 0 {
                    if hi < 64 {
                        break;
                    }
                    hi -= 64;
                    continue;
                }
                hi -= 1;
            }
            if hi < p {
                return;
            }
        }
    }

    /// reduce r (deg < 2p); result deg < p
    pub fn reduce(&mut self, r: &mut [u64]) {
        if self.is_sparse() {
            return self.reduce_sparse(r);
        }
        let p = self.p;
        let top = match degree(r) {
            Some(d) => d,
            None => return,
        };
        if top < p {
            return;
        }
        let mut hi = top as isize;
        while hi - 7 >= p as isize {
            let base = (hi - 7) as usize;
            let v = (get64(r, base) & 0xff) as usize;
            if v != 0 {
                xor_shifted(r, &self.table[v], base - p);
            }
            hi -= 8;
        }
        while hi >= p as isize {
            let q = hi as usize;
            if get_bit(r, q) {
                xor_shifted(r, &self.f, q - p);
            }
            hi -= 1;
        }
    }

    /// r ← r² mod f
    pub fn square(&mut self, r: &[u64], out: &mut [u64]) {
        for x in out.iter_mut() {
            *x = 0;
        }
        for (i, &w) in r.iter().enumerate() {
            if w == 0 {
                continue;
            }
            if 2 * i < out.len() {
                out[2 * i] = spread32(w as u32);
            }
            if 2 * i + 1 < out.len() {
                out[2 * i + 1] = spread32((w >> 32) as u32);
            }
        }
        self.reduce(out);
    }
}

/// gcd of two polynomials
pub fn poly_gcd(a: &[u64], b: &[u64]) -> Vec<u64> {
    let mut a = a.to_vec();
    let mut b = b.to_vec();
    loop {
        let db = match degree(&b) {
            None => return a,
            Some(d) => d,
        };
        // a ← a mod b
        while let Some(da) = degree(&a) {
            if da < db {
                break;
            }
            xor_shifted(&mut a, &b, da - db);
        }
        core::mem::swap(&mut a, &mut b);
    }
}

pub fn is_one(p: &[u64]) -> bool {
    degree(p) == Some(0)
}

/// x^(2^p) ≡ x (mod f). sieve > 0 checks gcd(x^(2^d)−x, f)=1 for d ≤ sieve
pub struct IrrJob {
    pub red: Reducer,
    pub r: Vec<u64>,
    tmp: Vec<u64>,
    pub iter: usize,
    pub sieve: usize,
    pub failed_at: usize, // 0 = not failed
    pub done: bool,
    pub result: bool,
}

impl IrrJob {
    pub fn new(f: &[u64], sieve: usize) -> IrrJob {
        let red = Reducer::new(f);
        let p = red.p;
        let ww = 2 * words(p) + 2;
        let mut r = vec![0u64; ww];
        r[0] = 2; // x
        let mut job = IrrJob { red, r, tmp: vec![0u64; ww], iter: 0, sieve, failed_at: 0, done: false, result: false };
        let f0 = get_bit(f, 0);
        let f1 = weight(f) % 2 == 1;
        if !(f0 && f1) {
            job.done = true;
            job.failed_at = 1;
        }
        job
    }
    pub fn p(&self) -> usize {
        self.red.p
    }
    /// do up to `k` squarings; returns true when finished
    pub fn step(&mut self, k: usize) -> bool {
        let p = self.red.p;
        let mut left = k;
        while !self.done && left > 0 {
            self.red.square(&self.r, &mut self.tmp);
            core::mem::swap(&mut self.r, &mut self.tmp);
            self.iter += 1;
            left -= 1;
            if self.iter <= self.sieve && self.iter < p {
                let mut g = self.r.clone();
                g[0] ^= 2;
                let gg = poly_gcd(&self.red.f, &g);
                if !is_one(&gg) {
                    self.done = true;
                    self.failed_at = self.iter;
                    self.result = false;
                }
            }
            if self.iter == p {
                self.done = true;
                let mut ok = self.r[0] == 2;
                for w in &self.r[1..] {
                    if *w != 0 {
                        ok = false;
                    }
                }
                self.result = ok;
                if !ok {
                    self.failed_at = p;
                }
            }
        }
        self.done
    }
}

/// bit sequence: bit `bit` of `nbits` consecutive outputs of g
pub fn output_bits(g: &mut Mt, bit: u32, nbits: usize) -> Vec<u64> {
    let mut v = vec![0u64; words(nbits) + 1];
    for i in 0..nbits {
        if (g.next_tempered() >> bit) & 1 == 1 {
            set_bit(&mut v, i);
        }
    }
    v
}

/// characteristic polynomial of a configured generator via BM on 2p+64 bits
pub fn char_poly(cfg: &MtConfig, seed: u64) -> (Vec<u64>, usize) {
    let p = cfg.p();
    let mut c = cfg.clone();
    c.temper = false;
    c.discard = 0;
    let mut g = Mt::new(c, seed);
    let nbits = 2 * p + 64;
    let bits = output_bits(&mut g, cfg.w - 1, nbits);
    let (cpoly, l) = berlekamp_massey(&bits, nbits, None);
    (reciprocal(&cpoly, l), l)
}

// -- state as a vector

/// window (x_k, …, x_{k+n−1}) → vector in GF(2)^p
pub fn window_to_vec(cfg: &MtConfig, win: &[u64]) -> Vec<u64> {
    let p = cfg.p();
    let mut v = vec![0u64; words(p)];
    let up = (cfg.w - cfg.r) as usize;
    for bi in 0..up {
        if (win[0] >> (cfg.r as usize + bi)) & 1 == 1 {
            set_bit(&mut v, bi);
        }
    }
    for q in 1..cfg.n {
        for bi in 0..cfg.w as usize {
            if (win[q] >> bi) & 1 == 1 {
                set_bit(&mut v, up + (q - 1) * cfg.w as usize + bi);
            }
        }
    }
    v
}

/// one step of the linear recurrence on the window
pub fn window_step(cfg: &MtConfig, win: &mut [u64]) {
    let n = cfg.n;
    let new = cfg.twist_word(win[0], win[1 % n], win[cfg.m % n]);
    for q in 0..n - 1 {
        win[q] = win[q + 1];
    }
    win[0] &= cfg.umask;
    win[n - 1] = new;
}

/// the p×p transition matrix B (row-major bit rows: row i, column j = ∂out_i/∂in_j)
pub fn transition_matrix(cfg: &MtConfig) -> Vec<Vec<u64>> {
    let p = cfg.p();
    let mut rows = vec![vec![0u64; words(p)]; p];
    for j in 0..p {
        let g = basis_state(cfg, j);
        let mut win = g.x.clone();
        window_step(cfg, &mut win);
        let v = window_to_vec(cfg, &win);
        for i in 0..p {
            if get_bit(&v, i) {
                set_bit(&mut rows[i], j);
            }
        }
    }
    rows
}

/// brute-force period of the state sequence (only for tiny p); None if > limit
pub struct PeriodJob {
    cfg: MtConfig,
    start: Vec<u64>,
    win: Vec<u64>,
    pub steps: u64,
    pub limit: u64,
    pub done: bool,
    pub found: bool,
}

impl PeriodJob {
    pub fn new(cfg: &MtConfig, limit: u64) -> PeriodJob {
        let g = basis_state(cfg, cfg.p() - 1);
        let mut win = g.x.clone();
        win[0] &= cfg.umask;
        PeriodJob { cfg: cfg.clone(), start: win.clone(), win, steps: 0, limit, done: false, found: false }
    }
    pub fn step(&mut self, k: u64) -> bool {
        let mut left = k;
        while !self.done && left > 0 {
            window_step(&self.cfg, &mut self.win);
            self.steps += 1;
            left -= 1;
            if self.win == self.start {
                self.done = true;
                self.found = true;
            } else if self.steps >= self.limit {
                self.done = true;
            }
        }
        self.done
    }
}

// -- k-distribution

/// k(v) for v = 1..w by incremental Gaussian elimination
pub struct KdistJob {
    p: usize,
    w: usize,
    kmax: usize,
    /// rows[t*w + j] = linear functional of the state giving bit j (MSB = 0) of output t
    rows: Vec<Vec<u64>>,
    pub v: usize, // next v to compute (1-based)
    pub k: Vec<u32>,
}

impl KdistJob {
    pub fn new(cfg: &MtConfig) -> KdistJob {
        let p = cfg.p();
        let w = cfg.w as usize;
        let kmax = p + 1;
        let pw = words(p);
        let mut rows = vec![vec![0u64; pw]; kmax * w];
        let mut c = cfg.clone();
        c.discard = 0;
        c.out_shift = 0;
        c.out_xor = 0;
        c.out_and = c.wm();
        for beta in 0..p {
            let mut g = basis_state(&c, beta);
            for t in 0..kmax {
                let z = g.next_tempered();
                let mut zz = z;
                while zz != 0 {
                    let bit = 63 - zz.leading_zeros() as usize; // bit index from LSB
                    zz &= !(1u64 << bit);
                    let j = w - 1 - bit; // MSB-first index
                    set_bit(&mut rows[t * w + j], beta);
                }
            }
        }
        KdistJob { p, w, kmax, rows, v: 1, k: Vec::new() }
    }

    /// compute k(v) for the next v; returns true when all v are done
    pub fn step(&mut self) -> bool {
        if self.v > self.w {
            return true;
        }
        let v = self.v;
        let p = self.p;
        let mut basis: Vec<Option<Vec<u64>>> = vec![None; p];
        let mut kv = 0u32;
        'outer: for t in 0..self.kmax {
            for j in 0..v {
                let mut row = self.rows[t * self.w + j].clone();
                loop {
                    match degree(&row) {
                        None => break 'outer, // dependent → k(v) = t
                        Some(lead) => match &basis[lead] {
                            Some(b) => {
                                for (x, y) in row.iter_mut().zip(b.iter()) {
                                    *x ^= *y;
                                }
                            }
                            None => {
                                basis[lead] = Some(row);
                                break;
                            }
                        },
                    }
                }
            }
            kv = (t + 1) as u32;
        }
        self.k.push(kv);
        self.v += 1;
        self.v > self.w
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mt::MtConfig;

    #[test]
    fn bm_small_lfsr() {
        // s_n = s_{n-1} ⊕ s_{n-4}: x^4 + x^3 + 1
        let mut s = vec![0u64; 2];
        let mut bits = [1u8, 0, 0, 1];
        let mut all = vec![];
        for i in 0..100 {
            let b = if i < 4 { bits[i] } else { all[i - 1] ^ all[i - 4] };
            all.push(b);
            if b == 1 {
                set_bit(&mut s, i);
            }
        }
        bits[0] = 0;
        let _ = bits;
        let (c, l) = berlekamp_massey(&s, 100, None);
        assert_eq!(l, 4);
        let f = reciprocal(&c, l);
        assert_eq!(f[0], 0b11001);
    }

    #[test]
    fn mt19937_degree() {
        let (f, l) = char_poly(&MtConfig::mt19937(), 5489);
        assert_eq!(l, 19937);
        assert_eq!(degree(&f), Some(19937));
        std::println!("MT19937 char poly weight = {}", weight(&f));
    }

    #[test]
    fn mt19937_primitive_sparse_matches_dense() {
        let (f, _) = char_poly(&MtConfig::mt19937(), 5489);
        let mut sp = Reducer::new(&f);
        assert!(sp.is_sparse());
        let ww = 2 * words(19937) + 2;
        let mut r1 = vec![0u64; ww];
        r1[0] = 2;
        let mut r2 = r1.clone();
        let mut t1 = vec![0u64; ww];
        // compare against naive bitwise reduction
        for _ in 0..40 {
            sp.square(&r1, &mut t1);
            core::mem::swap(&mut r1, &mut t1);
            // naive
            let mut sq = vec![0u64; ww];
            for i in 0..19937 {
                if get_bit(&r2, i) {
                    set_bit(&mut sq, 2 * i);
                }
            }
            for q in (19937..2 * 19937).rev() {
                if get_bit(&sq, q) {
                    xor_shifted(&mut sq, &f, q - 19937);
                }
            }
            r2 = sq;
            assert_eq!(r1, r2);
        }
        let mut j = IrrJob::new(&f, 0);
        while !j.step(5000) {}
        assert!(j.result, "MT19937 characteristic polynomial is primitive");
    }

    #[test]
    fn search_finds_mt607() {
        let cfg = MtConfig::new(32, 19, 9, 1, 0, 11, 0xffff_ffff, 7, 0x9d2c_5680, 15, 0xefc6_0000, 18, 1_812_433_253);
        let mut s = SearchJob::new(&cfg, 7);
        let mut rounds = 0;
        while s.found.is_empty() && rounds < 20000 {
            s.step(4096);
            rounds += 1;
        }
        std::println!("tried {} deg {} root {} small {} full {} found {:x?}", s.tried, s.rej_deg, s.rej_root, s.rej_small, s.rej_full, s.found);
        assert!(!s.found.is_empty());
    }

    #[test]
    fn irreducible_small() {
        // x^7 + x + 1 is primitive; x^7 + x^3 + x^2 + x + 1? (reducible? test with known) ; x^4+x^2+1 = (x^2+x+1)^2
        let mut j = IrrJob::new(&[0b1000_0011], 0);
        j.step(100);
        assert!(j.result);
        let mut j = IrrJob::new(&[0b10101], 0);
        j.step(100);
        assert!(!j.result);
        // x^127 + x + 1 is primitive (trinomial)
        let mut f = vec![0u64; 2];
        set_bit(&mut f, 127);
        set_bit(&mut f, 1);
        set_bit(&mut f, 0);
        let mut j = IrrJob::new(&f, 10);
        j.step(1000);
        assert!(j.result);
    }

    #[test]
    fn mt_family_irreducible() {
        for cfg in [MtConfig::mt11213b(), MtConfig::tt800()] {
            let (f, l) = char_poly(&cfg, 5489);
            assert_eq!(l, cfg.p());
            let mut j = IrrJob::new(&f, 12);
            while !j.step(1000) {}
            // TT800's char poly is primitive of degree 800 but 2^800-1 is not prime;
            // x^(2^800) ≡ x still holds for irreducible polynomials
            assert!(j.result, "p = {}", cfg.p());
        }
    }

    #[test]
    fn kdist_mt19937_like_small() {
        // a small MT: w=32 n=19 r=1 p=607 — check k(32) ≤ ⌊607/32⌋ and monotone sanity
        let mut cfg = MtConfig::new(32, 19, 9, 1, 0x9908_b0df, 11, 0xffff_ffff, 7, 0x9d2c_5680, 15, 0xefc6_0000, 18, 1_812_433_253);
        cfg.temper = true;
        let mut job = KdistJob::new(&cfg);
        while !job.step() {}
        for (i, &k) in job.k.iter().enumerate() {
            assert!(k as usize <= 607 / (i + 1));
        }
        std::println!("k(v) = {:?}", job.k);
    }

    #[test]
    fn transition_and_period_tiny() {
        // w=6 n=3 r=1 → p = 17; search a with full period by brute force
        let mut found = 0;
        for a in 32u64..64 {
            let mut cfg = MtConfig::new(6, 3, 1, 1, a, 6, 0x3f, 6, 0, 6, 0, 6, 1_812_433_253);
            cfg.temper = false;
            let mut pj = PeriodJob::new(&cfg, 1 << 18);
            while !pj.step(1 << 16) {}
            let (f, _l) = char_poly(&cfg, 1);
            let mut ij = IrrJob::new(&f, 0);
            while !ij.step(100) {}
            let full = pj.found && pj.steps == (1 << 17) - 1;
            assert_eq!(full, ij.result && degree(&f) == Some(17), "a = {a:#x}");
            if full {
                found += 1;
            }
        }
        assert!(found > 0);
        let cfg = MtConfig::new(8, 16, 7, 1, 0xb5, 3, 0xff, 2, 0, 4, 0, 5, 1_812_433_253);
        let b = transition_matrix(&cfg);
        assert_eq!(b.len(), 127);
    }
}

// -- searching for a twist matrix

/// Matsumoto–Nishimura's search, live: draw random a (top bit set so that A is
/// invertible), compute the characteristic polynomial by Berlekamp–Massey and
/// keep a if it is irreducible. With p a Mersenne exponent, irreducible ⇒
/// primitive ⇒ period 2^p − 1.
pub struct SearchJob {
    pub cfg: MtConfig,
    rng: u64,
    pub tried: u32,
    pub rej_deg: u32,
    pub rej_root: u32,
    pub rej_small: u32,
    pub rej_full: u32,
    pub found: Vec<u64>,
    pub cur_a: u64,
    cur: Option<IrrJob>,
    pub sieve: usize,
    /// small word sizes: enumerate every a with the top bit set instead of sampling
    pub exhaustive: bool,
    counter: u64,
    pub exhausted: bool,
}

impl SearchJob {
    pub fn new(cfg: &MtConfig, seed: u64) -> SearchJob {
        SearchJob {
            cfg: *cfg,
            rng: seed | 1,
            tried: 0,
            rej_deg: 0,
            rej_root: 0,
            rej_small: 0,
            rej_full: 0,
            found: Vec::new(),
            cur_a: 0,
            cur: None,
            sieve: 12.min(cfg.p().saturating_sub(1)),
            exhaustive: cfg.w <= 12,
            counter: 0,
            exhausted: false,
        }
    }
    fn rand(&mut self) -> u64 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn cur_iter(&self) -> usize {
        self.cur.as_ref().map(|j| j.iter).unwrap_or(0)
    }
    /// spend roughly `budget` squarings; returns number of candidates finished
    pub fn step(&mut self, budget: usize) -> u32 {
        let p = self.cfg.p();
        let mut left = budget as isize;
        let mut finished = 0;
        while left > 0 && !self.exhausted {
            if self.cur.is_none() {
                let w = self.cfg.w;
                let a = if self.exhaustive {
                    if self.counter >= 1u64 << (w - 1) {
                        self.exhausted = true;
                        break;
                    }
                    self.counter += 1;
                    (1u64 << (w - 1)) | (self.counter - 1)
                } else {
                    (self.rand() & self.cfg.wm()) | (1u64 << (w - 1))
                };
                self.cur_a = a;
                self.tried += 1;
                let mut c = self.cfg;
                c.a = a;
                let (f, l) = char_poly(&c, 0x1234_5678);
                left -= (p / 64).max(1) as isize; // BM cost, in squaring units (rough)
                if l != p {
                    self.rej_deg += 1;
                    finished += 1;
                    continue;
                }
                let j = IrrJob::new(&f, self.sieve);
                if j.done {
                    self.rej_root += 1;
                    finished += 1;
                    continue;
                }
                self.cur = Some(j);
            }
            let j = self.cur.as_mut().unwrap();
            let chunk = (left as usize).min(64);
            j.step(chunk);
            left -= chunk as isize;
            if j.done {
                if j.result {
                    self.found.push(self.cur_a);
                } else if j.failed_at <= self.sieve {
                    self.rej_small += 1;
                } else {
                    self.rej_full += 1;
                }
                self.cur = None;
                finished += 1;
            }
        }
        finished
    }
}
