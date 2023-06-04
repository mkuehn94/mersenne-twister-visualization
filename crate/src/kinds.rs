//! tinymt64, melg, xsadd, and the "wrapper" kinds that just bolt a different
//! seed hash or output transform onto an mt core. all checked against the C
//! ref in tools/cref (tests at the bottom).

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::fm;
use crate::params as P;
use crate::mt::{Mt, MtConfig};
use crate::sfmt::{self, Sfmt};
use crate::sources::{V_GZ, V_KNUTH_NOAA, V_MT19937, V_MT64, V_R, variant_config};

// -- tinymt64

#[derive(Clone, Debug)]
pub struct TinyMt64 {
    pub s: [u64; 2],
    pub mat1: u32,
    pub mat2: u32,
    pub tmat: u64,
}

pub use crate::params::{TINYMT64_MASK as T64_MASK, TINYMT64_MAT1 as T64_MAT1, TINYMT64_MAT2 as T64_MAT2, TINYMT64_TMAT as T64_TMAT};

impl TinyMt64 {
    pub fn new(seed: u64, mat1: u32, mat2: u32, tmat: u64) -> TinyMt64 {
        let mut t = TinyMt64 { s: [0; 2], mat1, mat2, tmat };
        t.seed(seed);
        t
    }
    pub fn seed(&mut self, seed: u64) {
        self.s = [seed ^ ((self.mat1 as u64) << 32), self.mat2 as u64 ^ self.tmat];
        for i in 1..P::TINYMT64_MIN_LOOP {
            let p = self.s[((i - 1) & 1) as usize];
            self.s[(i & 1) as usize] ^= i.wrapping_add(P::INIT_GENRAND_F64.wrapping_mul(p ^ (p >> 62)));
        }
        if self.s[0] & T64_MASK == 0 && self.s[1] == 0 {
            self.s = P::TINYMT64_ZERO_FIX;
        }
    }
    pub fn next_state(&mut self) {
        self.s[0] &= T64_MASK;
        let mut x = self.s[0] ^ self.s[1];
        x ^= x << P::TINYMT64_SH0;
        x ^= x >> 32; // swap-and-mix of the two halves, fixed in the reference
        x ^= x << 32;
        x ^= x << P::TINYMT64_SH1;
        self.s[0] = self.s[1];
        self.s[1] = x;
        if (x & 1) == 1 {
            self.s[0] ^= self.mat1 as u64;
            self.s[1] ^= (self.mat2 as u64) << 32;
        }
    }
    pub fn temper(&self) -> u64 {
        let mut x = self.s[0].wrapping_add(self.s[1]);
        x ^= self.s[0] >> P::TINYMT64_SH8;
        if x & 1 != 0 {
            x ^= self.tmat;
        }
        x
    }
    pub fn next(&mut self) -> u64 {
        self.next_state();
        self.temper()
    }
}

// -- melg

pub use crate::params::{MELG, MelgParams};

#[derive(Clone)]
pub struct Melg {
    pub p: &'static MelgParams,
    pub x: Vec<u64>,
    pub lung: u64,
    pub i: usize,
}

/// the four pieces of one MELG step, for display: (x, new lung, new x_i, output)
pub struct MelgStep {
    pub x: u64,
    pub lung: u64,
    pub xi: u64,
    pub out: u64,
}

