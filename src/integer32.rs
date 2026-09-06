use crate::{Control, Error, Handler, Instruction, Op, State};

/// WASM i32 operations with wrapping, shift masking, and exact integer traps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Int32 {
    Add,
    Mul,
    DivS,
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

pub(crate) fn handler(kind: Int32) -> Handler {
    // Each enum selects a statically compiled specialization. No second opcode
    // dispatch is needed in optimized execution, and no native code is emitted.
    macro_rules! select { ($($variant:ident),*) => {
        match kind { $(Int32::$variant => execute::<{Int32::$variant as u8}>),* }
    }; }
    select!(
        Add, Mul, DivS, Sub, And, Or, Xor, Shl, ShrS, ShrU, Rotl, Rotr, Eq, Ne, LtS, LtU, GtS, GtU,
        LeS, LeU, GeS, GeU, DivU, RemS, RemU
    )
}

fn execute<const KIND: u8>(i: &Instruction, s: &mut State<'_>) -> Result<Control, Error> {
    s.charge()?;
    if let Op::Int32 { dst, lhs, rhs, .. } = i.op {
        let a = s.registers[lhs as usize] as u32;
        let b = s.registers[rhs as usize] as u32;
        s.registers[dst as usize] = (match KIND {
            k if k == Int32::Add as u8 => a.wrapping_add(b),
            k if k == Int32::Mul as u8 => a.wrapping_mul(b),
            k if k == Int32::DivS as u8 => (a as i32).checked_div(b as i32).ok_or(if b == 0 {
                Error::DivisionByZero
            } else {
                Error::IntegerOverflow
            })? as u32,
            k if k == Int32::Sub as u8 => a.wrapping_sub(b),
            k if k == Int32::And as u8 => a & b,
            k if k == Int32::Or as u8 => a | b,
            k if k == Int32::Xor as u8 => a ^ b,
            k if k == Int32::Shl as u8 => a.wrapping_shl(b & 31),
            k if k == Int32::ShrS as u8 => ((a as i32) >> (b & 31)) as u32,
            k if k == Int32::ShrU as u8 => a >> (b & 31),
            k if k == Int32::Rotl as u8 => a.rotate_left(b & 31),
            k if k == Int32::Rotr as u8 => a.rotate_right(b & 31),
            k if k == Int32::Eq as u8 => u32::from(a == b),
            k if k == Int32::Ne as u8 => u32::from(a != b),
            k if k == Int32::LtS as u8 => u32::from((a as i32) < (b as i32)),
            k if k == Int32::LtU as u8 => u32::from(a < b),
            k if k == Int32::GtS as u8 => u32::from((a as i32) > (b as i32)),
            k if k == Int32::GtU as u8 => u32::from(a > b),
            k if k == Int32::LeS as u8 => u32::from((a as i32) <= (b as i32)),
            k if k == Int32::LeU as u8 => u32::from(a <= b),
            k if k == Int32::GeS as u8 => u32::from((a as i32) >= (b as i32)),
            k if k == Int32::GeU as u8 => u32::from(a >= b),
            k if k == Int32::DivU as u8 => a.checked_div(b).ok_or(Error::DivisionByZero)?,
            k if k == Int32::RemU as u8 => a.checked_rem(b).ok_or(Error::DivisionByZero)?,
            k if k == Int32::RemS as u8 => {
                if b == 0 {
                    return Err(Error::DivisionByZero);
                }
                (a as i32).wrapping_rem(b as i32) as u32
            }
            _ => unreachable!(),
        }) as u64;
    }
    Ok(Control::Next(i.next))
}
