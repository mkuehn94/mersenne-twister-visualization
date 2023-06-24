//! mt cloning / cracking. untemper n consecutive outputs → full state →
//! walk backwards through the twist to get the seed. once you have the state
//! the rest is just algebra.

use crate::mt::{MtConfig, RawSeq};
use crate::params::KNUTH_MUL;
use alloc::vec::Vec;

pub fn untemper_all(cfg: &MtConfig, outputs: &[u64]) -> Vec<u64> {
    outputs.iter().map(|&z| cfg.untemper(z)).collect()
}

/// given n raw words x_j … x_{j+n−1}, predict the next `k` tempered outputs
pub fn predict(cfg: &MtConfig, window: &[u64], k: usize) -> Vec<u64> {
    let mut seq = RawSeq::from_state(cfg, window);
    (0..k).map(|_| cfg.temper(seq.step())).collect()
}

// invert the "multiply by A" step. works because a has its top bit set,
// so we can read back whether a was xored in from the result's top bit.
pub fn a_inv(cfg: &MtConfig, v: u64) -> u64 {
    let top = (v >> (cfg.w - 1)) & 1;
    let v = if top == 1 { v ^ cfg.a } else { v };
    ((v << 1) | top) & cfg.wm()
}

/// multiplicative inverse mod 2^64 of an odd number (Newton)
pub fn inv_odd(f: u64) -> u64 {
    let mut x = f;
    for _ in 0..6 {
        x = x.wrapping_mul(2u64.wrapping_sub(f.wrapping_mul(x)));
    }
    x
}

/// How the seeded state x_0 … x_{n−1} was filled, as far as a rewind can tell.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Seeding {
    Unknown = 0,
    /// x_i = f·(x_{i−1} ⊕ (x_{i−1} ≫ (w−2))) + i
    InitGenrand = 1,
    /// x_i = 69069·x_{i−1}
    Knuth69069 = 2,
}

/// Run the word sequence backwards from n known consecutive words.
///
/// The recurrence x_{k+n} = x_{k+m} ⊕ (x_kᵘ | x_{k+1}ˡ)A can be solved for
/// x_k: equation k gives its upper part, equation k−1 its lower part. So from
/// x_k … x_{k+n−1} we get x_{k−1}, and so on, back to the seeded state. At
/// every position we test whether the window looks like a freshly seeded
/// state; if it does, the seed falls out by inverting x_1 = f(x_0).
pub struct RewindJob {
    pub cfg: MtConfig,
    ring: Vec<u64>,
    head: usize, // ring[head] = first word of the window
    /// words stepped back so far
    pub steps: u64,
    pub found: Seeding,
    pub seed: u64,
    pub max_steps: u64,
}

