//! decimal expansion of 2^p - 1. for p=19937 that's 6002 digits, shown at
//! the bottom of the period section.

use alloc::vec;
use alloc::vec::Vec;

const BASE: u32 = 1_000_000_000;

pub fn mersenne_decimal(p: u32) -> Vec<u8> {
    // schoolbook multiplication, limbs in base 1e9 (fits in u32)
    let mut limbs: Vec<u32> = vec![1];
    for _ in 0..p {
        let mut carry = 0u32;
        for l in limbs.iter_mut() {
            let v = *l * 2 + carry;
            if v >= BASE {
                *l = v - BASE;
                carry = 1;
            } else {
                *l = v;
                carry = 0;
            }
        }
        if carry > 0 {
            limbs.push(carry);
        }
    }
    // subtract 1 (2^p never ends in 0, so no borrow beyond the first limb)
    limbs[0] -= 1;
    let mut out = Vec::new();
    let top = limbs.len() - 1;
    let mut first = true;
    for i in (0..=top).rev() {
        let v = limbs[i];
        let mut digits = [0u8; 9];
        let mut x = v;
        for d in (0..9).rev() {
            digits[d] = b'0' + (x % 10) as u8;
            x /= 10;
        }
        if first {
            let mut k = 0;
            while k < 8 && digits[k] == b'0' {
                k += 1;
            }
            out.extend_from_slice(&digits[k..]);
            first = false;
        } else {
            out.extend_from_slice(&digits);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn m19937() {
        assert_eq!(mersenne_decimal(7), b"127");
        assert_eq!(mersenne_decimal(31), b"2147483647");
        let d = mersenne_decimal(19937);
        assert_eq!(d.len(), 6002);
        assert_eq!(&d[..6], b"431542"); // 2^19937−1 = 4315424797…
        assert_eq!(&d[d.len() - 6..], b"041471");
    }
}
