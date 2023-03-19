//! the Source trait + a catalog of mt variants. also has some deliberately
//! broken generators (randu, minstd, lcg, xorshift) so you can watch the
//! distribution tests fall apart on them.

use crate::kinds::Kind;
use crate::mt::{Mt, MtConfig, SeedMode};
use crate::params as P;
use alloc::boxed::Box;
use crate::sfmt::Sfmt;
use crate::tinymt::TinyMt;

pub const V_MT19937: u32 = 0;
pub const V_PYTHON: u32 = 1;
pub const V_KNUTH_NOAA: u32 = 2;
pub const V_GLIB20: u32 = 3;
pub const V_PHP_COKUS: u32 = 4;
pub const V_PHP_BROKEN: u32 = 5;
pub const V_PHP71: u32 = 6;
pub const V_PPSSPP: u32 = 7;
pub const V_MYBB: u32 = 8;
pub const V_ECJ: u32 = 9;
pub const V_MT64: u32 = 10;
pub const V_STIM: u32 = 11;
pub const V_APERY: u32 = 12;
pub const V_MT11213B: u32 = 13;
pub const V_TT800: u32 = 14;
pub const V_GNUGO: u32 = 15;
pub const V_R: u32 = 16;
pub const V_GZ: u32 = 17;
pub const V_COKUS2: u32 = 18;

pub fn variant_config(id: u32) -> MtConfig {
    let mut c = MtConfig::mt19937();
    match id {
        V_PYTHON => c.seed_mode = SeedMode::InitByArray,
        V_KNUTH_NOAA => {
            c.seed_mode = SeedMode::Knuth69069;
            c.zero_sub = P::KNUTH_ZERO_SEED;
        }
        V_GLIB20 => {
            c.seed_mode = SeedMode::Knuth69069;
            c.zero_sub = P::GLIB20_ZERO_SEED;
        }
        V_PHP_COKUS => {
            c.seed_mode = SeedMode::Cokus;
            c.out_shift = P::PHP_OUT_SHIFT;
            c.out_and = P::PHP_OUT_AND;
        }
        V_PHP_BROKEN => {
            c.lowbit_from_xk = true;
            c.out_shift = P::PHP_OUT_SHIFT;
            c.out_and = P::PHP_OUT_AND;
        }
        V_PHP71 => {
            c.out_shift = P::PHP_OUT_SHIFT;
            c.out_and = P::PHP_OUT_AND;
        }
        V_PPSSPP => c.lmask = P::PPSSPP_LMASK,
        V_MYBB => {
            c.out_shift = P::PHP_OUT_SHIFT;
            c.out_xor = P::MYBB_OUT_XOR;
            c.out_and = P::PHP_OUT_AND;
        }
        V_ECJ => c.discard = P::ECJ_DISCARD,
        V_MT64 => c = MtConfig::mt19937_64(),
        V_STIM => {
            c = MtConfig::mt19937_64();
            c.seed_xor = P::STIM_SEED_XOR;
        }
        V_APERY => {
            c = MtConfig::mt19937_64();
            c.out_and = P::APERY_OUT_AND;
        }
        V_MT11213B => c = MtConfig::mt11213b(),
        V_TT800 => c = MtConfig::tt800(),
        V_GNUGO => {
            c = MtConfig::tt800();
            c.l = P::TT800_GNUGO_L; // GNU Go drops the final y ^= y >> 16
            c.seed_mode = SeedMode::GnuGo;
        }
        V_R => c.seed_mode = SeedMode::RLcg,
        V_GZ => c.seed_mode = SeedMode::SeedSeq,
        V_COKUS2 => c.seed_mode = SeedMode::CokusShift,
        _ => {}
    }
    c
}

pub enum Source {
    Mt(Mt),
    Tiny(TinyMt),
    Sfmt(Sfmt),
    /// IBM RANDU: x ← 65539·x mod 2³¹ (falls on 15 planes in 3-D)
    Randu(u32),
    /// Park–Miller minimal standard: x ← 16807·x mod (2³¹−1)
    Minstd(u32),
    /// x ← 1664525·x + 1013904223 mod 2³² (Numerical Recipes; weak low bits)
    Lcg32(u32),
    /// Marsaglia xorshift32 (13, 17, 5)
    Xorshift32(u32),
    /// any other catalogued kind (kinds.rs)
    Kind(Box<Kind>),
}

pub const S_MT: u32 = 0; // configured MT (any variant)
pub const S_TINYMT: u32 = 20;
pub const S_TINYMT_ZERO: u32 = 21;
pub const S_SFMT: u32 = 22;
pub const S_RANDU: u32 = 30;
pub const S_MINSTD: u32 = 31;
pub const S_LCG32: u32 = 32;
pub const S_XORSHIFT32: u32 = 33;


impl Source {
    pub fn new_special(kind: u32, seed: u64) -> Option<Source> {
        let s32 = seed as u32;
        Some(match kind {
            S_TINYMT => Source::Tiny(TinyMt::default_params(s32)),
            S_TINYMT_ZERO => Source::Tiny(TinyMt::new(s32, 0, 0, 0)),
            S_SFMT => Source::Sfmt(Sfmt::new(s32)),
            S_RANDU => Source::Randu(s32 | 1),
            S_MINSTD => Source::Minstd(if s32 as u64 % P::MINSTD_M == 0 { 1 } else { (s32 as u64 % P::MINSTD_M) as u32 }), // a 0 state would stall the LCG
            S_LCG32 => Source::Lcg32(s32),
            S_XORSHIFT32 => Source::Xorshift32(if s32 == 0 { P::XORSHIFT32_ZERO_SEED } else { s32 }),
            _ => return crate::kinds::boxed(kind, seed).map(Source::Kind),
        })
    }

