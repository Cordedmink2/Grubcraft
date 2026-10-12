//! Deterministic fixed-point math for the Grubcraft simulation.
//!
//! Everything here is integer-only so results are bit-identical on every CPU
//! and compiler (Apple Silicon, x86-64, any OS). The simulation must use these
//! types instead of `f32`/`f64`; see `docs/10-architecture.md`.

#![no_std]

mod sin_table;

use core::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

const FRAC_BITS: u32 = 16;

/// Q16.16 fixed-point number: 16 integer bits, 16 fractional bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Fx(i32);

impl Fx {
    pub const ZERO: Fx = Fx(0);
    pub const ONE: Fx = Fx(1 << FRAC_BITS);
    pub const HALF: Fx = Fx(1 << (FRAC_BITS - 1));
    pub const MAX: Fx = Fx(i32::MAX);
    pub const MIN: Fx = Fx(i32::MIN);

    #[inline]
    pub const fn from_raw(raw: i32) -> Fx {
        Fx(raw)
    }

    #[inline]
    pub const fn raw(self) -> i32 {
        self.0
    }

    #[inline]
    pub const fn from_int(n: i32) -> Fx {
        Fx(n << FRAC_BITS)
    }

    /// `num / den` as fixed point, rounded toward negative infinity.
    #[inline]
    pub const fn from_ratio(num: i32, den: i32) -> Fx {
        Fx((((num as i64) << FRAC_BITS) / den as i64) as i32)
    }

    /// Integer part, rounded toward negative infinity.
    #[inline]
    pub const fn floor_int(self) -> i32 {
        self.0 >> FRAC_BITS
    }

    #[inline]
    pub const fn abs(self) -> Fx {
        Fx(self.0.wrapping_abs())
    }

    #[inline]
    pub fn min(self, other: Fx) -> Fx {
        if self.0 <= other.0 { self } else { other }
    }

    #[inline]
    pub fn max(self, other: Fx) -> Fx {
        if self.0 >= other.0 { self } else { other }
    }

    #[inline]
    pub fn clamp(self, lo: Fx, hi: Fx) -> Fx {
        self.max(lo).min(hi)
    }

    /// Square root; negative inputs return zero.
    pub fn sqrt(self) -> Fx {
        if self.0 <= 0 {
            return Fx::ZERO;
        }
        // sqrt(raw / 2^16) * 2^16 == sqrt(raw * 2^16)
        Fx(isqrt_u64((self.0 as u64) << FRAC_BITS) as i32)
    }
}

impl Add for Fx {
    type Output = Fx;
    #[inline]
    fn add(self, rhs: Fx) -> Fx {
        Fx(self.0.wrapping_add(rhs.0))
    }
}

impl Sub for Fx {
    type Output = Fx;
    #[inline]
    fn sub(self, rhs: Fx) -> Fx {
        Fx(self.0.wrapping_sub(rhs.0))
    }
}

impl Neg for Fx {
    type Output = Fx;
    #[inline]
    fn neg(self) -> Fx {
        Fx(self.0.wrapping_neg())
    }
}

impl Mul for Fx {
    type Output = Fx;
    #[inline]
    fn mul(self, rhs: Fx) -> Fx {
        Fx(((self.0 as i64 * rhs.0 as i64) >> FRAC_BITS) as i32)
    }
}

impl Mul<i32> for Fx {
    type Output = Fx;
    #[inline]
    fn mul(self, rhs: i32) -> Fx {
        Fx(self.0.wrapping_mul(rhs))
    }
}

impl Div for Fx {
    type Output = Fx;
    #[inline]
    fn div(self, rhs: Fx) -> Fx {
        Fx((((self.0 as i64) << FRAC_BITS) / rhs.0 as i64) as i32)
    }
}

impl Div<i32> for Fx {
    type Output = Fx;
    #[inline]
    fn div(self, rhs: i32) -> Fx {
        Fx(self.0 / rhs)
    }
}

