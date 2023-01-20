//! every constant in the crate, in one file. the other modules handle *how*
//! a generator works; *which* numbers it uses is all down here.
//!
//! everything gets checked bit-for-bit by cargo test against the prngs C code
//! (tools/cref/ref.txt) or the authors' published vectors, so if a number is
//! wrong something will blow up.
//!
//! rough layout: MT presets → seeding → port quirks → tinymt → sfmt → melg
//! → xsadd → wrapper kinds → classic non-MT stuff → mersenne exponents.

// ============================================================================
//  1. MT presets
// ============================================================================

/// the 13 numbers from Matsumoto & Nishimura 1998 that pin down an MT.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MtParams {
    /// word size in bits
    pub w: u32,
    /// degree of recurrence = number of state words
    pub n: usize,
    /// middle word offset, 1 ≤ m < n
    pub m: usize,
    /// separation point: x_k keeps its top w−r bits, x_{k+1} its low r bits
    pub r: u32,
    /// last row of the twist matrix A
    pub a: u64,
    /// tempering: y ^= (y >> u) & d
    pub u: u32,
    pub d: u64,
    /// tempering: y ^= (y << s) & b
    pub s: u32,
    pub b: u64,
    /// tempering: y ^= (y << t) & c
    pub t: u32,
    pub c: u64,
    /// tempering: y ^= y >> l
    pub l: u32,
    /// init_genrand multiplier
    pub f: u64,
}

/// mt19937ar.c (2002) / std::mt19937
pub const MT19937: MtParams = MtParams {
    w: 32, n: 624, m: 397, r: 31, a: 0x9908_b0df,
    u: 11, d: 0xffff_ffff, s: 7, b: 0x9d2c_5680, t: 15, c: 0xefc6_0000, l: 18,
    f: INIT_GENRAND_F32,
};

/// mt19937-64.c (2004) / std::mt19937_64
pub const MT19937_64: MtParams = MtParams {
    w: 64, n: 312, m: 156, r: 31, a: 0xB502_6F5A_A966_19E9,
    u: 29, d: 0x5555_5555_5555_5555, s: 17, b: 0x71D6_7FFF_EDA6_0000, t: 37, c: 0xFFF7_EEE0_0000_0000, l: 43,
    f: INIT_GENRAND_F64,
};

/// boost::random::mt11213b
pub const MT11213B: MtParams = MtParams {
    w: 32, n: 351, m: 175, r: 19, a: 0xccab_8ee7,
    u: 11, d: 0xffff_ffff, s: 7, b: 0x31b6_ab00, t: 15, c: 0xffe5_0000, l: 17,
    f: INIT_GENRAND_F32,
};

/// tt800 (matsumoto & kurita 1994). r=0 means the whole word participates in
/// the twist (no upper/lower split); u=w effectively disables the first temper.
pub const TT800: MtParams = MtParams {
    w: 32, n: 25, m: 7, r: 0, a: 0x8ebf_d028,
    u: 32, d: 0xffff_ffff, s: 7, b: 0x2b5b_2500, t: 15, c: 0xdb8b_0000, l: 16,
    f: INIT_GENRAND_F32,
};

/// GNU Go's TT800: identical except the final y ^= y >> 16 is missing (l = w)
pub const TT800_GNUGO_L: u32 = 32;

// ============================================================================
//  2. seeding
// ============================================================================

/// init_genrand: x_i = f·(x_{i−1} ⊕ (x_{i−1} ≫ (w−2))) + i
pub const INIT_GENRAND_F32: u64 = 1_812_433_253;
pub const INIT_GENRAND_F64: u64 = 6_364_136_223_846_793_005;

/// init_by_array: first init_genrand(19650218), then two mixing passes
pub const INIT_BY_ARRAY_SEED: u64 = 19_650_218;
pub const INIT_BY_ARRAY32_MUL1: u64 = 1_664_525;
pub const INIT_BY_ARRAY32_MUL2: u64 = 1_566_083_941;
pub const INIT_BY_ARRAY64_MUL1: u64 = 3_935_559_000_370_003_845;
pub const INIT_BY_ARRAY64_MUL2: u64 = 2_862_933_555_777_941_757;

/// 1998 original / Knuth TAOCP vol. 2: x_i = 69069·x_{i−1} (also Cokus' x|1 variant)
pub const KNUTH_MUL: u64 = 69_069;

