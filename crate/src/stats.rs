//! the distribution test stuff. chi2 histograms, box-muller, clt vs
//! irwin-hall, autocorrelation, mc pi, bit bias — all run against a Source.

use crate::fm;
use crate::mt::{Mt, MtConfig, basis_state};
use crate::sources::Source;
use alloc::vec;
use alloc::vec::Vec;

pub struct Hist {
    pub counts: Vec<u32>,
    pub chi2: f64,
    pub dof: f64,
    pub p: f64,
}

fn chi2_of(counts: &[u32], expected: &[f64]) -> (f64, f64, f64) {
    let mut chi2 = 0.0;
    let mut dof = -1.0;
    for (o, e) in counts.iter().zip(expected) {
        if *e > 0.0 {
            let d = *o as f64 - e;
            chi2 += d * d / e;
            dof += 1.0;
        }
    }
    let p = if dof > 0.0 { fm::gamma_q(dof / 2.0, chi2 / 2.0) } else { 1.0 };
    (chi2, dof, p)
}

pub fn uniform_hist(src: &mut Source, n: u32, bins: u32) -> Hist {
    let mut counts = vec![0u32; bins as usize];
    for _ in 0..n {
        let u = src.next_unit();
        let b = ((u * bins as f64) as usize).min(bins as usize - 1);
        counts[b] += 1;
    }
    let e = vec![n as f64 / bins as f64; bins as usize];
    let (chi2, dof, p) = chi2_of(&counts, &e);
    Hist { counts, chi2, dof, p }
}

// standard Box-Muller transform; reject u1=0 to avoid ln(0).
// a single retry is enough — getting 0.0 twice in a row from a 32-bit generator
// is basically impossible.
pub fn box_muller(src: &mut Source) -> (f64, f64) {
    let mut u1 = src.next_unit();
    if u1 <= 0.0 {
        u1 = src.next_unit();
    }
    let u2 = src.next_unit();
    let r = fm::sqrt(-2.0 * fm::ln(u1));
    let (s, c) = fm::sincos(2.0 * fm::PI * u2);
    (r * c, r * s)
}

pub fn normal_hist(src: &mut Source, n: u32, bins: u32, lo: f64, hi: f64) -> Hist {
    let mut counts = vec![0u32; bins as usize];
    let width = (hi - lo) / bins as f64;
    let put = |z: f64, counts: &mut Vec<u32>| {
        if z >= lo && z < hi {
            counts[((z - lo) / width) as usize] += 1;
        }
    };
    let mut i = 0;
    while i < n {
        let (a, b) = box_muller(src);
        put(a, &mut counts);
        if i + 1 < n {
            put(b, &mut counts);
        }
        i += 2;
    }
    let e: Vec<f64> = (0..bins).map(|k| n as f64 * (fm::phi_cdf(lo + (k + 1) as f64 * width) - fm::phi_cdf(lo + k as f64 * width))).collect();
    // merge sparse tail bins so expected count stays  >= 5 (chi2 requirement)
    // FIXME: this is a bit ad-hoc, would be cleaner to pick bin count adaptively
    let mut oc: Vec<u32> = Vec::new();
    let mut ec: Vec<f64> = Vec::new();
    let (mut ao, mut ae) = (0u32, 0.0);
    for k in 0..bins as usize {
        ao += counts[k];
        ae += e[k];
        if ae >= 5.0 {
            oc.push(ao);
            ec.push(ae);
            ao = 0;
            ae = 0.0;
        }
    }
    if let (Some(lo_), Some(le)) = (oc.last_mut(), ec.last_mut()) {
        *lo_ += ao;
        *le += ae;
    }
    let (chi2, dof, p) = chi2_of(&oc, &ec);
    Hist { counts, chi2, dof, p }
}

// histogram of the sum of k uniforms, scaled to [0,1] for the CLT plot
pub fn sum_hist(src: &mut Source, k: u32, n: u32, bins: u32) -> Hist {
    let mut counts = vec![0u32; bins as usize];
    for _ in 0..n {
        let mut s = 0.0;
        for _ in 0..k {
            s += src.next_unit();
        }
        let b = ((s / k as f64 * bins as f64) as usize).min(bins as usize - 1);
        counts[b] += 1;
    }
    // expected from the Irwin–Hall CDF
    let e: Vec<f64> = (0..bins)
        .map(|b| {
            let a = b as f64 / bins as f64 * k as f64;
            let c = (b + 1) as f64 / bins as f64 * k as f64;
            n as f64 * (irwin_hall_cdf(k, c) - irwin_hall_cdf(k, a))
        })
        .collect();
    let (chi2, dof, p) = chi2_of(&counts, &e);
    Hist { counts, chi2, dof, p }
}

fn binom(n: u32, k: u32) -> f64 {
    let mut r = 1.0;
    for i in 0..k {
        r *= (n - i) as f64 / (i + 1) as f64;
    }
    r
}