impl AddAssign for Fx {
    #[inline]
    fn add_assign(&mut self, rhs: Fx) {
        *self = *self + rhs;
    }
}

impl SubAssign for Fx {
    #[inline]
    fn sub_assign(&mut self, rhs: Fx) {
        *self = *self - rhs;
    }
}

/// Integer square root, rounded down.
pub const fn isqrt_u64(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    // Newton's method from an initial guess that is always >= the root.
    let mut x = 1u64 << (64 - n.leading_zeros()).div_ceil(2);
    loop {
        let y = (x + n / x) / 2;
        if y >= x {
            return x;
        }
        x = y;
    }
}

/// A 2D vector in fixed point (map plane: x east, y south, in tiles).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FxVec2 {
    pub x: Fx,
    pub y: Fx,
}

impl FxVec2 {
    pub const ZERO: FxVec2 = FxVec2 {
        x: Fx::ZERO,
        y: Fx::ZERO,
    };

    #[inline]
    pub const fn new(x: Fx, y: Fx) -> FxVec2 {
        FxVec2 { x, y }
    }

    #[inline]
    pub const fn from_ints(x: i32, y: i32) -> FxVec2 {
        FxVec2 {
            x: Fx::from_int(x),
            y: Fx::from_int(y),
        }
    }

    /// Squared length in raw units (Q32.32), without overflow for any input.
    #[inline]
    pub fn length_sq_raw(self) -> u64 {
        let x = self.x.0 as i64;
        let y = self.y.0 as i64;
        (x * x) as u64 + (y * y) as u64
    }

    #[inline]
    pub fn length(self) -> Fx {
        Fx(isqrt_u64(self.length_sq_raw()) as i32)
    }

    /// Unit vector in the same direction, or zero for a zero vector.
    pub fn normalized(self) -> FxVec2 {
        let len = self.length();
        if len.0 == 0 {
            return FxVec2::ZERO;
        }
        FxVec2::new(self.x / len, self.y / len)
    }

    #[inline]
    pub fn dot(self, rhs: FxVec2) -> Fx {
        Fx(
            ((self.x.0 as i64 * rhs.x.0 as i64 + self.y.0 as i64 * rhs.y.0 as i64) >> FRAC_BITS)
                as i32,
        )
    }
}