impl RewindJob {
    pub fn new(cfg: &MtConfig, window: &[u64], max_steps: u64) -> RewindJob {
        // max_steps bounds the rewind; in practice we stop way before this
        // because the seeding pattern becomes recognizable quickly
        RewindJob { cfg: *cfg, ring: window.to_vec(), head: 0, steps: 0, found: Seeding::Unknown, seed: 0, max_steps }
    }
    #[inline]
    fn w(&self, t: usize) -> u64 {
        self.ring[(self.head + t) % self.ring.len()]
    }
    fn back_one(&mut self) {
        let c = &self.cfg;
        let (n, m) = (c.n, c.m);
        // equation k−1: x_{k−1+n} ⊕ x_{k−1+m} = (x_{k−1}ᵘ | x_kˡ)A
        let y1 = a_inv(c, self.w(n - 1) ^ self.w(m - 1));
        // equation k−2: x_{k−2+n} ⊕ x_{k−2+m} = (x_{k−2}ᵘ | x_{k−1}ˡ)A
        let y2 = a_inv(c, self.w(n - 2) ^ self.w(m - 2));
        let prev = (y1 & c.umask) | (y2 & c.lmask);
        self.head = (self.head + n - 1) % n;
        self.ring[self.head] = prev; // overwrites the old last word
        self.steps += 1;
    }
    fn f_gen(&self, x: u64, i: u64) -> u64 {
        let c = &self.cfg;
        c.f.wrapping_mul(x ^ (x >> (c.w - 2))).wrapping_add(i) & c.wm()
    }
    // TODO: also detect init_by_array (CPython random.seed) here. the two
    // extra mixing rounds make the pattern much less obvious, haven't found
    // a cheap test for it yet — right now a python-seeded generator just
    // comes back as Unknown (the rewind test relies on that).
    /// does the current window look like x_0 … x_{n−1} right after seeding?
    fn check(&mut self) -> bool {
        let c = self.cfg;
        let n = c.n;
        let ts = [1usize, 2, 3, 4, n - 3, n - 2, n - 1];
        if ts.iter().all(|&t| self.w(t) == self.f_gen(self.w(t - 1), t as u64) || t == 1) && self.w(1) == self.f_gen(self.recover_x0_genrand(), 1) {
            self.found = Seeding::InitGenrand;
            self.seed = self.recover_x0_genrand();
            return true;
        }
        if ts.iter().skip(1).all(|&t| self.w(t) == self.w(t - 1).wrapping_mul(KNUTH_MUL) & c.wm()) {
            self.found = Seeding::Knuth69069;
            self.seed = self.w(1).wrapping_mul(inv_odd(KNUTH_MUL)) & c.wm();
            return true;
        }
        false
    }
    /// x_0 from x_1 = f·(x_0 ⊕ (x_0 ≫ (w−2))) + 1
    fn recover_x0_genrand(&self) -> u64 {
        let c = &self.cfg;
        let t = self.w(1).wrapping_sub(1).wrapping_mul(inv_odd(c.f)) & c.wm();
        crate::mt::inv_shr_xor(t, c.w - 2, c.wm(), c.w)
    }
    /// step back up to k words; true when finished (found or gave up)
    pub fn step(&mut self, k: u64) -> bool {
        if self.found != Seeding::Unknown || self.steps >= self.max_steps {
            return true;
        }
        for _ in 0..k {
            if self.check() {
                return true;
            }
            if self.steps >= self.max_steps {
                return true;
            }
            self.back_one();
        }
        self.check()
    }
    /// the current window (after a successful search: the seeded state;
    /// x_0's lower bits are not recoverable and are replaced by the seed)
    pub fn window(&self) -> Vec<u64> {
        let mut v: Vec<u64> = (0..self.cfg.n).map(|t| self.w(t)).collect();
        if self.found != Seeding::Unknown {
            v[0] = self.seed;
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mt::Mt;
    #[test]
    fn clone_works_at_any_offset() {
        for cfg in [MtConfig::mt19937(), MtConfig::mt19937_64()] {
            let mut g = Mt::new(cfg, 0xC0FFEE);
            for _ in 0..1000 {
                g.next();
            }
            let obs: Vec<u64> = (0..cfg.n).map(|_| g.next()).collect();
            let win = untemper_all(&cfg, &obs);
            let pred = predict(&cfg, &win, 2000);
            let actual: Vec<u64> = (0..2000).map(|_| g.next()).collect();
            assert_eq!(pred, actual);
        }
    }

    #[test]
    fn rewind_finds_seed_and_offset() {
        use crate::sources::{V_KNUTH_NOAA, V_MT19937, V_PYTHON, variant_config};
        for (v, want) in [(V_MT19937, Seeding::InitGenrand), (V_KNUTH_NOAA, Seeding::Knuth69069), (V_PYTHON, Seeding::Unknown)] {
            for (seed, used) in [(0xDEAD_BEEFu64, 54_321u64), (5489, 0), (1, 623), (7, 624), (0x8000_0001, 99_999)] {
                let cfg = variant_config(v);
                let mut g = Mt::new(cfg, seed);
                for _ in 0..used {
                    g.next();
                }
                let obs: Vec<u64> = (0..cfg.n).map(|_| g.next()).collect();
                let win = untemper_all(&cfg, &obs);
                let mut j = RewindJob::new(&cfg, &win, 200_000);
                while !j.step(10_000) {}
                assert_eq!(j.found, want, "variant {v} seed {seed}");
                if want != Seeding::Unknown {
                    assert_eq!(j.seed, seed, "variant {v}");
                    assert_eq!(j.steps, used + cfg.n as u64, "variant {v} seed {seed}");
                    let mut fresh = Mt::new(cfg, seed);
                    fresh.seed(seed);
                    assert_eq!(&j.window()[1..], &fresh.x[1..]);
                }
            }
        }
    }
}
