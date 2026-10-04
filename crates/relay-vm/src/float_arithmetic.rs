//! Software scalar arithmetic. Guest FP control never changes host FP state.
use rustc_apfloat::{
    ieee::{Double, Quad, Single},
    Float, FloatConvert, Round, Status, StatusAnd,
};

#[derive(Clone, Copy)]
pub(crate) enum Operation {
    Add,
    Subtract,
    Multiply,
    Divide,
    MultiplyAdd,
}

pub(crate) fn binary(
    left: u64,
    right: u64,
    double: bool,
    operation: Operation,
    fpcr: u64,
) -> (u64, u64) {
    if double {
        calculate::<Double>([left, right, 0], operation, fpcr, 63, 52, 0x7ff)
    } else {
        calculate::<Single>(
            [left as u32 as u64, right as u32 as u64, 0],
            operation,
            fpcr,
            31,
            23,
            0xff,
        )
    }
}

/// Scalar FMADD/FMSUB/FNMADD/FNMSUB share one fused operation. Negate inputs
/// before NaN selection and rounding, rather than negating the rounded result.
pub(crate) fn fused(
    mut left: u64,
    right: u64,
    mut addend: u64,
    double: bool,
    negate_product: bool,
    negate_addend: bool,
    fpcr: u64,
) -> (u64, u64) {
    let sign = 1u64 << if double { 63 } else { 31 };
    if negate_product {
        left ^= sign;
    }
    if negate_addend {
        addend ^= sign;
    }
    if double {
        calculate::<Double>(
            [left, right, addend],
            Operation::MultiplyAdd,
            fpcr,
            63,
            52,
            0x7ff,
        )
    } else {
        calculate::<Single>(
            [
                left as u32 as u64,
                right as u32 as u64,
                addend as u32 as u64,
            ],
            Operation::MultiplyAdd,
            fpcr,
            31,
            23,
            0xff,
        )
    }
}

fn evaluate<F: Float>(
    left: F,
    right: F,
    addend: F,
    operation: Operation,
    rounding: Round,
) -> StatusAnd<F> {
    match operation {
        Operation::Add => left.add_r(right, rounding),
        Operation::Subtract => left.sub_r(right, rounding),
        Operation::Multiply => left.mul_r(right, rounding),
        Operation::Divide => left.div_r(right, rounding),
        Operation::MultiplyAdd => left.mul_add_r(right, addend, rounding),
    }
}