/// f(x) = 1/(k−1)! · Σ_{j=0}^{⌊x⌋} (−1)^j C(k,j) (x−j)^{k−1}
pub fn irwin_hall_pdf(k: u32, x: f64) -> f64 {
    if x < 0.0 || x > k as f64 {
        return 0.0;
    }
    let mut fact = 1.0;
    for i in 1..k {
        fact *= i as f64;
    }
    let mut s = 0.0;
    let jmax = fm::floor(x) as u32;
    for j in 0..=jmax.min(k) {
        let sign = if j % 2 == 0 { 1.0 } else { -1.0 };
        let mut pw = 1.0;
        for _ in 0..k - 1 {
            pw *= x - j as f64;
        }
        s += sign * binom(k, j) * pw;
    }
    s / fact
}

pub fn irwin_hall_cdf(k: u32, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= k as f64 {
        return 1.0;
    }
    let mut fact = 1.0;
    for i in 1..=k {
        fact *= i as f64;
    }
    let mut s = 0.0;
    let jmax = fm::floor(x) as u32;
    for j in 0..=jmax.min(k) {
        let sign = if j % 2 == 0 { 1.0 } else { -1.0 };
        let mut pw = 1.0;
        for _ in 0..k {
            pw *= x - j as f64;
        }
        s += sign * binom(k, j) * pw;
    }
    s / fact
}

/// sample autocorrelation r_h of U_i − ½ for h = 1..=maxlag
pub fn autocorr(src: &mut Source, n: u32, maxlag: u32) -> Vec<f64> {
    let xs: Vec<f64> = (0..n).map(|_| src.next_unit() - 0.5).collect();
    let var: f64 = xs.iter().map(|x| x * x).sum::<f64>() / n as f64;
    (1..=maxlag)
        .map(|h| {
            let h = h as usize;
            let mut s = 0.0;
            for i in 0..xs.len() - h {
                s += xs[i] * xs[i + h];
            }
            s / (n as f64 - h as f64) / var
        })
        .collect()
}

/// π estimates 4·(hits/N) at log-spaced checkpoints; returns (N, estimate) pairs
pub fn mc_pi(src: &mut Source, n: u32, points: u32) -> Vec<f64> {
    let mut out = Vec::new();
    let mut hits = 0u64;
    let mut next_cp = 10.0f64;
    // log-spaced checkpoints: i could just hardcode the breakpoints but this is less magic
    let ratio = fm::exp(fm::ln(n as f64 / 10.0) / points as f64);
    for i in 1..=n as u64 {
        let x = src.next_unit();
        let y = src.next_unit();
        if x * x + y * y < 1.0 {
            hits += 1;
        }
        if i as f64 >= next_cp || i == n as u64 {
            out.push(i as f64);
            out.push(4.0 * hits as f64 / i as f64);
            while next_cp <= i as f64 {
                next_cp *= ratio;
            }
        }
    }
    out
}

/// fraction of 1-bits at each of the 32 positions, per block of `per_row` outputs
pub fn bit_bias(src: &mut Source, per_row: u32, rows: u32) -> Vec<f64> {
    let mut out = Vec::with_capacity((rows * 32) as usize);
    for _ in 0..rows {
        let mut cnt = [0u32; 32];
        for _ in 0..per_row {
            let v = src.next_u32();
            for b in 0..32 {
                cnt[b] += (v >> (31 - b)) & 1;
            }
        }
        for c in cnt {
            out.push(c as f64 / per_row as f64);
        }
    }
    out
}

/// zero-excess recovery: start from a state with a single 1-bit and record
/// the fraction of 1-bits in the whole state after every twist
pub fn zero_excess(cfg: &MtConfig, twists: u32) -> Vec<f64> {
    let p = cfg.p();
    let mut g: Mt = basis_state(cfg, p - 1); // top bit of the last word
    let total = (cfg.n as u32 * cfg.w) as f64;
    let mut out = Vec::with_capacity(twists as usize + 1);
    let ones = |g: &Mt| g.x.iter().map(|v| v.count_ones()).sum::<u32>() as f64;
    out.push(ones(&g) / total);
    for _ in 0..twists {
        g.twist();
        out.push(ones(&g) / total);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mt::{Mt, MtConfig};
    #[test]
    fn uniform_ok() {
        let mut s = Source::Mt(Mt::new(MtConfig::mt19937(), 1));
        let h = uniform_hist(&mut s, 100_000, 50);
        assert!(h.p > 1e-4 && h.dof == 49.0);
        let h = normal_hist(&mut s, 100_000, 80, -4.0, 4.0);
        assert!(h.p > 1e-4, "normal p {}", h.p);
        let h = sum_hist(&mut s, 3, 50_000, 30);
        assert!(h.p > 1e-4, "sum p {}", h.p);
        assert!((irwin_hall_pdf(2, 1.0) - 1.0).abs() < 1e-12);
        assert!((irwin_hall_cdf(3, 1.5) - 0.5).abs() < 1e-12);
        let z = zero_excess(&MtConfig::mt19937(), 2000);
        std::println!("zero excess after 100/500/1000/2000 twists: {} {} {} {}", z[100], z[500], z[1000], z[2000]);
    }
}
