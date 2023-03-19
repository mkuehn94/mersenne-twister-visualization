//! float math for no_std (ln, exp, sin, cos, erfc).
//! accuracy ~1e-15 rel for trig/log, ~1e-7 for erfc. good enough for chi2.

pub const PI: f64 = core::f64::consts::PI;
pub const LN2: f64 = core::f64::consts::LN_2;

#[inline]
pub fn sqrt(x: f64) -> f64 {
    {
            if x <= 0.0 {
            return if x == 0.0 { 0.0 } else { f64::NAN };
        }
        let mut y = f64::from_bits((x.to_bits() >> 1) + (1023u64 << 51));
        for _ in 0..6 {
            y = 0.5 * (y + x / y);
        }
        y
    }
}

#[inline]
pub fn floor(x: f64) -> f64 {
    {
        let t = x as i64 as f64;
        if t > x { t - 1.0 } else { t }
    }
}

#[inline]
pub fn abs(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & !(1u64 << 63))
}

pub fn ln(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if x.is_infinite() {
        return x;
    }
    let mut bits = x.to_bits();
    let mut e: i64 = ((bits >> 52) & 0x7ff) as i64;
    if e == 0 {
        // subnormal: scale up
        let y = x * (1u64 << 54) as f64;
        bits = y.to_bits();
        e = ((bits >> 52) & 0x7ff) as i64 - 54;
    }
    e -= 1023;
    let mut m = f64::from_bits((bits & 0x000f_ffff_ffff_ffff) | (1023u64 << 52)); // [1,2)
    if m > core::f64::consts::SQRT_2 {
        m *= 0.5;
        e += 1;
    }
    let s = (m - 1.0) / (m + 1.0);
    let s2 = s * s;
    let mut term = s;
    let mut sum = 0.0;
    let mut k = 1.0;
    while k < 60.0 {
        let add = term / k;
        sum += add;
        if abs(add) < 1e-18 {
            break;
        }
        term *= s2;
        k += 2.0;
    }
    2.0 * sum + e as f64 * LN2
}

pub fn exp(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x > 709.7 {
        return f64::INFINITY;
    }
    if x < -745.0 {
        return 0.0;
    }
    let k = floor(x / LN2 + 0.5);
    // Cody–Waite: ln 2 split into a short high part and a correction
    let r = (x - k * 6.931_471_803_691_238_164_90e-1) - k * 1.908_214_929_270_587_700_02e-10;
    let mut term = 1.0;
    let mut sum = 1.0;
    let mut i = 1.0;
    while i < 30.0 {
        term *= r / i;
        sum += term;
        if abs(term) < 1e-18 {
            break;
        }
        i += 1.0;
    }
    let ki = k as i64;
    // scale by 2^k in two halves to stay in range
    let half = ki / 2;
    let p1 = f64::from_bits(((half + 1023) as u64) << 52);
    let p2 = f64::from_bits(((ki - half + 1023) as u64) << 52);
    sum * p1 * p2
}

fn sin_kernel(x: f64) -> f64 {
    let x2 = x * x;
    let mut term = x;
    let mut sum = x;
    let mut i = 1.0;
    while i < 30.0 {
        term *= -x2 / ((2.0 * i) * (2.0 * i + 1.0));
        sum += term;
        if abs(term) < 1e-18 {
            break;
        }
        i += 1.0;
    }
    sum
}

fn cos_kernel(x: f64) -> f64 {
    let x2 = x * x;
    let mut term = 1.0;
    let mut sum = 1.0;
    let mut i = 1.0;
    while i < 30.0 {
        term *= -x2 / ((2.0 * i - 1.0) * (2.0 * i));
        sum += term;
        if abs(term) < 1e-18 {
            break;
        }
        i += 1.0;
    }
    sum
}