fn calculate<F: Float + FloatConvert<Quad>>(
    inputs: [u64; 3],
    operation: Operation,
    fpcr: u64,
    sign_bit: u32,
    fraction_bits: u32,
    exponent: u64,
) -> (u64, u64) {
    let [mut left, mut right, mut addend] = inputs;
    let sign = 1u64 << sign_bit;
    let fraction = (1u64 << fraction_bits) - 1;
    let exponent = exponent << fraction_bits;
    let quiet = 1u64 << (fraction_bits - 1);
    let default_nan = exponent | quiet;
    let flush = fpcr & (1 << 24) != 0;
    let mut flags = 0;
    for input in [&mut left, &mut right, &mut addend] {
        if flush && *input & exponent == 0 && *input & fraction != 0 {
            *input &= sign;
            flags |= 128;
        }
    }
    let is_nan = |value: u64| value & exponent == exponent && value & fraction != 0;
    let signaling = |value: u64| is_nan(value) && value & quiet == 0;
    // FPMulAdd prioritizes addend, then multiplicands, with signaling NaNs
    // preceding quiet NaNs. Binary operations have an inert zero addend.
    let operands = [addend, left, right];
    let any_signaling = operands.into_iter().any(signaling);
    let nan = operands
        .into_iter()
        .find(|v| signaling(*v))
        .or_else(|| operands.into_iter().find(|v| is_nan(*v)));
    let invalid_product = matches!(operation, Operation::MultiplyAdd)
        && ((left & !sign == exponent && right & !sign == 0)
            || (right & !sign == exponent && left & !sign == 0));
    if invalid_product && !any_signaling {
        return (default_nan, flags | 1);
    }
    if let Some(nan) = nan {
        flags |= u64::from(any_signaling);
        return (
            if fpcr & (1 << 25) != 0 {
                default_nan
            } else {
                nan | quiet
            },
            flags,
        );
    }
    let rounding = match (fpcr >> 22) & 3 {
        0 => Round::NearestTiesToEven,
        1 => Round::TowardPositive,
        2 => Round::TowardNegative,
        _ => Round::TowardZero,
    };
    let a = F::from_bits(u128::from(left));
    let b = F::from_bits(u128::from(right));
    let c = F::from_bits(u128::from(addend));
    let result = evaluate(a, b, c, operation, rounding);
    // APFloat and ARM use the same five low exception bits.
    flags |= u64::from(result.status.bits());
    let mut bits = result.value.to_bits() as u64;
    // APFloat omits OVERFLOW when directed rounding saturates to the
    // largest finite value. Recover ARM's flag using the wider exponent
    // range; conversion of S/D operands to Quad is exact.
    if bits & !sign == exponent - 1 && result.status.contains(Status::INEXACT) {
        let a_wide: Quad = a.convert(&mut false).value;
        let b_wide: Quad = b.convert(&mut false).value;
        let c_wide: Quad = c.convert(&mut false).value;
        let magnitude = evaluate(a_wide, b_wide, c_wide, operation, Round::TowardZero)
            .value
            .to_bits()
            & !(1u128 << 127);
        let overflow_exponent = if sign_bit == 63 { 1024 } else { 128 };
        if magnitude >= ((16383u128 + overflow_exponent) << 112) {
            flags |= 4;
        }
    }
    if result.status.contains(Status::INVALID_OP) {
        bits = default_nan;
    }
    // ARM detects tininess before rounding. A value rounded up to the
    // smallest normal is still tiny; rounding toward zero exposes this.
    let tiny = result.value.is_denormal()
        || result.status.contains(Status::UNDERFLOW)
        || (bits & !sign == 1 << fraction_bits
            && result.status.contains(Status::INEXACT)
            && evaluate(a, b, c, operation, Round::TowardZero)
                .value
                .is_denormal());
    if tiny {
        if flush {
            bits &= sign;
            flags = (flags & !16) | 8;
        } else if result.status.contains(Status::INEXACT) {
            flags |= 8;
        }
    }
    (bits, flags)
}

/// FRINT{N,P,M,Z,A,X,I}: result stays floating point; only X raises IXC.
pub(crate) fn round_integral(input: u64, double: bool, mode: u32, fpcr: u64) -> (u64, u64) {
    let rounding = match mode {
        0 => Round::NearestTiesToEven,
        1 => Round::TowardPositive,
        2 => Round::TowardNegative,
        3 => Round::TowardZero,
        4 => Round::NearestTiesToAway,
        _ => match (fpcr >> 22) & 3 {
            0 => Round::NearestTiesToEven,
            1 => Round::TowardPositive,
            2 => Round::TowardNegative,
            _ => Round::TowardZero,
        },
    };
    if double {
        integral::<Double>(input, rounding, mode == 6, fpcr)
    } else {
        integral::<Single>(input as u32 as u64, rounding, mode == 6, fpcr)
    }
}

fn integral<F: Float>(input: u64, round: Round, exact: bool, fpcr: u64) -> (u64, u64) {
    let value = F::from_bits(u128::from(input));
    if fpcr & (1 << 24) != 0 && value.is_denormal() {
        return (F::ZERO.copy_sign(value).to_bits() as u64, 128);
    }
    let result = value.round_to_integral(round);
    let bits = if result.value.is_nan() && fpcr & (1 << 25) != 0 {
        F::NAN.to_bits()
    } else {
        result.value.to_bits()
    };
    let flags = u64::from(result.status.bits());
    (bits as u64, if exact { flags } else { flags & !16 })
}