impl Add for FxVec2 {
    type Output = FxVec2;
    #[inline]
    fn add(self, rhs: FxVec2) -> FxVec2 {
        FxVec2::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for FxVec2 {
    type Output = FxVec2;
    #[inline]
    fn sub(self, rhs: FxVec2) -> FxVec2 {
        FxVec2::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<Fx> for FxVec2 {
    type Output = FxVec2;
    #[inline]
    fn mul(self, rhs: Fx) -> FxVec2 {
        FxVec2::new(self.x * rhs, self.y * rhs)
    }
}

impl AddAssign for FxVec2 {
    #[inline]
    fn add_assign(&mut self, rhs: FxVec2) {
        *self = *self + rhs;
    }
}

/// An angle as a fraction of a full turn: 65,536 steps per revolution.
/// Wrapping arithmetic makes angle maths automatically modulo one turn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Angle(pub u16);

impl Angle {
    pub const ZERO: Angle = Angle(0);
    pub const QUARTER: Angle = Angle(0x4000);
    pub const HALF: Angle = Angle(0x8000);

    pub fn sin(self) -> Fx {
        // 1024 table entries, 64 angle steps between entries, linear interpolation.
        let idx = (self.0 >> 6) as usize;
        let frac = (self.0 & 63) as i32;
        let a = sin_table::SIN_TABLE[idx];
        let b = sin_table::SIN_TABLE[idx + 1];
        Fx(a + (((b - a) * frac) >> 6))
    }

    #[inline]
    pub fn cos(self) -> Fx {
        Angle(self.0.wrapping_add(Angle::QUARTER.0)).sin()
    }

    /// Unit vector pointing along this angle (0 = +x, quarter turn = +y).
    #[inline]
    pub fn to_vec(self) -> FxVec2 {
        FxVec2::new(self.cos(), self.sin())
    }

    #[inline]
    pub fn wrapping_add(self, rhs: Angle) -> Angle {
        Angle(self.0.wrapping_add(rhs.0))
    }
}

/// PCG32 (XSH RR) random number generator: small, fast, fully deterministic.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Pcg32 {
    state: u64,
    inc: u64,
}

impl Pcg32 {
    const MULT: u64 = 6364136223846793005;

    pub fn new(seed: u64, stream: u64) -> Pcg32 {
        let mut rng = Pcg32 {
            state: 0,
            inc: (stream << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    /// Internal state, for hashing and snapshots.
    pub fn raw_state(&self) -> (u64, u64) {
        (self.state, self.inc)
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(Self::MULT).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform integer in `0..bound` (unbiased, via rejection).
    pub fn below(&mut self, bound: u32) -> u32 {
        assert!(bound > 0);
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let r = self.next_u32();
            if r >= threshold {
                return r % bound;
            }
        }
    }

    /// Uniform fixed-point value in `[0, 1)`.
    pub fn unit(&mut self) -> Fx {
        Fx((self.next_u32() >> FRAC_BITS) as i32)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn arithmetic() {
        let a = Fx::from_int(3);
        let b = Fx::from_ratio(1, 2);
        assert_eq!((a * b).raw(), Fx::from_ratio(3, 2).raw());
        assert_eq!(a / Fx::from_int(2), Fx::from_ratio(3, 2));
        assert_eq!((-a).floor_int(), -3);
        assert_eq!(Fx::from_ratio(-1, 2).floor_int(), -1);
        assert_eq!(Fx::from_int(9).sqrt(), Fx::from_int(3));
        assert_eq!(Fx::from_int(-4).sqrt(), Fx::ZERO);
    }

    #[test]
    fn isqrt_is_floor_of_root() {
        for n in (0u64..10_000).chain([u64::MAX, u64::MAX - 1, 1 << 62, (1 << 32) - 1]) {
            let r = isqrt_u64(n);
            assert!(r * r <= n, "n={n} r={r}");
            assert!(
                (r + 1).checked_mul(r + 1).is_none_or(|sq| sq > n),
                "n={n} r={r}"
            );
        }
    }

    #[test]
    fn trig_matches_float_reference() {
        for step in 0..=u16::MAX as u32 {
            let a = Angle(step as u16);
            let expected = (step as f64 / 65536.0 * core::f64::consts::TAU).sin();
            let got = a.sin().raw() as f64 / 65536.0;
            assert!(
                (got - expected).abs() < 5e-5,
                "angle {step}: {got} vs {expected}"
            );
        }
        assert_eq!(Angle::ZERO.cos(), Fx::ONE);
        assert_eq!(Angle::QUARTER.sin(), Fx::ONE);
    }

    #[test]
    fn vectors() {
        let v = FxVec2::from_ints(3, 4);
        assert_eq!(v.length(), Fx::from_int(5));
        let n = v.normalized();
        assert!((n.length().raw() - Fx::ONE.raw()).abs() <= 2);
        assert_eq!(FxVec2::ZERO.normalized(), FxVec2::ZERO);
    }

    #[test]
    fn pcg32_known_sequence() {
        // Reference values from the PCG paper's demo (seed 42, stream 54).
        let mut rng = Pcg32::new(42, 54);
        let got: [u32; 6] = core::array::from_fn(|_| rng.next_u32());
        assert_eq!(
            got,
            [
                0xa15c02b7, 0x7b47f409, 0xba1d3330, 0x83d2f293, 0xbfa4784b, 0xcbed606e
            ]
        );
    }

    #[test]
    fn pcg32_below_in_range() {
        let mut rng = Pcg32::new(1, 1);
        for _ in 0..1000 {
            assert!(rng.below(7) < 7);
            assert!(rng.unit() < Fx::ONE);
        }
    }
}
