//! Structured control lowering. Local registers are mutable across control
//! joins; stack reads snapshot them. Branch result registers implement merges.
use crate::{Error, Op, Program};
use wasmparser::{BlockType, FunctionBody, Operator, ValType};

#[derive(PartialEq)]
enum Kind {
    Function,
    Block,
    Loop,
    If,
}
struct Frame {
    kind: Kind,
    start: usize,
    stack: Vec<u16>,
    result: Option<u16>,
    exits: Vec<usize>,
    false_jump: Option<usize>,
    entry_reachable: bool,
    end_reachable: bool,
}
fn unsupported() -> Error {
    Error::InvalidProgram("unsupported structured WASM feature")
}
fn patch(ops: &mut [Op], index: usize, destination: usize) {
    match &mut ops[index] {
        Op::Jump { target } | Op::JumpIf { target, .. } => *target = destination,
        _ => unreachable!(),
    }
}
fn merge(ops: &mut Vec<Op>, frame: &Frame, stack: &[u16]) -> Result<(), Error> {
    if let Some(dst) = frame.result {
        ops.push(Op::Copy {
            dst,
            src: *stack.last().ok_or_else(unsupported)?,
        });
    }
    Ok(())
}

pub(super) fn compile(
    body: &FunctionBody<'_>,
    locals: usize,
    fusion: bool,
) -> Result<Program, Error> {
    let mut registers = locals;
    let mut fresh = || {
        let register = u16::try_from(registers).map_err(|_| Error::InvalidRegister)?;
        registers += 1;
        Ok::<_, Error>(register)
    };
    let mut ops = Vec::new();
    let mut stack = Vec::new();
    let result = fresh()?;
    let mut frames = vec![Frame {
        kind: Kind::Function,
        start: 0,
        stack: vec![],
        result: Some(result),
        exits: vec![],
        false_jump: None,
        entry_reachable: true,
        end_reachable: false,
    }];
    let mut reachable = true;
    for op in body.get_operators_reader().map_err(|_| unsupported())? {
        let op = op.map_err(|_| unsupported())?;
        match op {
            Operator::Block { blockty } | Operator::Loop { blockty } | Operator::If { blockty } => {
                let result = match blockty {
                    BlockType::Empty => None,
                    BlockType::Type(ValType::I64 | ValType::I32) => Some(fresh()?),
                    _ => return Err(unsupported()),
                };
                let kind = match op {
                    Operator::Loop { .. } => Kind::Loop,
                    Operator::If { .. } => Kind::If,
                    _ => Kind::Block,
                };
                let false_jump = if kind == Kind::If && reachable {
                    let src = stack.pop().ok_or_else(unsupported)?;
                    let condition = fresh()?;
                    ops.push(Op::Eqz {
                        dst: condition,
                        src,
                    });
                    let index = ops.len();
                    ops.push(Op::JumpIf {
                        condition,
                        target: usize::MAX,
                    });
                    Some(index)
                } else {
                    None
                };
                if frames.len() >= 1024 {
                    return Err(Error::InvalidProgram("control nesting limit"));
                }
                frames.push(Frame {
                    kind,
                    result,
                    start: ops.len(),
                    stack: stack.clone(),
                    exits: vec![],
                    false_jump,
                    entry_reachable: reachable,
                    end_reachable: false,
                });
            }
            Operator::Else => {
                let frame = frames.last_mut().ok_or_else(unsupported)?;
                if frame.kind != Kind::If {
                    return Err(unsupported());
                }
                if reachable {
                    merge(&mut ops, frame, &stack)?;
                    frame.exits.push(ops.len());
                    ops.push(Op::Jump { target: usize::MAX });
                    frame.end_reachable = true;
                }
                let end = ops.len();
                if let Some(index) = frame.false_jump.take() {
                    patch(&mut ops, index, end);
                }
                stack = frame.stack.clone();
                reachable = frame.entry_reachable;
            }
            Operator::End => {
                let frame = frames.pop().ok_or_else(unsupported)?;
                if reachable {
                    merge(&mut ops, &frame, &stack)?;
                }
                let end = ops.len();
                if let Some(index) = frame.false_jump {
                    patch(&mut ops, index, end);
                }
                for &index in &frame.exits {
                    patch(&mut ops, index, end);
                }
                reachable = reachable
                    || frame.end_reachable
                    || (frame.false_jump.is_some() && frame.entry_reachable);
                stack = frame.stack;
                if let Some(reg) = frame.result {
                    stack.push(reg);
                }
                if frame.kind == Kind::Function {
                    ops.push(Op::Return { src: result });
                }
            }
            _ if !reachable => {}
            Operator::Br { relative_depth } | Operator::BrIf { relative_depth } => {
                let conditional = matches!(op, Operator::BrIf { .. });
                let condition = if conditional {
                    Some(stack.pop().ok_or_else(unsupported)?)
                } else {
                    None
                };
                let index = frames
                    .len()
                    .checked_sub(relative_depth as usize + 1)
                    .ok_or_else(unsupported)?;
                let frame = &mut frames[index];
                let target = if frame.kind == Kind::Loop {
                    frame.start
                } else {
                    merge(&mut ops, frame, &stack)?;
                    frame.exits.push(ops.len());
                    frame.end_reachable = true;
                    usize::MAX
                };
                ops.push(if let Some(condition) = condition {
                    Op::JumpIf { condition, target }
                } else {
                    Op::Jump { target }
                });
                if !conditional {
                    reachable = false;
                }
            }
            Operator::Return => {
                ops.push(Op::Return {
                    src: stack.pop().ok_or_else(unsupported)?,
                });
                reachable = false;
            }
            Operator::Unreachable => {
                ops.push(Op::Trap);
                reachable = false;
            }
            Operator::Nop | Operator::I64ExtendI32U => {}
            Operator::I32Const { value } => {
                let dst = fresh()?;
                ops.push(Op::Const {
                    dst,
                    value: value as u32 as u64,
                });
                stack.push(dst);
            }
            Operator::I64Const { value } => {
                let dst = fresh()?;
                ops.push(Op::Const {
                    dst,
                    value: value as u64,
                });
                stack.push(dst);
            }
            ref other if super::unary_kind(other).is_some() => {
                let src = stack.pop().ok_or_else(unsupported)?;
                let dst = fresh()?;
                ops.push(Op::Unary64 {
                    kind: super::unary_kind(other).unwrap(),
                    dst,
                    src,
                });
                stack.push(dst);
            }
            Operator::LocalGet { local_index } => {
                let dst = fresh()?;
                ops.push(Op::Copy {
                    dst,
                    src: local_index as u16,
                });
                stack.push(dst);
            }
            Operator::LocalSet { local_index } => {
                ops.push(Op::Copy {
                    dst: local_index as u16,
                    src: stack.pop().ok_or_else(unsupported)?,
                });
            }
            Operator::LocalTee { local_index } => {
                ops.push(Op::Copy {
                    dst: local_index as u16,
                    src: *stack.last().ok_or_else(unsupported)?,
                });
            }
            Operator::Select
            | Operator::TypedSelect {
                ty: ValType::I64 | ValType::I32,
            } => {
                let condition = stack.pop().ok_or_else(unsupported)?;
                let if_false = stack.pop().ok_or_else(unsupported)?;
                let if_true = stack.pop().ok_or_else(unsupported)?;
                let dst = fresh()?;
                ops.push(Op::Select {
                    dst,
                    condition,
                    if_true,
                    if_false,
                });
                stack.push(dst);
            }
            ref other if super::integer32_kind(other).is_some() => {
                let rhs = stack.pop().ok_or_else(unsupported)?;
                let lhs = stack.pop().ok_or_else(unsupported)?;
                let dst = fresh()?;
                ops.push(Op::Int32 {
                    kind: super::integer32_kind(other).unwrap(),
                    dst,
                    lhs,
                    rhs,
                });
                stack.push(dst);
            }
            Operator::Drop => {
                stack.pop().ok_or_else(unsupported)?;
            }
            Operator::I64Eqz | Operator::I32Eqz => {
                let src = stack.pop().ok_or_else(unsupported)?;
                let dst = fresh()?;
                ops.push(Op::Eqz { dst, src });
                stack.push(dst);
            }
            other => {
                let rhs = stack.pop().ok_or_else(unsupported)?;
                let lhs = stack.pop().ok_or_else(unsupported)?;
                let dst = fresh()?;
                ops.push(match other {
                    Operator::I64Add => Op::Add { dst, lhs, rhs },
                    Operator::I64Mul => Op::Mul { dst, lhs, rhs },
                    Operator::I64DivS => Op::DivSigned { dst, lhs, rhs },
                    _ => Op::Int64 {
                        kind: super::integer_kind(&other).ok_or_else(unsupported)?,
                        dst,
                        lhs,
                        rhs,
                    },
                });
                stack.push(dst);
            }
        }
        if ops.len() > 1_000_000 {
            return Err(unsupported());
        }
    }
    if !frames.is_empty() {
        return Err(unsupported());
    }
    Program::compile(&ops, registers, fusion)
}