/// Float-to-integer rounding is encoded by the instruction, not FPCR.RMode.
pub(crate) fn to_integer(
    input: u64,
    double: bool,
    wide: bool,
    signed: bool,
    rounding: u32,
    fpcr: u64,
) -> (u64, u64) {
    let round = match rounding {
        0 => Round::NearestTiesToEven,
        2 => Round::NearestTiesToAway,
        4 => Round::TowardPositive,
        8 => Round::TowardNegative,
        _ => Round::TowardZero,
    };
    if double {
        convert_integer::<Double>(input, wide, signed, round, fpcr)
    } else {
        convert_integer::<Single>(input as u32 as u64, wide, signed, round, fpcr)
    }
}

fn convert_integer<F: Float>(
    input: u64,
    wide: bool,
    signed: bool,
    round: Round,
    fpcr: u64,
) -> (u64, u64) {
    let value = F::from_bits(u128::from(input));
    if fpcr & (1 << 24) != 0 && value.is_denormal() {
        return (0, 128);
    }
    let width = if wide { 64 } else { 32 };
    let converted = if signed {
        value.to_i128_r(width, round, &mut false).map(|v| v as u128)
    } else {
        value.to_u128_r(width, round, &mut false)
    };
    let result = if wide {
        converted.value as u64
    } else {
        converted.value as u32 as u64
    };
    (result, u64::from(converted.status.bits()))
}

/// FCMP/FCMPE result nibble and cumulative exception contribution.
pub(crate) fn compare(
    mut left: u64,
    mut right: u64,
    double: bool,
    signaling: bool,
    fpcr: u64,
) -> (u8, u64) {
    let (sign, exponent, fraction, quiet) = if double {
        (
            1u64 << 63,
            0x7ff0_0000_0000_0000,
            0x000f_ffff_ffff_ffff,
            1u64 << 51,
        )
    } else {
        left = left as u32 as u64;
        right = right as u32 as u64;
        (1u64 << 31, 0x7f80_0000, 0x007f_ffff, 1u64 << 22)
    };
    let mut flags = 0;
    for input in [&mut left, &mut right] {
        if fpcr & (1 << 24) != 0 && *input & exponent == 0 && *input & fraction != 0 {
            *input &= sign;
            flags |= 128;
        }
    }
    let nan = |value: u64| value & exponent == exponent && value & fraction != 0;
    if nan(left) || nan(right) {
        let invalid =
            signaling || (nan(left) && left & quiet == 0) || (nan(right) && right & quiet == 0);
        return (3, flags | u64::from(invalid));
    }
    if left == right || (left | right) & !sign == 0 {
        return (6, flags);
    }
    let less = if (left ^ right) & sign != 0 {
        left & sign != 0
    } else if left & sign != 0 {
        left > right
    } else {
        left < right
    };
    (if less { 8 } else { 2 }, flags)
}

