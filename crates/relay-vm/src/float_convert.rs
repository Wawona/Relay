//! Integer-only IEEE precision conversion for the advertised Arm FPCR model.
//! Host floating-point modes and host exception flags never affect results.

const IOC: u64 = 1;
const OFC: u64 = 4;
const UFC: u64 = 8;
const IXC: u64 = 16;
const IDC: u64 = 128;

pub(crate) fn narrow_double(input: u64, fpcr: u64) -> (u32, u64) {
    let negative = input >> 63 != 0;
    let sign = (input >> 32) as u32 & 0x8000_0000;
    let exponent = ((input >> 52) & 0x7ff) as i32;
    let fraction = input & ((1u64 << 52) - 1);
    let flush = fpcr & (1 << 24) != 0;
    if exponent == 0x7ff {
        if fraction == 0 {
            return (sign | 0x7f80_0000, 0);
        }
        let flags = if fraction & (1 << 51) == 0 { IOC } else { 0 };
        let nan = if fpcr & (1 << 25) != 0 {
            0x7fc0_0000
        } else {
            sign | 0x7fc0_0000 | (fraction >> 29) as u32
        };
        return (nan, flags);
    }
    if exponent == 0 {
        if fraction == 0 {
            return (sign, 0);
        }
        if flush {
            return (sign, IDC);
        }
    }
    let mut power = if exponent == 0 {
        -1022
    } else {
        exponent - 1023
    };
    let significand = fraction | if exponent == 0 { 0 } else { 1 << 52 };
    if power < -126 && flush {
        return (sign, UFC);
    }
    let shift = (29 + (-126 - power).max(0)) as u32;
    let (mut rounded, remainder, above_half, exactly_half) = if shift >= 64 {
        (0, significand != 0, false, false)
    } else {
        let tail = significand & ((1u64 << shift) - 1);
        let half = 1u64 << (shift - 1);
        (significand >> shift, tail != 0, tail > half, tail == half)
    };
    let mode = (fpcr >> 22) & 3;
    let increment = remainder
        && match mode {
            0 => above_half || (exactly_half && rounded & 1 != 0),
            1 => !negative,
            2 => negative,
            _ => false,
        };
    rounded += u64::from(increment);
    if power >= -126 && rounded == 1 << 24 {
        rounded >>= 1;
        power += 1;
    }
    if power > 127 {
        let infinity = mode == 0 || (mode == 1 && !negative) || (mode == 2 && negative);
        return (
            sign | if infinity { 0x7f80_0000 } else { 0x7f7f_ffff },
            OFC | IXC,
        );
    }
    let flags = if remainder {
        IXC | if power < -126 { UFC } else { 0 }
    } else {
        0
    };
    let result = if power < -126 {
        rounded as u32
    } else {
        (((power + 127) as u32) << 23) | (rounded as u32 & 0x7f_ffff)
    };
    (sign | result, flags)
}

#[cfg(kani)]
#[kani::proof]
fn narrow_double_preserves_sign_and_limits_status() {
    let input: u64 = kani::any();
    let fpcr: u64 = kani::any();
    let (result, flags) = narrow_double(input, fpcr);
    assert!(flags & !(IOC | OFC | UFC | IXC | IDC) == 0);
    // Default NaN intentionally discards the input sign.
    if result != 0x7fc0_0000 || fpcr & (1 << 25) == 0 {
        assert!(u64::from(result >> 31) == input >> 63);
    }
}

pub(crate) fn widen_single(input: u32, fpcr: u64) -> (u64, u64) {
    let sign = u64::from(input & 0x8000_0000) << 32;
    let exponent = (input >> 23) & 0xff;
    let fraction = input & 0x7f_ffff;
    if exponent == 0xff {
        if fraction == 0 {
            return (sign | 0x7ff0_0000_0000_0000, 0);
        }
        let flags = if fraction & (1 << 22) == 0 { IOC } else { 0 };
        let nan = if fpcr & (1 << 25) != 0 {
            0x7ff8_0000_0000_0000
        } else {
            sign | 0x7ff8_0000_0000_0000 | (u64::from(fraction) << 29)
        };
        return (nan, flags);
    }
    if exponent == 0 {
        if fraction == 0 {
            return (sign, 0);
        }
        if fpcr & (1 << 24) != 0 {
            return (sign, IDC);
        }
        let highest = 31 - fraction.leading_zeros();
        let double_exponent = u64::from(1023 - 149 + highest);
        let mantissa = (u64::from(fraction) << (52 - highest)) & ((1u64 << 52) - 1);
        return (sign | (double_exponent << 52) | mantissa, 0);
    }
    (
        sign | (u64::from(exponent + 896) << 52) | (u64::from(fraction) << 29),
        0,
    )
}

