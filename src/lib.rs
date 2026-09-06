//! Static-handler execution foundation. Translation allocates ordinary data.
//! This register IR is not yet a complete WASM or QEMU frontend.
#![forbid(unsafe_code)]

use std::collections::BTreeSet;

#[cfg(feature = "wasm")]
pub mod wasm;

#[cfg(feature = "wasmtime-baseline")]
pub mod baseline;

mod integer;
mod optimize;
pub use integer::{Int64, Unary64};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Optimization {
    None,
    Fusion,
    Semantic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Const {
        dst: u16,
        value: u64,
    },
    Copy {
        dst: u16,
        src: u16,
    },
    Add {
        dst: u16,
        lhs: u16,
        rhs: u16,
    },
    Mul {
        dst: u16,
        lhs: u16,
        rhs: u16,
    },
    DivSigned {
        dst: u16,
        lhs: u16,
        rhs: u16,
    },
    Int64 {
        kind: Int64,
        dst: u16,
        lhs: u16,
        rhs: u16,
    },
    Unary64 {
        kind: Unary64,
        dst: u16,
        src: u16,
    },
    Eqz {
        dst: u16,
        src: u16,
    },
    Load64 {
        dst: u16,
        address: u16,
        offset: u64,
    },
    Store64 {
        src: u16,
        address: u16,
        offset: u64,
    },
    Jump {
        target: usize,
    },
    JumpIf {
        condition: u16,
        target: usize,
    },
    Return {
        src: u16,
    },
    Trap,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidProgram(&'static str),
    InvalidRegister,
    InvalidTarget,
    InvalidArguments,
    OutOfFuel,
    MemoryOutOfBounds,
    DivisionByZero,
    IntegerOverflow,
    Unreachable,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

type Handler = fn(&Instruction, &mut State<'_>) -> Result<Control, Error>;

struct Instruction {
    handler: Handler,
    op: Op,
    fused: Option<Op>,
    next: usize,
}

enum Control {
    Next(usize),
    Return(u64),
}

struct State<'a> {
    registers: Vec<u64>,
    memory: &'a mut [u8],
    fuel: u64,
    executed: u64,
}

impl State<'_> {
    fn charge(&mut self) -> Result<(), Error> {
        if self.fuel == 0 {
            return Err(Error::OutOfFuel);
        }
        self.fuel -= 1;
        self.executed += 1;
        Ok(())
    }
    fn range(&self, address: u16, offset: u64) -> Result<std::ops::Range<usize>, Error> {
        let start = self.registers[address as usize]
            .checked_add(offset)
            .and_then(|v| usize::try_from(v).ok())
            .ok_or(Error::MemoryOutOfBounds)?;
        let end = start.checked_add(8).ok_or(Error::MemoryOutOfBounds)?;
        if end > self.memory.len() {
            return Err(Error::MemoryOutOfBounds);
        }
        Ok(start..end)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    pub value: u64,
    pub remaining_fuel: u64,
    pub executed_ops: u64,
}

/// Validated immutable instruction data containing only locally selected handlers.
pub struct Program {
    instructions: Vec<Instruction>,
    registers: usize,
    fused_pairs: usize,
    accelerator: Option<optimize::AffineLoop>,
}

impl Program {
    pub fn compile(ops: &[Op], registers: usize, fuse: bool) -> Result<Self, Error> {
        if ops.is_empty() || ops.len() > 1_000_000 || registers == 0 || registers > 65536 {
            return Err(Error::InvalidProgram("program or register limit"));
        }
        let mut targets = BTreeSet::new();
        targets.insert(0);
        for (pc, op) in ops.iter().enumerate() {
            let valid = |r: u16| (r as usize) < registers;
            let registers_ok = match *op {
                Op::Const { dst, .. } => valid(dst),
                Op::Copy { dst, src } | Op::Eqz { dst, src } | Op::Unary64 { dst, src, .. } => {
                    valid(dst) && valid(src)
                }
                Op::Add { dst, lhs, rhs }
                | Op::Mul { dst, lhs, rhs }
                | Op::DivSigned { dst, lhs, rhs }
                | Op::Int64 { dst, lhs, rhs, .. } => valid(dst) && valid(lhs) && valid(rhs),
                Op::Load64 { dst, address, .. } => valid(dst) && valid(address),
                Op::Store64 { src, address, .. } => valid(src) && valid(address),
                Op::JumpIf { condition, .. } => valid(condition),
                Op::Return { src } => valid(src),
                Op::Jump { .. } | Op::Trap => true,
            };
            if !registers_ok {
                return Err(Error::InvalidRegister);
            }
            if let Op::Jump { target } | Op::JumpIf { target, .. } = *op {
                if target >= ops.len() {
                    return Err(Error::InvalidTarget);
                }
                targets.insert(target);
            }
            if pc + 1 == ops.len() && !matches!(op, Op::Return { .. } | Op::Jump { .. } | Op::Trap)
            {
                return Err(Error::InvalidProgram("fallthrough beyond program"));
            }
        }
        let mut instructions: Vec<_> = ops
            .iter()
            .enumerate()
            .map(|(pc, &op)| Instruction {
                handler: handler_for(op),
                op,
                fused: None,
                next: pc + 1,
            })
            .collect();
        let mut fused_pairs = 0;
        let mut pc = 0;
        while fuse && pc + 1 < ops.len() {
            // No entry into the middle of a fused pair. Retain original indices
            // so branches and fault locations never need approximate relocation.
            if !targets.contains(&(pc + 1))
                && matches!(ops[pc], Op::Add { .. })
                && matches!(ops[pc + 1], Op::Mul { .. })
            {
                instructions[pc].handler = add_mul;
                instructions[pc].fused = Some(ops[pc + 1]);
                instructions[pc].next = pc + 2;
                fused_pairs += 1;
                pc += 2;
            } else {
                pc += 1;
            }
        }
        Ok(Self {
            instructions,
            registers,
            fused_pairs,
            accelerator: None,
        })
    }

    pub fn compile_optimized(
        ops: &[Op],
        registers: usize,
        optimization: Optimization,
    ) -> Result<Self, Error> {
        let mut program = Self::compile(ops, registers, optimization != Optimization::None)?;
        if optimization == Optimization::Semantic {
            program.accelerator = optimize::AffineLoop::recognize(ops);
        }
        Ok(program)
    }

    pub fn has_semantic_accelerator(&self) -> bool {
        self.accelerator.is_some()
    }

    pub fn fused_pairs(&self) -> usize {
        self.fused_pairs
    }

    /// Registers start at zero; supplied values initialize a prefix.
    /// Fuel counts original IR operations equally with fusion enabled or disabled.
    pub fn run(&self, args: &[u64], memory: &mut [u8], fuel: u64) -> Result<Outcome, Error> {
        if args.len() > self.registers {
            return Err(Error::InvalidArguments);
        }
        let mut state = State {
            registers: vec![0; self.registers],
            memory,
            fuel,
            executed: 0,
        };
        state.registers[..args.len()].copy_from_slice(args);
        let mut pc = 0;
        if self
            .accelerator
            .as_ref()
            .is_some_and(|a| a.execute(&mut state))
        {
            pc = 4;
        }
        loop {
            let instruction = &self.instructions[pc];
            match (instruction.handler)(instruction, &mut state)? {
                Control::Next(next) => pc = next,
                Control::Return(value) => {
                    return Ok(Outcome {
                        value,
                        remaining_fuel: state.fuel,
                        executed_ops: state.executed,
                    })
                }
            }
        }
    }
}

fn handler_for(op: Op) -> Handler {
    match op {
        Op::Const { .. } => constant,
        Op::Copy { .. } => copy,
        Op::Add { .. } => add,
        Op::Mul { .. } => mul,
        Op::DivSigned { .. } => div_signed,
        Op::Int64 { kind, .. } => integer::handler(kind),
        Op::Unary64 { kind, .. } => integer::unary_handler(kind),
        Op::Eqz { .. } => eqz,
        Op::Trap => trap,
        Op::Load64 { .. } => load,
        Op::Store64 { .. } => store,
        Op::Jump { .. } => jump,
        Op::JumpIf { .. } => jump_if,
        Op::Return { .. } => ret,
    }
}

fn eqz(i: &Instruction, s: &mut State<'_>) -> Result<Control, Error> {
    s.charge()?;
    if let Op::Eqz { dst, src } = i.op {
        s.registers[dst as usize] = u64::from(s.registers[src as usize] == 0);
    }
    Ok(Control::Next(i.next))
}

fn trap(_: &Instruction, s: &mut State<'_>) -> Result<Control, Error> {
    s.charge()?;
    Err(Error::Unreachable)
}

fn constant(i: &Instruction, s: &mut State<'_>) -> Result<Control, Error> {
    s.charge()?;
    if let Op::Const { dst, value } = i.op {
        s.registers[dst as usize] = value;
    }
    Ok(Control::Next(i.next))
}
fn copy(i: &Instruction, s: &mut State<'_>) -> Result<Control, Error> {
    s.charge()?;
    if let Op::Copy { dst, src } = i.op {
        s.registers[dst as usize] = s.registers[src as usize];
    }
    Ok(Control::Next(i.next))
}
fn add(i: &Instruction, s: &mut State<'_>) -> Result<Control, Error> {
    s.charge()?;
    if let Op::Add { dst, lhs, rhs } = i.op {
        s.registers[dst as usize] =
            s.registers[lhs as usize].wrapping_add(s.registers[rhs as usize]);
    }
    Ok(Control::Next(i.next))
}
fn mul(i: &Instruction, s: &mut State<'_>) -> Result<Control, Error> {
    s.charge()?;
    if let Op::Mul { dst, lhs, rhs } = i.op {
        s.registers[dst as usize] =
            s.registers[lhs as usize].wrapping_mul(s.registers[rhs as usize]);
    }
    Ok(Control::Next(i.next))
}
fn add_mul(i: &Instruction, s: &mut State<'_>) -> Result<Control, Error> {
    add(i, s)?;
    s.charge()?;
    if let Some(Op::Mul { dst, lhs, rhs }) = i.fused {
        s.registers[dst as usize] =
            s.registers[lhs as usize].wrapping_mul(s.registers[rhs as usize]);
    }
    Ok(Control::Next(i.next))
}
fn div_signed(i: &Instruction, s: &mut State<'_>) -> Result<Control, Error> {
    s.charge()?;
    if let Op::DivSigned { dst, lhs, rhs } = i.op {
        let (a, b) = (
            s.registers[lhs as usize] as i64,
            s.registers[rhs as usize] as i64,
        );
        if b == 0 {
            return Err(Error::DivisionByZero);
        }
        s.registers[dst as usize] = a.checked_div(b).ok_or(Error::IntegerOverflow)? as u64;
    }
    Ok(Control::Next(i.next))
}
fn load(i: &Instruction, s: &mut State<'_>) -> Result<Control, Error> {
    s.charge()?;
    if let Op::Load64 {
        dst,
        address,
        offset,
    } = i.op
    {
        let range = s.range(address, offset)?;
        let mut bytes = [0; 8];
        bytes.copy_from_slice(&s.memory[range]);
        s.registers[dst as usize] = u64::from_le_bytes(bytes);
    }
    Ok(Control::Next(i.next))
}
fn store(i: &Instruction, s: &mut State<'_>) -> Result<Control, Error> {
    s.charge()?;
    if let Op::Store64 {
        src,
        address,
        offset,
    } = i.op
    {
        let range = s.range(address, offset)?;
        s.memory[range].copy_from_slice(&s.registers[src as usize].to_le_bytes());
    }
    Ok(Control::Next(i.next))
}
fn jump(i: &Instruction, s: &mut State<'_>) -> Result<Control, Error> {
    s.charge()?;
    if let Op::Jump { target } = i.op {
        return Ok(Control::Next(target));
    }
    unreachable!()
}
fn jump_if(i: &Instruction, s: &mut State<'_>) -> Result<Control, Error> {
    s.charge()?;
    if let Op::JumpIf { condition, target } = i.op {
        return Ok(Control::Next(if s.registers[condition as usize] != 0 {
            target
        } else {
            i.next
        }));
    }
    unreachable!()
}
fn ret(i: &Instruction, s: &mut State<'_>) -> Result<Control, Error> {
    s.charge()?;
    if let Op::Return { src } = i.op {
        return Ok(Control::Return(s.registers[src as usize]));
    }
    unreachable!()
}