#[cfg(kani)]
#[kani::proof]
fn comparison_result_and_exception_domains() {
    let (nzcv, flags) = compare(
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
    );
    assert!(matches!(nzcv, 2 | 3 | 6 | 8));
    assert_eq!(flags & !129, 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fused_rounds_once_and_applies_each_input_sign_form() {
        // Exact products differ from 1 by 2^-46 / 2^-104. Separate multiply
        // then add would round the product to 1 and incorrectly return zero.
        assert_eq!(
            fused(
                0x3f80_0001,
                0x3f7f_fffe,
                0xbf80_0000,
                false,
                false,
                false,
                0
            ),
            (0xa880_0000, 0),
        );
        assert_eq!(
            fused(
                0x3ff0_0000_0000_0001,
                0x3fef_ffff_ffff_fffe,
                0xbff0_0000_0000_0000,
                true,
                false,
                false,
                0
            ),
            (0xb970_0000_0000_0000, 0),
        );
        for (product, addend, expected) in [
            (false, false, 7.0f64),
            (true, false, -5.0),
            (true, true, -7.0),
            (false, true, 5.0),
        ] {
            assert_eq!(
                fused(
                    2.0f64.to_bits(),
                    3.0f64.to_bits(),
                    1.0f64.to_bits(),
                    true,
                    product,
                    addend,
                    0
                ),
                (expected.to_bits(), 0),
            );
        }
    }

    #[test]
    fn integral_rounding_preserves_sign_nan_payload_and_exactness_policy() {
        for (input, mode, control, output, flags) in [
            (0x4020_0000, 0, 0, 0x4000_0000, 0),  // 2.5, ties even -> 2
            (0x4020_0000, 4, 0, 0x4040_0000, 0),  // ties away -> 3
            (0xbf00_0000, 2, 0, 0xbf80_0000, 0),  // floor(-0.5) -> -1
            (0xbf00_0000, 3, 0, 0x8000_0000, 0),  // trunc(-0.5) -> -0
            (0x3fc0_0000, 6, 0, 0x4000_0000, 16), // exact form raises IXC
            (0x3fc0_0000, 7, 0, 0x4000_0000, 0),
            (0x3fc0_0000, 6, 3 << 22, 0x3f80_0000, 16),
            (0x3fc0_0000, 7, 3 << 22, 0x3f80_0000, 0),
            (0x8000_0001, 6, 1 << 24, 0x8000_0000, 128),
            (0xff80_1234, 0, 0, 0xffc0_1234, 1),
            (0xff80_1234, 0, 1 << 25, 0x7fc0_0000, 1),
            (0x7fc0_5678, 6, 0, 0x7fc0_5678, 0),
        ] {
            assert_eq!(round_integral(input, false, mode, control), (output, flags));
        }
        assert_eq!(
            round_integral(0x8000_0000_0000_0001, true, 6, 1 << 24),
            (0x8000_0000_0000_0000, 128)
        );
        assert_eq!(
            round_integral(0x7ff0_0000_0000_1234, true, 7, 0),
            (0x7ff8_0000_0000_1234, 1)
        );
    }

    #[test]
    fn arithmetic_boundaries_preserve_arm_exceptions() {
        // Captured independently with native AArch64 in the reference corpus.
        let cases = [
            (0x3f80_0000, 0, Operation::Divide, 0, 0x7f80_0000, 2),
            (0, 0, Operation::Divide, 0, 0x7fc0_0000, 1),
            (0xbf00_0000, 1, Operation::Divide, 1 << 22, 0xff7f_ffff, 20),
            (0x0080_0000, 0x4040_0000, Operation::Divide, 1 << 24, 0, 8),
            (1, 0x3f80_0000, Operation::Add, 1 << 24, 0x3f80_0000, 128),
            (0x7fc0_5678, 0xff80_1234, Operation::Add, 0, 0xffc0_1234, 1),
            (
                0x7fc0_5678,
                0xff80_1234,
                Operation::Add,
                1 << 25,
                0x7fc0_0000,
                1,
            ),
            (
                0x3f80_0000,
                0x3f80_0000,
                Operation::Subtract,
                2 << 22,
                0x8000_0000,
                0,
            ),
            (
                0x0080_0000,
                0x3f00_0000,
                Operation::Multiply,
                0,
                0x0040_0000,
                0,
            ),
        ];
        for (left, right, operation, control, value, flags) in cases {
            assert_eq!(
                binary(left, right, false, operation, control),
                (value, flags)
            );
        }
        assert_eq!(
            binary(
                0x3ff0_0000_0000_0000,
                0x4008_0000_0000_0000,
                true,
                Operation::Divide,
                0
            ),
            (0x3fd5_5555_5555_5555, 16)
        );
    }
}