#[cfg(kani)]
#[kani::proof]
fn widen_single_is_exact_for_finite_unflushed_inputs() {
    let input: u32 = kani::any();
    let fpcr: u64 = kani::any();
    kani::assume(fpcr & (1 << 24) == 0);
    kani::assume(input & 0x7f80_0000 != 0x7f80_0000);
    let (wide, flags) = widen_single(input, fpcr);
    assert!(flags == 0);
    let (back, narrow_flags) = narrow_double(wide, fpcr);
    assert!(back == input);
    assert!(narrow_flags == 0);
}

pub(crate) fn integer_to_float(
    input: u64,
    wide: bool,
    signed: bool,
    double: bool,
    fpcr: u64,
) -> (u64, u64) {
    let input = if wide { input } else { u64::from(input as u32) };
    let signed_value = if wide {
        input as i64
    } else {
        input as u32 as i32 as i64
    };
    let negative = signed && signed_value < 0;
    let magnitude = if signed {
        signed_value.unsigned_abs()
    } else {
        input
    };
    if magnitude == 0 {
        return (0, 0);
    }
    let fraction_bits = if double { 52 } else { 23 };
    let bias = if double { 1023 } else { 127 };
    let sign = u64::from(negative) << if double { 63 } else { 31 };
    let mut highest = 63 - magnitude.leading_zeros();
    let mut flags = 0;
    let mut significand = if highest <= fraction_bits {
        magnitude << (fraction_bits - highest)
    } else {
        let shift = highest - fraction_bits;
        let tail = magnitude & ((1u64 << shift) - 1);
        let head = magnitude >> shift;
        let half = 1u64 << (shift - 1);
        let increment = tail != 0
            && match (fpcr >> 22) & 3 {
                0 => tail > half || (tail == half && head & 1 != 0),
                1 => !negative,
                2 => negative,
                _ => false,
            };
        if tail != 0 {
            flags = IXC;
        }
        head + u64::from(increment)
    };
    if significand == 1 << (fraction_bits + 1) {
        significand >>= 1;
        highest += 1;
    }
    (
        sign | (u64::from(highest + bias) << fraction_bits)
            | (significand & ((1 << fraction_bits) - 1)),
        flags,
    )
}

#[cfg(kani)]
#[kani::proof]
fn integer_conversion_cannot_overflow_floating_range() {
    let input: u64 = kani::any();
    let wide: bool = kani::any();
    let signed: bool = kani::any();
    let double: bool = kani::any();
    let fpcr: u64 = kani::any();
    let (result, flags) = integer_to_float(input, wide, signed, double, fpcr);
    assert!(flags == 0 || flags == IXC);
    if double {
        assert!(result & 0x7ff0_0000_0000_0000 != 0x7ff0_0000_0000_0000);
    } else {
        assert!(result <= u64::from(u32::MAX));
        assert!(result & 0x7f80_0000 != 0x7f80_0000);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precision_boundaries_preserve_architectural_results_and_flags() {
        assert_eq!(narrow_double(0x3ff0_0000_0000_0000, 0), (0x3f80_0000, 0));
        assert_eq!(narrow_double(0x8000_0000_0000_0000, 0), (0x8000_0000, 0));
        assert_eq!(narrow_double(0x3690_0000_0000_0000, 0), (0, UFC | IXC));
        assert_eq!(
            narrow_double(0x3690_0000_0000_0000, 1 << 22),
            (1, UFC | IXC)
        );
        assert_eq!(
            narrow_double(0x7fef_ffff_ffff_ffff, 0),
            (0x7f80_0000, OFC | IXC)
        );
        assert_eq!(
            narrow_double(0x7fef_ffff_ffff_ffff, 3 << 22),
            (0x7f7f_ffff, OFC | IXC)
        );
        assert_eq!(narrow_double(1, 1 << 24), (0, IDC));
        assert_eq!(narrow_double(0x7ff0_0000_0000_0001, 0), (0x7fc0_0000, IOC));
        assert_eq!(widen_single(1, 0), (0x36a0_0000_0000_0000, 0));
        assert_eq!(widen_single(1, 1 << 24), (0, IDC));
        assert_eq!(
            widen_single(0xff80_0001, 1 << 25),
            (0x7ff8_0000_0000_0000, IOC)
        );
    }

    #[test]
    fn integer_conversion_boundaries_match_reference_vectors() {
        assert_eq!(
            integer_to_float(u64::MAX, true, false, true, 0),
            (0x43f0_0000_0000_0000, IXC)
        );
        assert_eq!(
            integer_to_float(u64::MAX, true, false, true, 3 << 22),
            (0x43ef_ffff_ffff_ffff, IXC)
        );
        assert_eq!(
            integer_to_float(i64::MIN as u64, true, true, false, 0),
            (0xdf00_0000, 0)
        );
        assert_eq!(
            integer_to_float(u64::MAX, false, false, false, 0),
            (0x4f80_0000, IXC)
        );
        assert_eq!(
            integer_to_float(u64::MAX, false, true, false, 0),
            (0xbf80_0000, 0)
        );
    }
}