impl Melg {
    pub fn new(p: &'static MelgParams, seed: u64) -> Melg {
        let mut m = Melg { p, x: alloc::vec![0; p.nn], lung: 0, i: 0 };
        m.seed(seed);
        m
    }
    pub fn seed(&mut self, seed: u64) {
        let nn = self.p.nn;
        self.x[0] = seed;
        for i in 1..nn {
            let q = self.x[i - 1];
            self.x[i] = P::MELG_SEED_F.wrapping_mul(q ^ (q >> 62)).wrapping_add(i as u64);
        }
        let q = self.x[nn - 1];
        self.lung = P::MELG_SEED_F.wrapping_mul(q ^ (q >> 62)).wrapping_add(nn as u64);
        self.i = 0;
    }
    pub fn step(&mut self) -> MelgStep {
        let p = self.p;
        let nn = p.nn;
        let i = self.i;
        let mu = p.masku();
        let x = (self.x[i] & mu) | (self.x[(i + 1) % nn] & !mu);
        let ax = (x >> 1) ^ if x & 1 != 0 { p.a } else { 0 };
        self.lung = ax ^ self.x[(i + p.mm) % nn] ^ (self.lung ^ (self.lung << p.neg));
        let xi = x ^ (self.lung ^ (self.lung >> p.pos));
        self.x[i] = xi;
        let out = (xi ^ (xi << p.shift1)) ^ (self.x[(i + p.lag1) % nn] & p.mask1);
        self.i = (i + 1) % nn;
        MelgStep { x, lung: self.lung, xi, out }
    }
    pub fn next(&mut self) -> u64 {
        self.step().out
    }
}

// -- xsadd

pub fn xsadd_next_state(s: &mut [u32; 4]) {
    let mut t = s[0];
    t ^= t << P::XSADD_SH1;
    t ^= t >> P::XSADD_SH2;
    t ^= s[3] << P::XSADD_SH3;
    *s = [s[1], s[2], s[3], t];
}
pub fn xsadd_seed(seed: u32) -> [u32; 4] {
    let mut s = [seed, 0, 0, 0];
    for i in 1..P::XSADD_LOOP {
        let p = s[((i - 1) & 3) as usize];
        s[(i & 3) as usize] ^= i.wrapping_add(P::XSADD_SEED_F.wrapping_mul(p ^ (p >> 30)));
    }
    if s == [0; 4] {
        s = P::XSADD_ZERO_FIX;
    }
    for _ in 0..P::XSADD_LOOP {
        xsadd_next_state(&mut s);
    }
    s
}

// -- output mappings on MT

/// srs_log from sdcmicro_recordswap_exp_race.c (a portable log, reproduced
/// operation for operation so the race order matches the C)
fn srs_log(y: f64) -> f64 {
    use P::{SRS_LN2_HI as LN2_HI, SRS_LN2_LO as LN2_LO};
    let u = y.to_bits();
    let mut e = ((u >> 52) & 0x7ff) as i32 - 1023;
    let mut m = f64::from_bits((u & 0x000f_ffff_ffff_ffff) | 0x3ff0_0000_0000_0000);
    if m > P::SRS_SQRT2 {
        m *= 0.5;
        e += 1;
    }
    let z = (m - 1.0) / (m + 1.0);
    let z2 = z * z;
    let mut r = 1.0 / 25.0;
    for d in [23.0, 21.0, 19.0, 17.0, 15.0, 13.0, 11.0, 9.0, 7.0, 5.0, 3.0, 1.0] {
        r = r * z2 + 1.0 / d;
    }
    e as f64 * LN2_HI + (e as f64 * LN2_LO + 2.0 * (z * r))
}

#[derive(Clone)]
pub enum Map {
    /// the core's own output
    Id,
    /// low 32 bits of a 64-bit core (profanity's next_u32); native stays 64-bit
    Id64,
    /// (a≫5 · 2²⁶ + b≫6 [+ 0.4]) / 2⁵³ → ⌊·2³²⌋   (Ruby / Octave)
    Real53 { plus04: bool },
    /// R unif_rand → ⌊dn·u⌋ (R ≤ 3.5 sample())
    RIndex(f64),
    /// R unif_rand → ⌊u·10^d + ½⌋ (cellKey record keys)
    CellKey(f64),
    /// GLib g_rand_double: (a/2³² + b)/2³², redraw if ≥ 1
    GlibDouble,
    /// GLib 2.2 g_rand_int_range rejection, values in [0, dist)
    GlibRange(u32),
    /// GIMP solid noise: 32 random swaps of a 64-entry table, then cycle it
    Perm64 { perm: [u8; P::GIMP_TABLE], idx: usize },
    /// sdcMicro: 1/Exp(1) race over `pool` keys, emit the order
    ExpRace { pool: usize, order: [u8; P::SDC_MAX_POOL], pos: usize, have: bool },
    /// quackle: 64-bit rejection to [lo, hi]
    Reject64 { lo: u64, hi: u64 },
}

