//! Guarded semantic acceleration. Only a pure, whole-program affine recurrence
//! is recognized. No memory, host calls, traps, or intermediate observations may
//! occur within it. Fuel remains the original IR cost, independent of speed.
use crate::{Op, State};

pub(crate) struct AffineLoop {
    value: u16,
    increment: u16,
    multiplier: u16,
    counter: u16,
    step: u16,
}

impl AffineLoop {
    pub(crate) fn recognize(ops: &[Op]) -> Option<Self> {
        let [Op::Add {
            dst: value,
            lhs: first,
            rhs: increment,
        }, Op::Mul {
            dst: second,
            lhs: third,
            rhs: multiplier,
        }, Op::Add {
            dst: counter,
            lhs: fourth,
            rhs: step,
        }, Op::JumpIf {
            condition,
            target: 0,
        }, Op::Return { .. }] = ops
        else {
            return None;
        };
        if value != first
            || value != second
            || value != third
            || counter != fourth
            || counter != condition
        {
            return None;
        }
        let registers = [value, increment, multiplier, counter, step];
        for i in 0..registers.len() {
            if registers[i + 1..].contains(&registers[i]) {
                return None;
            }
        }
        Some(Self {
            value: *value,
            increment: *increment,
            multiplier: *multiplier,
            counter: *counter,
            step: *step,
        })
    }

    pub(crate) fn execute(&self, state: &mut State<'_>) -> bool {
        let count = state.registers[self.counter as usize];
        // A zero-start do/while executes 2^64 times; it is not an empty loop.
        if count == 0 || state.registers[self.step as usize] != u64::MAX {
            return false;
        }
        let Some(cost) = count.checked_mul(4) else {
            return false;
        };
        if cost > state.fuel {
            return false;
        }
        let multiplier = state.registers[self.multiplier as usize];
        let increment = state.registers[self.increment as usize].wrapping_mul(multiplier);
        state.registers[self.value as usize] = affine_power(
            state.registers[self.value as usize],
            multiplier,
            increment,
            count,
        );
        state.registers[self.counter as usize] = 0;
        state.fuel -= cost;
        state.executed += cost;
        true
    }
}

/// Exponentiation of the affine map x -> a*x+b modulo 2^64.
/// Composition needs no division, so even multipliers and wraparound work.
fn affine_power(mut value: u64, mut a: u64, mut b: u64, mut count: u64) -> u64 {
    while count != 0 {
        if count & 1 != 0 {
            value = a.wrapping_mul(value).wrapping_add(b);
        }
        b = a.wrapping_mul(b).wrapping_add(b);
        a = a.wrapping_mul(a);
        count >>= 1;
    }
    value
}
