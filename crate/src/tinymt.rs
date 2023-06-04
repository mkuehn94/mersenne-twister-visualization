//! tinymt32 (saito & matsumoto 2011). 127-bit state, period 2^127-1.
//! ported from the prngs repo which follows the reference tinymt32.c.
//!
//! FIXME: the zero-fix constants spell out "TINY" in ASCII which is a bit silly

#[derive(Clone, Debug)]
pub struct TinyMt {
    pub s: [u32; 4],
    pub mat1: u32,
    pub mat2: u32,
    pub tmat: u32,
}

use crate::params::{
    INIT_GENRAND_F32, TINYMT32_MASK as MASK, TINYMT32_MAT1, TINYMT32_MAT2, TINYMT32_MIN_LOOP, TINYMT32_PRE_LOOP,
    TINYMT32_SH0 as SH0, TINYMT32_SH1 as SH1, TINYMT32_SH8 as SH8, TINYMT32_TMAT, TINYMT32_ZERO_FIX,
};

impl TinyMt {
    pub fn new(seed: u32, mat1: u32, mat2: u32, tmat: u32) -> TinyMt {
        let mut t = TinyMt { s: [0; 4], mat1, mat2, tmat };
        t.seed(seed);
        t
    }
    pub fn default_params(seed: u32) -> TinyMt {
        TinyMt::new(seed, TINYMT32_MAT1, TINYMT32_MAT2, TINYMT32_TMAT)
    }

    /// part 1 of seeding: fill and mix (before the 8 warm-up steps)
    pub fn seed_fill(&mut self, seed: u32) {
        self.s = [seed, self.mat1, self.mat2, self.tmat];
        for i in 1..TINYMT32_MIN_LOOP {
            let p = self.s[((i - 1) & 3) as usize];
            self.s[(i & 3) as usize] ^= i.wrapping_add((INIT_GENRAND_F32 as u32).wrapping_mul(p ^ (p >> 30)));
        }
        // check for the all-zero state and fix it
        if (self.s[0] & MASK) == 0 && self.s[1] == 0 && self.s[2] == 0 && self.s[3] == 0 {
            self.s = TINYMT32_ZERO_FIX;
        }
    }

    pub fn seed(&mut self, seed: u32) {
        self.seed_fill(seed);
        for _ in 0..TINYMT32_PRE_LOOP {
            self.next_state();
        }
    }

    pub fn next_state(&mut self) {
        let mut y = self.s[3];
        let mut x = (self.s[0] & MASK) ^ self.s[1] ^ self.s[2];
        x ^= x << SH0;
        y ^= (y >> SH0) ^ x;
        self.s[0] = self.s[1];
        self.s[1] = self.s[2];
        self.s[2] = x ^ (y << SH1);
        self.s[3] = y;
        if y & 1 != 0 {
            self.s[1] ^= self.mat1;
            self.s[2] ^= self.mat2;
        }
    }

    pub fn temper(&self) -> u32 {
        let mut t0 = self.s[3];
        let t1 = self.s[0].wrapping_add(self.s[2] >> SH8);
        t0 ^= t1;
        if t1 & 1 != 0 {
            t0 ^= self.tmat;
        }
        t0
    }

    pub fn next(&mut self) -> u32 {
        self.next_state();
        self.temper()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference() {
        // tinymt32 reference check output, seed 1
        let mut t = TinyMt::default_params(1);
        let v: alloc::vec::Vec<u32> = (0..5).map(|_| t.next()).collect();
        assert_eq!(v, [2_545_341_989, 981_918_433, 3_715_302_833, 2_387_538_352, 3_591_001_365]);
    }
}