#[derive(Clone)]
pub enum Core {
    Mt(Mt),
    Tiny64(TinyMt64),
    Melg(Melg),
    Sfmt(Sfmt),
    Xsadd([u32; 4]),
}

#[derive(Clone)]
pub struct Kind {
    pub id: u32,
    pub core: Core,
    pub map: Map,
    /// the seed actually handed to the core generator (after hashing/truncation)
    pub core_seed: u64,
}

pub const K_TINYMT64: u32 = 100;
pub const K_WRK2: u32 = 101;
pub const K_MELG0: u32 = 110; // 110..=116 in MELG order
pub const K_SFMT0: u32 = 120; // 120..=129 in sfmt::PARAMS order
pub const K_XSADD: u32 = 130;
pub const K_GZ: u32 = 140;
pub const K_MINECRAFT: u32 = 141;
pub const K_OASIS07: u32 = 142;
pub const K_OASIS11: u32 = 143;
pub const K_OCTAVE: u32 = 144;
pub const K_RUBY: u32 = 145;
pub const K_RINDEX: u32 = 146;
pub const K_CELLKEY: u32 = 147;
pub const K_GLIB_DOUBLE: u32 = 148;
pub const K_GLIB_RANGE: u32 = 149;
pub const K_GIMP: u32 = 150;
pub const K_SDCMICRO: u32 = 151;
pub const K_BEAST2_M1: u32 = 152;
pub const K_PROFANITY: u32 = 153;
pub const K_QISKIT: u32 = 154;
pub const K_QUACKLE: u32 = 155;

fn mt(v: u32, seed: u64) -> Core {
    Core::Mt(Mt::new(variant_config(v), seed))
}

/// the seed the kind passes on to its core generator
pub fn core_seed(id: u32, seed: u64) -> u64 {
    match id {
        K_MINECRAFT => minecraft_seed(P::MINECRAFT_CHUNK.0, P::MINECRAFT_CHUNK.1),
        K_OASIS07 => oasis_seed(seed, P::OASIS_GE_07.0, P::OASIS_GE_07.1),
        K_OASIS11 => oasis_seed(seed, P::OASIS_GE_11.0, P::OASIS_GE_11.1),
        K_RINDEX | K_CELLKEY | K_SDCMICRO | K_PROFANITY | K_QUACKLE => seed & 0xffff_ffff,
        K_BEAST2_M1 => {
            if seed == 1 { P::BEAST2_GOOD_SEED } else { seed.wrapping_sub(1) & 0xffff_ffff }
        }
        _ if (K_SFMT0..K_SFMT0 + 10).contains(&id) || id == K_XSADD => seed & 0xffff_ffff,
        _ => seed,
    }
}

