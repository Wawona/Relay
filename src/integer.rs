use crate::{Control, Error, Handler, Instruction, Op, State};

/// WASM i64 operations with wrapping, shift masking, and exact integer traps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Int64 {
    Sub,
    And,
    Or,
    Xor,
    Shl,
    ShrS,
    ShrU,
    Rotl,
    Rotr,
    Eq,
    Ne,
    LtS,
    LtU,
    GtS,
    GtU,
    LeS,
    LeU,
    GeS,
    GeU,
    DivU,
    RemS,
    RemU,
}

pub(crate) fn handler(kind: Int64) -> Handler {
    // Each enum selects a statically compiled specialization. No second opcode
    // dispatch is needed in optimized execution, and no native code is emitted.
    macro_rules! select { ($($variant:ident),*) => {
        match kind { $(Int64::$variant => execute::<{Int64::$variant as u8}>),* }
    }; }
    select!(
        Sub, And, Or, Xor, Shl, ShrS, ShrU, Rotl, Rotr, Eq, Ne, LtS, LtU, GtS, GtU, LeS, LeU, GeS,
        GeU, DivU, RemS, RemU
    )
}

fn execute<const KIND: u8>(i: &Instruction, s: &mut State<'_>) -> Result<Control, Error> {
    s.charge()?;
    if let Op::Int64 { dst, lhs, rhs, .. } = i.op {
        let a = s.registers[lhs as usize];
        let b = s.registers[rhs as usize];
        s.registers[dst as usize] = match KIND {
            k if k == Int64::Sub as u8 => a.wrapping_sub(b),
            k if k == Int64::And as u8 => a & b,
            k if k == Int64::Or as u8 => a | b,
            k if k == Int64::Xor as u8 => a ^ b,
            k if k == Int64::Shl as u8 => a.wrapping_shl((b & 63) as u32),
            k if k == Int64::ShrS as u8 => ((a as i64) >> (b & 63)) as u64,
            k if k == Int64::ShrU as u8 => a >> (b & 63),
            k if k == Int64::Rotl as u8 => a.rotate_left((b & 63) as u32),
            k if k == Int64::Rotr as u8 => a.rotate_right((b & 63) as u32),
            k if k == Int64::Eq as u8 => u64::from(a == b),
            k if k == Int64::Ne as u8 => u64::from(a != b),
            k if k == Int64::LtS as u8 => u64::from((a as i64) < (b as i64)),
            k if k == Int64::LtU as u8 => u64::from(a < b),
            k if k == Int64::GtS as u8 => u64::from((a as i64) > (b as i64)),
            k if k == Int64::GtU as u8 => u64::from(a > b),
            k if k == Int64::LeS as u8 => u64::from((a as i64) <= (b as i64)),
            k if k == Int64::LeU as u8 => u64::from(a <= b),
            k if k == Int64::GeS as u8 => u64::from((a as i64) >= (b as i64)),
            k if k == Int64::GeU as u8 => u64::from(a >= b),
            k if k == Int64::DivU as u8 => a.checked_div(b).ok_or(Error::DivisionByZero)?,
            k if k == Int64::RemU as u8 => a.checked_rem(b).ok_or(Error::DivisionByZero)?,
            k if k == Int64::RemS as u8 => {
                if b == 0 {
                    return Err(Error::DivisionByZero);
                }
                (a as i64).wrapping_rem(b as i64) as u64
            }
            _ => unreachable!(),
        };
    }
    Ok(Control::Next(i.next))
}

/// Integer bit counts and sign extensions, compiled into static handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Unary64 {
    Clz,
    Ctz,
    Popcnt,
    Extend8S,
    Extend16S,
    Extend32S,
}
pub(crate) fn unary_handler(kind: Unary64) -> Handler {
    match kind {
        Unary64::Clz => unary::<0>,
        Unary64::Ctz => unary::<1>,
        Unary64::Popcnt => unary::<2>,
        Unary64::Extend8S => unary::<3>,
        Unary64::Extend16S => unary::<4>,
        Unary64::Extend32S => unary::<5>,
    }
}
fn unary<const KIND: u8>(i: &Instruction, s: &mut State<'_>) -> Result<Control, Error> {
    s.charge()?;
    if let Op::Unary64 { dst, src, .. } = i.op {
        let a = s.registers[src as usize];
        s.registers[dst as usize] = match KIND {
            0 => a.leading_zeros() as u64,
            1 => a.trailing_zeros() as u64,
            2 => a.count_ones() as u64,
            3 => a as i8 as i64 as u64,
            4 => a as i16 as i64 as u64,
            5 => a as i32 as i64 as u64,
            _ => unreachable!(),
        };
    }
    Ok(Control::Next(i.next))
}