/// R set.seed: x ← 69069x + 1, run this many times before filling the state
pub const R_LCG_MUL: u64 = 69_069;
pub const R_LCG_ADD: u64 = 1;
pub const R_LCG_WARMUP: usize = 51;

/// GNU Go gg_srand: s_{i+1} = 1313·s_i + 88897
pub const GNUGO_LCG_MUL: u64 = 1313;
pub const GNUGO_LCG_ADD: u64 = 88_897;

/// std::seed_seq::generate (C++ [rand.util.seedseq])
pub const SEED_SEQ_FILL: u32 = 0x8b8b_8b8b;
pub const SEED_SEQ_R1: u32 = 1_371_501_266;
pub const SEED_SEQ_MUL1: u32 = 1_664_525;
pub const SEED_SEQ_MUL2: u32 = 1_566_083_941;

// ============================================================================
//  3. quirks of catalogued MT ports (see sources::variant_config)
// ============================================================================

/// the 1998 code and its ports replace seed 0 by 4357
pub const KNUTH_ZERO_SEED: u64 = 4357;
/// GLib 2.0 GRand replaces seed 0 by this
pub const GLIB20_ZERO_SEED: u64 = 0x6b84_2128;
/// PHP / MyBB return z ≫ 1, i.e. 31 bits
pub const PHP_OUT_SHIFT: u32 = 1;
pub const PHP_OUT_AND: u64 = 0x7fff_ffff;
/// PPSSPP sceMt19937: they set lmask = umask by accident, so the twist only uses the top bit of x[k+1]
pub const PPSSPP_LMASK: u64 = 0x8000_0000;
/// MyBB my_rand: (z ≫ 1) ⊕ mask
pub const MYBB_OUT_XOR: u64 = 0x1f2e_3d4c;
/// ECJ primeGenerator: throw away 2·624 + 1 outputs
pub const ECJ_DISCARD: u32 = 2 * 624 + 1;
/// Stim: seed ⊕ this before mt19937_64  (stole the constant from their source, felt appropriate)
pub const STIM_SEED_XOR: u64 = 0xDEAD_BEEF_1239;
/// Apery: clears the low bit of every Zobrist key
pub const APERY_OUT_AND: u64 = !1u64;

// ============================================================================
//  4. TinyMT (Saito & Matsumoto 2011)
// ============================================================================

/// tinymt32: the RFC 8682 parameter set
pub const TINYMT32_MAT1: u32 = 0x8f70_11ee;
pub const TINYMT32_MAT2: u32 = 0xfc78_ff1f;
pub const TINYMT32_TMAT: u32 = 0x3793_fdff;
pub const TINYMT32_MASK: u32 = 0x7fff_ffff;
pub const TINYMT32_SH0: u32 = 1;
pub const TINYMT32_SH1: u32 = 10;
pub const TINYMT32_SH8: u32 = 8;
/// seeding: MIN_LOOP mixing rounds, then PRE_LOOP warm-up steps
pub const TINYMT32_MIN_LOOP: u32 = 8;
pub const TINYMT32_PRE_LOOP: u32 = 8;

/// tinymt64: the parameter set of the reference check64.c
pub const TINYMT64_MAT1: u32 = 0xfa05_1f40;
pub const TINYMT64_MAT2: u32 = 0xffd0_fff4;
pub const TINYMT64_TMAT: u64 = 0x58d0_2ffe_ffbf_ffbc;
pub const TINYMT64_MASK: u64 = 0x7fff_ffff_ffff_ffff;
pub const TINYMT64_SH0: u32 = 12;
pub const TINYMT64_SH1: u32 = 11;
pub const TINYMT64_SH8: u32 = 8;
pub const TINYMT64_MIN_LOOP: u64 = 8;

/// both TinyMTs fall back to these if seeding produced the all-zero state
pub const TINYMT32_ZERO_FIX: [u32; 4] = [b'T' as u32, b'I' as u32, b'N' as u32, b'Y' as u32];
pub const TINYMT64_ZERO_FIX: [u64; 2] = [b'T' as u64, b'M' as u64];

// ============================================================================
//  5. SFMT (Saito & Matsumoto 2006), SFMT-params<MEXP>.h
// ============================================================================