impl Kind {
    pub fn new(id: u32, seed: u64) -> Option<Kind> {
        let core_seed = core_seed(id, seed);
        let seed = core_seed;
        let (core, map) = match id {
            K_TINYMT64 => (Core::Tiny64(TinyMt64::new(seed, T64_MAT1, T64_MAT2, T64_TMAT)), Map::Id),
            K_WRK2 => (Core::Tiny64(TinyMt64::new(seed, 0, 0, 0)), Map::Id),
            _ if (K_MELG0..K_MELG0 + 7).contains(&id) => (Core::Melg(Melg::new(&MELG[(id - K_MELG0) as usize], seed)), Map::Id),
            _ if (K_SFMT0..K_SFMT0 + 10).contains(&id) => (Core::Sfmt(Sfmt::with(&sfmt::PARAMS[(id - K_SFMT0) as usize], seed as u32)), Map::Id),
            K_XSADD => (Core::Xsadd(xsadd_seed(seed as u32)), Map::Id),
            K_GZ => (mt(V_GZ, seed), Map::Id),
            K_MINECRAFT | K_OASIS07 | K_OASIS11 => (mt(V_MT19937, seed), Map::Id),
            K_OCTAVE => (mt(V_MT19937, seed), Map::Real53 { plus04: true }),
            K_RUBY => (mt(V_MT19937, seed), Map::Real53 { plus04: false }),
            K_RINDEX => (mt(V_R, seed), Map::RIndex(P::R_INDEX_DN)),
            K_CELLKEY => (mt(V_R, seed), Map::CellKey(P::CELLKEY_SCALE)),
            K_GLIB_DOUBLE => (mt(V_MT19937, seed), Map::GlibDouble),
            K_GLIB_RANGE => (mt(V_MT19937, seed), Map::GlibRange(P::GLIB_RANGE_DIST)),
            K_GIMP => {
                let mut core = Mt::new(variant_config(V_MT19937), seed);
                let mut perm = [0u8; P::GIMP_TABLE];
                for (i, p) in perm.iter_mut().enumerate() {
                    *p = i as u8;
                }
                for _ in 0..P::GIMP_SWAPS {
                    let j = glib_range(&mut core, P::GIMP_TABLE as u32) as usize;
                    let k = glib_range(&mut core, P::GIMP_TABLE as u32) as usize;
                    perm.swap(j, k);
                }
                (Core::Mt(core), Map::Perm64 { perm, idx: 0 })
            }
            K_SDCMICRO => (mt(V_MT19937, seed), Map::ExpRace { pool: P::SDC_POOL, order: [0; P::SDC_MAX_POOL], pos: 0, have: false }),
            K_BEAST2_M1 => (mt(V_KNUTH_NOAA, seed), Map::Id),
            K_PROFANITY => (mt(V_MT64, seed), Map::Id64),
            K_QISKIT => (mt(V_MT64, seed), Map::Id),
            K_QUACKLE => (mt(V_MT64, seed), Map::Reject64 { lo: P::QUACKLE_RANGE.0, hi: P::QUACKLE_RANGE.1 }),
            _ => return None,
        };
        Some(Kind { id, core, map, core_seed })
    }

    /// width of the native output in bits (64 for 64-bit cores, else 32)
    pub fn native_bits(&self) -> u32 {
        match (&self.core, &self.map) {
            (Core::Tiny64(_) | Core::Melg(_), _) => 64,
            (Core::Mt(m), Map::Id | Map::Id64) => m.cfg.w,
            _ => 32,
        }
    }
    /// dimension of the state as a GF(2) vector space (upper bound for the
    /// linear complexity of any output bit)
    pub fn state_bits(&self) -> usize {
        match &self.core {
            Core::Mt(m) => m.cfg.n * m.cfg.w as usize,
            Core::Tiny64(_) | Core::Xsadd(_) => 128,
            Core::Melg(m) => 64 * m.p.nn + 64,
            Core::Sfmt(s) => 128 * s.p.n,
        }
    }
    /// which output bit is a linear function of the state: bit 0 for the
    /// generators whose output adds two words (the carries make the top bit
    /// nonlinear), else the top bit
    pub fn lc_bit(&self) -> u32 {
        match &self.core {
            Core::Tiny64(_) | Core::Xsadd(_) => 0,
            _ => 1,
        }
    }
    /// SFMT emits a 128-bit block as four 32-bit words, so the word stream
    /// cycles through four different functionals of the state; only every
    /// fourth word (bit 127 of each block) obeys one linear recurrence
    pub fn lc_stride(&self) -> usize {
        match &self.core {
            Core::Sfmt(_) => 4,
            _ => 1,
        }
    }
    /// output range when it is not a full machine word (e.g. 10⁶ for an index)
    pub fn range(&self) -> Option<f64> {
        match &self.map {
            Map::RIndex(d) => Some(*d),
            Map::CellKey(s) => Some(*s + 1.0),
            Map::GlibRange(d) => Some(*d as f64),
            Map::Perm64 { .. } => Some(64.0),
            Map::ExpRace { pool, .. } => Some(*pool as f64),
            Map::Reject64 { lo, hi } => Some((hi - lo + 1) as f64),
            _ => None,
        }
    }