/// returns (sin x, cos x)
pub fn sincos(x: f64) -> (f64, f64) {
    let q = floor(x / (PI / 2.0) + 0.5);
    let r = x - q * (PI / 2.0);
    let (s, c) = (sin_kernel(r), cos_kernel(r));
    match (q as i64).rem_euclid(4) {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}

pub fn sin(x: f64) -> f64 {
    sincos(x).0
}
pub fn cos(x: f64) -> f64 {
    sincos(x).1
}

/// complementary error function (erfcc from Numerical Recipes)
pub fn erfc(x: f64) -> f64 {
    let z = abs(x);
    let t = 1.0 / (1.0 + 0.5 * z);
    let ans = t
        * exp(
            -z * z - 1.265_512_23
                + t * (1.000_023_68
                    + t * (0.374_091_96
                        + t * (0.096_784_18
                            + t * (-0.186_288_06
                                + t * (0.278_868_07
                                    + t * (-1.135_203_98
                                        + t * (1.488_515_87 + t * (-0.822_152_23 + t * 0.170_872_77)))))))),
        );
    if x >= 0.0 { ans } else { 2.0 - ans }
}

/// standard normal CDF Φ(x)
pub fn phi_cdf(x: f64) -> f64 {
    0.5 * erfc(-x / core::f64::consts::SQRT_2)
}

/// standard normal density φ(x)
pub fn phi_pdf(x: f64) -> f64 {
    exp(-0.5 * x * x) / sqrt(2.0 * PI)
}

/// ln Γ(x), Lanczos approximation (g = 7, n = 9)
pub fn lgamma(x: f64) -> f64 {
    const G: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        return ln(PI / abs(sin(PI * x))) - lgamma(1.0 - x);
    }
    let x = x - 1.0;
    let mut a = G[0];
    let t = x + 7.5;
    for i in 1..9 {
        a += G[i] / (x + i as f64);
    }
    0.5 * ln(2.0 * PI) + (x + 0.5) * ln(t) - t + ln(a)
}

/// upper incomplete gamma Q(a, x). chi-square p = Q(k/2, χ²/2)
pub fn gamma_q(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    if x < a + 1.0 {
        // series for P
        let mut ap = a;
        let mut sum = 1.0 / a;
        let mut del = sum;
        for _ in 0..1000 {
            ap += 1.0;
            del *= x / ap;
            sum += del;
            if abs(del) < abs(sum) * 1e-15 {
                break;
            }
        }
        let p = sum * exp(-x + a * ln(x) - lgamma(a));
        1.0 - p
    } else {
        // continued fraction for Q (Lentz)
        let tiny = 1e-300;
        let mut b = x + 1.0 - a;
        let mut c = 1.0 / tiny;
        let mut d = 1.0 / b;
        let mut h = d;
        let mut i = 1.0;
        while i < 1000.0 {
            let an = -i * (i - a);
            b += 2.0;
            d = an * d + b;
            if abs(d) < tiny {
                d = tiny;
            }
            c = b + an / c;
            if abs(c) < tiny {
                c = tiny;
            }
            d = 1.0 / d;
            let del = d * c;
            h *= del;
            if abs(del - 1.0) < 1e-15 {
                break;
            }
            i += 1.0;
        }
        exp(-x + a * ln(x) - lgamma(a)) * h
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn math() {
        for &x in &[1e-300, 1e-5, 0.3, 1.0, 2.5, 10.0, 1e10, 1e300] {
            assert!((ln(x) - x.ln()).abs() <= 1e-14 * x.ln().abs().max(1.0), "ln {x}");
        }
        for &x in &[-700.0, -20.0, -1.0, 0.0, 0.5, 3.3, 50.0, 700.0] {
            assert!((exp(x) - x.exp()).abs() <= 1e-14 * x.exp(), "exp {x}");
        }
        for &x in &[-100.0, -3.0, -0.1, 0.0, 0.7, 2.0, 6.28, 1000.0] {
            assert!((sin(x) - x.sin()).abs() < 1e-12, "sin {x}");
            assert!((cos(x) - x.cos()).abs() < 1e-12, "cos {x}");
        }
        assert!((phi_cdf(1.96) - 0.975002).abs() < 1e-6);
        assert!((lgamma(5.0) - 24f64.ln()).abs() < 1e-12);
        // chi2 with 10 dof at 18.307 has p = 0.05
        assert!((gamma_q(5.0, 18.307 / 2.0) - 0.05).abs() < 1e-4);
        assert!((gamma_q(50.0, 30.0) - 0.999_481_1).abs() < 1e-6);
        assert!((sqrt(2.0) - 2f64.sqrt()).abs() < 1e-15);
        assert_eq!(floor(-1.5), -2.0);
    }
}