#[derive(Debug, PartialEq)]
pub struct SfmtParams {
    /// Mersenne exponent: the period is a multiple of 2^MEXP − 1
    pub mexp: u32,
    /// number of 128-bit blocks
    pub n: usize,
    /// offset of the second block in the recursion
    pub pos1: usize,
    /// d ≪₃₂ SL1
    pub sl1: u32,
    /// a ≪₁₂₈ (8·SL2)
    pub sl2: u32,
    /// (b ≫₃₂ SR1) ∧ MSK
    pub sr1: u32,
    /// c ≫₁₂₈ (8·SR2)
    pub sr2: u32,
    pub msk: [u32; 4],
    /// period certification vector
    pub parity: [u32; 4],
}

const fn sp(mexp: u32, n: usize, pos1: usize, sl1: u32, sl2: u32, sr1: u32, sr2: u32, msk: [u32; 4], parity: [u32; 4]) -> SfmtParams {
    SfmtParams { mexp, n, pos1, sl1, sl2, sr1, sr2, msk, parity }
}

/// in order of MEXP; index 5 is SFMT19937
pub static SFMT: [SfmtParams; 10] = [
    sp(607, 5, 2, 15, 3, 13, 3, [0xfdff37ff, 0xef7f3f7d, 0xff777b7d, 0x7ff7fb2f], [0x1, 0x0, 0x0, 0x5986f054]),
    sp(1279, 10, 7, 14, 3, 5, 1, [0xf7fefffd, 0x7fefcfff, 0xaff3ef3f, 0xb5ffff7f], [0x1, 0x0, 0x0, 0x20000000]),
    sp(2281, 18, 12, 19, 1, 5, 1, [0xbff7ffbf, 0xfdfffffe, 0xf7ffef7f, 0xf2f7cbbf], [0x1, 0x0, 0x0, 0x41dfa600]),
    sp(4253, 34, 17, 20, 1, 7, 1, [0x9f7bffff, 0x9fffff5f, 0x3efffffb, 0xfffff7bb], [0xa8000001, 0xaf5390a3, 0xb740b3f8, 0x6c11486d]),
    sp(11213, 88, 68, 14, 3, 7, 3, [0xeffff7fb, 0xffffffef, 0xdfdfbfff, 0x7fffdbfd], [0x1, 0x0, 0xe8148000, 0xd0c7afa3]),
    sp(19937, 156, 122, 18, 1, 11, 1, [0xdfffffef, 0xddfecb7f, 0xbffaffff, 0xbffffff6], [0x1, 0x0, 0x0, 0x13c9e684]),
    sp(44497, 348, 330, 5, 3, 9, 3, [0xeffffffb, 0xdfbebfff, 0xbfbf7bef, 0x9ffd7bff], [0x1, 0x0, 0xa3ac4000, 0xecc1327a]),
    sp(86243, 674, 366, 6, 7, 19, 1, [0xfdbffbff, 0xbff7ff3f, 0xfd77efff, 0xbf9ff3ff], [0x1, 0x0, 0x0, 0xe9528d85]),
    sp(132049, 1032, 110, 19, 1, 21, 1, [0xffffbb5f, 0xfb6ebf95, 0xfffefffa, 0xcff77fff], [0x1, 0x0, 0xcb520000, 0xc7e91c7d]),
    sp(216091, 1689, 627, 11, 3, 10, 1, [0xbff7bff7, 0xbfffffff, 0xbffffa7f, 0xffddfbfb], [0xf8000001, 0x89e80709, 0x3bd2b64b, 0x0c64b1e4]),
];
pub const SFMT19937_INDEX: usize = 5;

// ============================================================================
//  6. MELG (Harase & Kimoto 2018), melg<MEXP>-64.c
// ============================================================================

#[derive(Debug, PartialEq)]
pub struct MelgParams {
    pub mexp: u32,
    /// number of 64-bit state words (plus one more, the lung)
    pub nn: usize,
    /// middle offset
    pub mm: usize,
    /// last row of the twist matrix
    pub a: u64,
    /// x = (x_i ∧ upper P bits) | (x_{i+1} ∧ lower 64−P bits)
    pub p: u32,
    /// output: z = x_i ⊕ (x_i ≪ SHIFT1) ⊕ (x_{i+LAG1} ∧ MASK1)
    pub lag1: usize,
    pub shift1: u32,
    pub mask1: u64,
    /// lung update ℓ ⊕ (ℓ ≪ NEG), state update ℓ ⊕ (ℓ ≫ POS)
    pub neg: u32,
    pub pos: u32,
}