    fn core_next(&mut self) -> u64 {
        match &mut self.core {
            Core::Mt(m) => m.next(),
            Core::Tiny64(t) => t.next(),
            Core::Melg(m) => m.next(),
            Core::Sfmt(s) => s.next() as u64,
            Core::Xsadd(s) => {
                xsadd_next_state(s);
                s[3].wrapping_add(s[2]) as u64
            }
        }
    }

    /// what the C kind's native next (u64 for 64-bit kinds, else u32) returns
    pub fn next_native(&mut self) -> u64 {
        match self.map.clone() {
            Map::Id | Map::Id64 => self.core_next(),
            Map::Real53 { plus04 } => {
                let a = (self.core_next() as u32 >> P::REAL53_A_SHIFT) as f64;
                let b = (self.core_next() as u32 >> P::REAL53_B_SHIFT) as f64;
                let v = if plus04 {
                    (a * P::REAL53_A_SCALE + b + P::OCTAVE_OFFSET) * (1.0 / P::REAL53_DIV)
                } else {
                    (a * P::REAL53_A_SCALE + b) * (1.0 / P::REAL53_DIV)
                };
                let v = if v >= 1.0 { P::OCTAVE_MAX_BELOW_ONE } else { v };
                (v * 4_294_967_296.0) as u32 as u64
            }
            Map::RIndex(dn) => {
                let u = self.r_unif();
                fm::floor(dn * u) as u32 as u64
            }
            Map::CellKey(scale) => {
                let u = self.r_unif();
                (u * scale + 0.5) as u32 as u64
            }
            Map::GlibDouble => {
                const T: f64 = P::GLIB_DOUBLE_TRANSFORM;
                let v = loop {
                    let mut r = self.core_next() as u32 as f64 * T;
                    r = (r + self.core_next() as u32 as f64) * T;
                    if r < 1.0 {
                        break r;
                    }
                };
                (v * 4_294_967_296.0) as u32 as u64
            }
            Map::GlibRange(dist) => match &mut self.core {
                Core::Mt(m) => glib_range(m, dist) as u64,
                _ => 0,
            },
            Map::Perm64 { perm, idx } => {
                if let Map::Perm64 { idx: i, .. } = &mut self.map {
                    *i = (idx + 1) % P::GIMP_TABLE;
                }
                perm[idx] as u64
            }
            Map::ExpRace { pool, mut order, mut pos, have } => {
                if pos >= pool || !have {
                    let mut keys = [0f64; 64];
                    for k in keys.iter_mut().take(pool) {
                        let x0 = self.core_next() as u32 as f64;
                        let x1 = self.core_next() as u32 as f64;
                        let mut c = (x0 + x1 * 4_294_967_296.0) / 18_446_744_073_709_551_616.0;
                        if c >= 1.0 {
                            c = f64::from_bits(0x3fef_ffff_ffff_ffff);
                        }
                        *k = 1.0 / -srs_log(1.0 - c);
                    }
                    for (i, o) in order.iter_mut().enumerate().take(pool) {
                        *o = i as u8;
                    }
                    for i in 0..pool - 1 {
                        let mut best = i;
                        for j in i + 1..pool {
                            if keys[order[j] as usize] > keys[order[best] as usize] {
                                best = j;
                            }
                        }
                        order.swap(i, best);
                    }
                    pos = 0;
                }
                let v = order[pos] as u64;
                self.map = Map::ExpRace { pool, order, pos: pos + 1, have: true };
                v
            }
            Map::Reject64 { lo, hi } => {
                let range = hi - lo + 1;
                let limit = u64::MAX - (u64::MAX % range);
                loop {
                    let d = self.core_next();
                    if d < limit {
                        break (lo + d % range) as u32 as u64;
                    }
                }
            }
        }
    }