    /// (value, number of meaningful bits)
    pub fn next_raw(&mut self) -> (u64, u32) {
        match self {
            Source::Mt(m) => {
                let v = m.next();
                let bits = m.cfg.w.saturating_sub(m.cfg.out_shift);
                (v, bits)
            }
            Source::Tiny(t) => (t.next() as u64, 32),
            Source::Sfmt(s) => (s.next() as u64, 32),
            Source::Randu(x) => {
                *x = ((*x as u64 * P::RANDU_A) & P::RANDU_MASK) as u32;
                (*x as u64, 31)
            }
            Source::Minstd(x) => {
                *x = ((*x as u64 * P::MINSTD_A) % P::MINSTD_M) as u32;
                (*x as u64, 31)
            }
            Source::Lcg32(x) => {
                *x = x.wrapping_mul(P::LCG32_A).wrapping_add(P::LCG32_C);
                (*x as u64, 32)
            }
            Source::Xorshift32(x) => {
                let (a, b, c) = P::XORSHIFT32_SHIFTS;
                *x ^= *x << a;
                *x ^= *x >> b;
                *x ^= *x << c;
                (*x as u64, 32)
            }
            Source::Kind(k) => {
                let v = k.next_native();
                match k.range() {
                    // an index in [0, range): spread it over 32 bits so the
                    // histograms and bit plots compare like with like
                    Some(r) => ((v as f64 / r * 4_294_967_296.0) as u64 & 0xffff_ffff, 32),
                    None => (v, k.native_bits()),
                }
            }
        }
    }

    /// 32 bits, left-aligned
    pub fn next_u32(&mut self) -> u32 {
        let (v, bits) = self.next_raw();
        if bits >= 32 { (v >> (bits - 32)) as u32 } else { (v << (32 - bits)) as u32 }
    }

    /// uniform in [0,1): v / 2^bits  (53-bit resolution at most)
    pub fn next_unit(&mut self) -> f64 {
        let (v, bits) = self.next_raw();
        if bits > 53 {
            (v >> (bits - 53)) as f64 / (1u64 << 53) as f64
        } else {
            v as f64 / (1u64 << bits) as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec::Vec;

    /// compare against vectors produced by compiling the prngs repo's own C code
    /// (tools/cref/harness.c → ref.txt)
    #[test]
    fn matches_repo_c() {
        let txt = include_str!("../../tools/cref/ref.txt");
        let mut checked = 0;
        for line in txt.lines() {
            let (head, rest) = line.split_once(':').unwrap();
            let mut hp = head.split(' ');
            let name = hp.next().unwrap();
            let seed: u64 = hp.next().unwrap().parse().unwrap();
            let (firsts, thousandth) = rest.split_once('|').unwrap();
            let want: Vec<u64> = firsts.split_whitespace().map(|x| x.parse().unwrap()).collect();
            let want1000: u64 = thousandth.trim().parse().unwrap();
            let mut src: Source = match name {
                "mt19937" => Source::Mt(Mt::new(variant_config(V_MT19937), seed)),
                "mt64" => Source::Mt(Mt::new(variant_config(V_MT64), seed)),
                "mt11213" => Source::Mt(Mt::new(variant_config(V_MT11213B), seed)),
                "tt800" => Source::Mt(Mt::new(variant_config(V_TT800), seed)),
                "tinymt32" => Source::new_special(S_TINYMT, seed).unwrap(),
                "rusefi" => Source::new_special(S_TINYMT_ZERO, seed).unwrap(),
                "sfmt" => Source::new_special(S_SFMT, seed).unwrap(),
                "php_cokus_std_noshift" => {
                    let mut c = MtConfig::mt19937();
                    c.seed_mode = SeedMode::Cokus;
                    Source::Mt(Mt::new(c, seed))
                }
                "php_cokus_broken_shift" => {
                    let mut c = variant_config(V_PHP_COKUS);
                    c.lowbit_from_xk = true;
                    Source::Mt(Mt::new(c, seed))
                }
                "php_wagner_broken_shift" => Source::Mt(Mt::new(variant_config(V_PHP_BROKEN), seed)),
                "php_wagner_std_shift" => Source::Mt(Mt::new(variant_config(V_PHP71), seed)),
                "ppsspp" => Source::Mt(Mt::new(variant_config(V_PPSSPP), seed)),
                "glib20" => Source::Mt(Mt::new(variant_config(V_GLIB20), seed)),
                "seeded_byarray" => Source::Mt(Mt::new(variant_config(V_PYTHON), seed)),
                "noaa" | "beast2" => Source::Mt(Mt::new(variant_config(V_KNUTH_NOAA), seed)),
                "ecj" => Source::Mt(Mt::new(variant_config(V_ECJ), seed)),
                "stim" => Source::Mt(Mt::new(variant_config(V_STIM), seed)),
                "apery" => Source::Mt(Mt::new(variant_config(V_APERY), seed)),
                "mybb_mask1" => {
                    let mut c = variant_config(V_MYBB);
                    c.out_xor = 1;
                    Source::Mt(Mt::new(c, seed))
                }
                _ => continue, // the other kinds are checked in kinds.rs
            };
            let mut got = Vec::new();
            for _ in 0..1000 {
                got.push(src.next_raw().0);
            }
            assert_eq!(&got[..6], &want[..], "{name} seed {seed}");
            assert_eq!(got[999], want1000, "{name} seed {seed} #1000");
            checked += 1;
        }
        assert_eq!(checked, 100, "{}", String::from("all lines"));
    }
}