const fn mp(mexp: u32, nn: usize, mm: usize, a: u64, p: u32, lag1: usize, shift1: u32, mask1: u64, neg: u32, pos: u32) -> MelgParams {
    MelgParams { mexp, nn, mm, a, p, lag1, shift1, mask1, neg, pos }
}

/// in order of MEXP; index 5 is MELG19937-64
pub static MELG: [MelgParams; 7] = [
    mp(607, 9, 5, 0x81f1_fd68_0123_48bc, 31, 3, 30, 0x66ed_c62a_6bf8_c826, 13, 35),
    mp(1279, 19, 7, 0x1afe_fd15_26d3_952b, 63, 5, 6, 0x3a23_d78e_8fb5_e349, 22, 37),
    mp(2281, 35, 17, 0x7cbe_23eb_ca8a_6d36, 41, 6, 6, 0xe4e2_242b_6e15_aebe, 36, 21),
    mp(4253, 66, 29, 0xfac1_e8c5_6471_d722, 29, 9, 5, 0xcb67_b0c1_8fe1_4f4d, 30, 20),
    mp(11213, 175, 45, 0xddbc_d6e5_25e1_c757, 13, 4, 5, 0xbd2d_1251_e589_593f, 33, 13),
    mp(19937, 311, 81, 0x5c32_e06d_f730_fc42, 33, 19, 16, 0x6aed_e6fd_97b3_38ec, 23, 33),
    mp(44497, 695, 373, 0x4fa9_ca36_f293_c9a9, 17, 95, 6, 0x06fb_bee2_9aae_fd91, 37, 14),
];
impl MelgParams {
    /// mask of the upper P bits
    pub fn masku(&self) -> u64 {
        !0u64 << (64 - self.p)
    }
}
pub const MELG_SEED_F: u64 = INIT_GENRAND_F64;
pub const MELG19937_INDEX: usize = 5;

// ============================================================================
//  7. XSadd (Saito & Matsumoto 2014)
// ============================================================================

pub const XSADD_SH1: u32 = 15;
pub const XSADD_SH2: u32 = 18;
pub const XSADD_SH3: u32 = 11;
pub const XSADD_LOOP: u32 = 8;
pub const XSADD_SEED_F: u32 = INIT_GENRAND_F32 as u32;
pub const XSADD_ZERO_FIX: [u32; 4] = [b'X' as u32, b'S' as u32, b'A' as u32, b'D' as u32];

// ============================================================================
//  8. wrapper kinds: what they do to the seed and to the output
// ============================================================================

/// Minecraft Bedrock slime chunks: seed = (cx · 0x1f1f1f1f) ⊕ cz
pub const MINECRAFT_MUL: u32 = 0x1f1f_1f1f;
/// the chunk the catalog entry uses
pub const MINECRAFT_CHUNK: (u32, u32) = (3, 5);

/// OasisLMF gulcalc: seed' = (g·G mod 2³¹ + e·E mod 2³¹ + seed) mod 2³¹
pub const OASIS_MOD: u64 = 1 << 31;
pub const OASIS_GROUP_MUL: u64 = 1_543_270_363;
pub const OASIS_EVENT_MUL: u64 = 1_943_272_559;
/// (group, event) of the two catalog entries
pub const OASIS_GE_07: (u64, u64) = (0, 7);
pub const OASIS_GE_11: (u64, u64) = (1, 1);

/// BEAST 2 Randomizer.setSeed: seed − 1, except seed 1 → 4357
pub const BEAST2_GOOD_SEED: u64 = 4357;

/// Ruby / Octave: (a ≫ 5, b ≫ 6) → (a·2²⁶ + b [+ 0.4]) / 2⁵³
pub const REAL53_A_SHIFT: u32 = 5;
pub const REAL53_B_SHIFT: u32 = 6;
pub const REAL53_A_SCALE: f64 = 67_108_864.0; // 2^26
pub const REAL53_DIV: f64 = 9_007_199_254_740_992.0; // 2^53
pub const OCTAVE_OFFSET: f64 = 0.4;
/// Octave clamps 1.0 to the largest double below it
pub const OCTAVE_MAX_BELOW_ONE: f64 = 0.999_999_999_999_999_9;