    /// R's unif_rand() with its fixup() clamp into (0, 1)
    fn r_unif(&mut self) -> f64 {
        let x = self.core_next() as u32 as f64 * P::R_2POW_M32;
        const I2_32M1: f64 = P::R_I2_32M1;
        if x <= 0.0 {
            0.5 * I2_32M1
        } else if 1.0 - x <= 0.0 {
            1.0 - 0.5 * I2_32M1
        } else {
            x
        }
    }

    /// the core generator's configuration when it is an MT (for the debugger)
    pub fn mt_config(&self) -> Option<MtConfig> {
        match &self.core {
            Core::Mt(m) => Some(m.cfg),
            _ => None,
        }
    }
}

pub fn minecraft_seed(cx: u32, cz: u32) -> u64 {
    (cx.wrapping_mul(P::MINECRAFT_MUL) ^ cz) as u64
}
pub fn oasis_seed(seed: u64, group: u64, event: u64) -> u64 {
    const MOD: u64 = P::OASIS_MOD;
    let s1 = group.wrapping_mul(P::OASIS_GROUP_MUL) % MOD;
    let s2 = event.wrapping_mul(P::OASIS_EVENT_MUL) % MOD;
    (s1.wrapping_add(s2).wrapping_add(seed) % MOD) as u32 as u64
}

/// GLib ≥ 2.2 g_rand_int_range(0, dist) for dist ≤ 2³¹
pub fn glib_range(m: &mut Mt, dist: u32) -> u32 {
    if dist == 0 {
        return 0;
    }
    let maxvalue = if dist <= 0x8000_0000 {
        let mut left = (0x8000_0000u32 % dist) * 2;
        if left >= dist {
            left -= dist;
        }
        0xffff_ffffu32 - left
    } else {
        dist - 1
    };
    loop {
        let r = m.next() as u32;
        if r <= maxvalue {
            return r % dist;
        }
    }
}

pub fn boxed(id: u32, seed: u64) -> Option<Box<Kind>> {
    Kind::new(id, seed).map(Box::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mt::SeedMode;
    use crate::sources::{V_COKUS2, V_GNUGO};
    use alloc::vec::Vec;

    fn gen_for(name: &str, seed: u64) -> Option<Box<dyn FnMut() -> u64>> {
        let melg = [607u32, 1279, 2281, 4253, 11213, 19937, 44497];
        let sfmt_sizes: Vec<u32> = sfmt::PARAMS.iter().map(|p| p.mexp).collect();
        let id = match name {
            "tinymt64" => K_TINYMT64,
            "wrk2" => K_WRK2,
            "xsadd" => K_XSADD,
            "gz" => K_GZ,
            "minecraft" => K_MINECRAFT,
            "oasis07" => K_OASIS07,
            "oasis11" => K_OASIS11,
            "octave" => K_OCTAVE,
            "ruby" => K_RUBY,
            "r_index" => K_RINDEX,
            "cellkey" => K_CELLKEY,
            "glib_double" => K_GLIB_DOUBLE,
            "glib_range" => K_GLIB_RANGE,
            "gimp" => K_GIMP,
            "sdcmicro" => K_SDCMICRO,
            "beast2_m1" => K_BEAST2_M1,
            "profanity" => K_PROFANITY,
            "qiskit" => K_QISKIT,
            "quackle" => K_QUACKLE,
            "gnugo" | "seeded_genrand" | "seeded_cokus2" => {
                let c = match name {
                    "gnugo" => variant_config(V_GNUGO),
                    "seeded_cokus2" => variant_config(V_COKUS2),
                    _ => variant_config(V_MT19937),
                };
                let mut m = Mt::new(c, seed);
                return Some(Box::new(move || m.next()));
            }
            _ if name.starts_with("melg") => {
                let p: u32 = name[4..].parse().ok()?;
                K_MELG0 + melg.iter().position(|&q| q == p)? as u32
            }
            _ if name.starts_with("sfmt") && name.len() > 4 => {
                let p: u32 = name[4..].parse().ok()?;
                K_SFMT0 + sfmt_sizes.iter().position(|&q| q == p)? as u32
            }
            _ => return None,
        };
        let mut k = Kind::new(id, seed).unwrap();
        Some(Box::new(move || k.next_native()))
    }

    #[test]
    fn matches_repo_c_kinds() {
        let txt = include_str!("../../tools/cref/ref.txt");
        let mut checked = 0;
        let mut seen = alloc::collections::BTreeSet::new();
        for line in txt.lines() {
            let (head, rest) = line.split_once(':').unwrap();
            let mut hp = head.split(' ');
            let name = hp.next().unwrap();
            let seed: u64 = hp.next().unwrap().parse().unwrap();
            let Some(mut g) = gen_for(name, seed) else { continue };
            let (firsts, thousandth) = rest.split_once('|').unwrap();
            let want: Vec<u64> = firsts.split_whitespace().map(|x| x.parse().unwrap()).collect();
            let want1000: u64 = thousandth.trim().parse().unwrap();
            let got: Vec<u64> = (0..1000).map(|_| g()).collect();
            assert_eq!(&got[..6], &want[..], "{name} seed {seed}");
            assert_eq!(got[999], want1000, "{name} seed {seed} #1000");
            checked += 1;
            seen.insert(name);
        }
        assert_eq!(seen.len(), 38, "{seen:?}");
        assert_eq!(checked, 38 * 5);
        let _ = SeedMode::GnuGo;
    }

    #[test]
    fn official_vectors() {
        // tinymt64 check64.out.txt (seed 1)
        let mut t = TinyMt64::new(1, T64_MAT1, T64_MAT2, T64_TMAT);
        assert_eq!([t.next(), t.next(), t.next()], [15503804787016557143, 17280942441431881838, 2177846447079362065]);
    }
}

#[cfg(test)]
mod lc_survey {
    use super::*;
    use crate::gf2::BmJob;
    use crate::sources::Source;
    extern crate std;
    #[test]
    #[ignore]
    fn survey() {
        let mut ids: Vec<u32> = alloc::vec![K_TINYMT64, K_WRK2, K_XSADD];
        for i in 0..7 { ids.push(K_MELG0 + i); }
        for i in 0..10 { ids.push(K_SFMT0 + i); }
        for id in K_GZ..=K_QUACKLE { ids.push(id); }
        for id in ids {
            let k = Kind::new(id, 5489).unwrap();
            let (sb, bit) = (k.state_bits(), k.lc_bit());
            let t0 = std::time::Instant::now();
            let mut s = Source::new_special(id, 5489).unwrap();
            let nbits = 2 * sb + 128;
            let mut seq = alloc::vec![0u64; nbits / 64 + 1];
            let st = k.lc_stride();
            for i in 0..nbits { let mut vb = (0, 1); for _ in 0..st { vb = s.next_raw(); } let (v, b) = vb; let sh = if bit == 0 { 0 } else { b - 1 }; if (v >> sh) & 1 == 1 { seq[i / 64] |= 1 << (i % 64); } }
            let mut j = BmJob::new(&seq, nbits);
            j.step(usize::MAX);
            std::println!("{id} state={sb} bit={bit} L={} nbits={nbits} {:?}", j.l, t0.elapsed());
        }
    }
}