/// R: unif_rand = fixup(z · 2⁻³²), fixup keeps the value strictly inside (0, 1)
pub const R_2POW_M32: f64 = 2.328_306_436_538_696_3e-10;
pub const R_I2_32M1: f64 = 2.328_306_437_080_797e-10;
/// R ≤ 3.5 sample(): ⌊dn · u⌋, the catalog uses dn = 10⁶
pub const R_INDEX_DN: f64 = 1_000_000.0;
/// cellKey record keys: ⌊u · 10^d + ½⌋ with d = 8
pub const CELLKEY_SCALE: f64 = 1e8;

/// GLib g_rand_double: (a/2³² + b)/2³²
pub const GLIB_DOUBLE_TRANSFORM: f64 = 2.328_306_436_538_696_289_062_5e-10;
/// GLib g_rand_int_range: the catalog's range
pub const GLIB_RANGE_DIST: u32 = 1_000_000;

/// GIMP solid noise: 32 swaps in a 64-entry table
pub const GIMP_TABLE: usize = 64;
pub const GIMP_SWAPS: usize = 32;

/// sdcMicro record swapping: size of the exponential race
pub const SDC_POOL: usize = 16;
/// largest pool the C code accepts (its buffer size)
pub const SDC_MAX_POOL: usize = 64;
/// srs_log: ln 2 split in two for accuracy, and √2
pub const SRS_LN2_HI: f64 = 6.931_471_803_691_238_164_90e-01;
pub const SRS_LN2_LO: f64 = 1.908_214_929_270_587_700_02e-10;
pub const SRS_SQRT2: f64 = 1.414_213_562_373_095_1;

/// Quackle tile draws: rejection sampling into [lo, hi]
pub const QUACKLE_RANGE: (u64, u64) = (0, 99);

// ============================================================================
//  9. classic generators for contrast (statistics section)
// ============================================================================

/// IBM RANDU: x ← 65539·x mod 2³¹
pub const RANDU_A: u64 = 65_539;
pub const RANDU_MASK: u64 = 0x7fff_ffff;
/// Park–Miller minimal standard: x ← 16807·x mod (2³¹ − 1)
pub const MINSTD_A: u64 = 16_807;
pub const MINSTD_M: u64 = 0x7fff_ffff;
/// Numerical Recipes LCG: x ← 1664525·x + 1013904223 mod 2³²
pub const LCG32_A: u32 = 1_664_525;
pub const LCG32_C: u32 = 1_013_904_223;
/// Marsaglia xorshift32 (13, 17, 5); seed 0 would be stuck, so it gets this
pub const XORSHIFT32_SHIFTS: (u32, u32, u32) = (13, 17, 5);
pub const XORSHIFT32_ZERO_SEED: u32 = 2_463_534_242;

// ============================================================================
// 10. Mersenne exponents
// ============================================================================

/// p with 2^p − 1 prime, up to the largest SFMT size
pub const MERSENNE_EXPONENTS: [u32; 31] = [
    2, 3, 5, 7, 13, 17, 19, 31, 61, 89, 107, 127, 521, 607, 1279, 2203, 2281, 3217, 4253, 4423, 9689, 9941, 11213, 19937, 21701,
    23209, 44497, 86243, 110503, 132049, 216091,
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tables_are_consistent() {
        for p in SFMT.iter() {
            assert!(MERSENNE_EXPONENTS.contains(&p.mexp));
            assert!(128 * p.n as u32 >= p.mexp && p.pos1 < p.n);
        }
        for p in MELG.iter() {
            assert!(MERSENNE_EXPONENTS.contains(&p.mexp));
            // state: NN words minus the 64−P bits that are never used, plus the lung
            assert_eq!(64 * p.nn as u32 - (64 - p.p) + 64, p.mexp);
        }
        for q in [MT19937, MT19937_64, MT11213B, TT800] {
            let p = q.n as u32 * q.w - q.r;
            // TT800 is the exception: 2^800 − 1 is not prime, its φ was checked
            // primitive by factoring 2^800 − 1 instead
            assert!(MERSENNE_EXPONENTS.contains(&p) || p == 800, "{p}");
            assert_eq!(q.a >> (q.w - 1), 1, "A must have its top bit set");
        }
