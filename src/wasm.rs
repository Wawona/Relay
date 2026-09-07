//! Experimental pure integer function frontend. Unsupported features are rejected
//! before execution; this module makes no WASI or full-WASM conformance claim.
use crate::{Error, Op, Outcome, Program};
use wasmparser::{ExternalKind, Operator, Parser, Payload, ValType, Validator};
#[path = "wasm_control.rs"]
mod control;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegerType {
    I32,
    I64,
}

impl IntegerType {
    fn from_wasm(value: ValType) -> Result<Self, Error> {
        match value {
            ValType::I32 => Ok(Self::I32),
            ValType::I64 => Ok(Self::I64),
            _ => Err(Error::InvalidProgram("unsupported function value type")),
        }
    }

    fn normalize(self, value: u64) -> u64 {
        match self {
            Self::I32 => value as u32 as u64,
            Self::I64 => value,
        }
    }
}

pub struct Function {
    program: Program,
    parameters: Vec<IntegerType>,
    result: IntegerType,
}

impl Function {
    /// Validate the entire module, then lower one exported pure function to validated
    /// register data. No guest bytes ever become native instructions.
    pub fn compile(bytes: &[u8], export: &str, fusion: bool) -> Result<Self, Error> {
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(Error::InvalidProgram("module byte limit"));
        }
        Validator::new()
            .validate_all(bytes)
            .map_err(|_| Error::InvalidProgram("invalid WebAssembly"))?;
        let unsupported =
            || Error::InvalidProgram("unsupported WebAssembly feature in experimental frontend");
        let mut types = Vec::new();
        let mut functions = Vec::new();
        let mut selected = None;
        let mut bodies = Vec::new();
        for payload in Parser::new(0).parse_all(bytes) {
            match payload.map_err(|_| unsupported())? {
                Payload::Version { .. }
                | Payload::End(_)
                | Payload::CustomSection(_)
                | Payload::CodeSectionStart { .. } => {}
                Payload::TypeSection(reader) => {
                    for ty in reader.into_iter_err_on_gc_types() {
                        types.push(ty.map_err(|_| unsupported())?);
                    }
                }
                Payload::FunctionSection(reader) => {
                    for ty in reader {
                        functions.push(ty.map_err(|_| unsupported())? as usize);
                    }
                }
                Payload::ExportSection(reader) => {
                    for item in reader {
                        let item = item.map_err(|_| unsupported())?;
                        if item.name == export && item.kind == ExternalKind::Func {
                            selected = Some(item.index as usize);
                        }
                    }
                }
                Payload::CodeSectionEntry(body) => bodies.push(body),
                _ => return Err(unsupported()),
            }
        }
        let index = selected.ok_or(Error::InvalidProgram("missing function export"))?;
        let ty = &types[functions[index]];
        if ty.results().len() != 1 {
            return Err(unsupported());
        }
        let result = IntegerType::from_wasm(ty.results()[0])?;
        let parameters = ty
            .params()
            .iter()
            .copied()
            .map(IntegerType::from_wasm)
            .collect::<Result<Vec<_>, _>>()?;
        let body = &bodies[index];
        let mut local_count = parameters.len();
        for local in body.get_locals_reader().map_err(|_| unsupported())? {
            let (count, ty) = local.map_err(|_| unsupported())?;
            if !matches!(ty, ValType::I64 | ValType::I32) {
                return Err(unsupported());
            }
            local_count = local_count
                .checked_add(count as usize)
                .ok_or_else(unsupported)?;
            if local_count > 65536 {
                return Err(unsupported());
            }
        }
        if local_count > 65536 {
            return Err(unsupported());
        }
        let structured = body
            .get_operators_reader()
            .map_err(|_| unsupported())?
            .into_iter()
            .any(|op| {
                matches!(
                    op,
                    Ok(Operator::Block { .. }
                        | Operator::Loop { .. }
                        | Operator::If { .. }
                        | Operator::Br { .. }
                        | Operator::BrIf { .. }
                        | Operator::Unreachable)
                )
            });
        if structured {
            return Ok(Self {
                program: control::compile(body, local_count, fusion)?,
                parameters,
                result,
            });
        }
        let mut locals: Vec<_> = (0..local_count).map(|v| v as u16).collect();
        let mut next_register = local_count;
        let mut fresh = || {
            let reg = u16::try_from(next_register).map_err(|_| Error::InvalidRegister)?;
            next_register += 1;
            Ok::<u16, Error>(reg)
        };
        let mut stack = Vec::new();
        let mut ops = Vec::new();
        let mut finished = false;
        for op in body.get_operators_reader().map_err(|_| unsupported())? {
            let op = op.map_err(|_| unsupported())?;
            if finished {
                if !matches!(op, Operator::End) {
                    return Err(unsupported());
                }
                continue;
            }
            match op {
                Operator::I64Const { value } => {
                    let dst = fresh()?;
                    ops.push(Op::Const {
                        dst,
                        value: value as u64,
                    });
                    stack.push(dst);
                }
                Operator::I32Const { value } => {
                    let dst = fresh()?;
                    ops.push(Op::Const {
                        dst,
                        value: value as u32 as u64,
                    });
                    stack.push(dst);
                }
                Operator::I64ExtendI32U | Operator::Nop => {}
                Operator::I64Eqz | Operator::I32Eqz => {
                    let src = stack.pop().ok_or_else(unsupported)?;
                    let dst = fresh()?;
                    ops.push(Op::Eqz { dst, src });
                    stack.push(dst);
                }
                ref other if unary_kind(other).is_some() => {
                    let src = stack.pop().ok_or_else(unsupported)?;
                    let dst = fresh()?;
                    ops.push(Op::Unary64 {
                        kind: unary_kind(other).unwrap(),
                        dst,
                        src,
                    });
                    stack.push(dst);
                }
                Operator::LocalGet { local_index } => stack.push(locals[local_index as usize]),
                Operator::LocalSet { local_index } => {
                    locals[local_index as usize] = stack.pop().ok_or_else(unsupported)?;
                }
                Operator::LocalTee { local_index } => {
                    locals[local_index as usize] = *stack.last().ok_or_else(unsupported)?;
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
                ref other if integer32_kind(other).is_some() => {
                    let rhs = stack.pop().ok_or_else(unsupported)?;
                    let lhs = stack.pop().ok_or_else(unsupported)?;
                    let dst = fresh()?;
                    ops.push(Op::Int32 {
                        kind: integer32_kind(other).unwrap(),
                        dst,
                        lhs,
                        rhs,
                    });
                    stack.push(dst);
                }
                Operator::Drop => {
                    stack.pop().ok_or_else(unsupported)?;
                }
                Operator::I64Add | Operator::I64Mul | Operator::I64DivS => {
                    let rhs = stack.pop().ok_or_else(unsupported)?;
                    let lhs = stack.pop().ok_or_else(unsupported)?;
                    let dst = fresh()?;
                    ops.push(match op {
                        Operator::I64Add => Op::Add { dst, lhs, rhs },
                        Operator::I64Mul => Op::Mul { dst, lhs, rhs },
                        _ => Op::DivSigned { dst, lhs, rhs },
                    });
                    stack.push(dst);
                }
                Operator::End | Operator::Return => {
                    ops.push(Op::Return {
                        src: stack.pop().ok_or_else(unsupported)?,
                    });
                    finished = true;
                }
                other => {
                    let kind = integer_kind(&other).ok_or_else(unsupported)?;
                    let rhs = stack.pop().ok_or_else(unsupported)?;
                    let lhs = stack.pop().ok_or_else(unsupported)?;
                    let dst = fresh()?;
                    ops.push(Op::Int64 {
                        kind,
                        dst,
                        lhs,
                        rhs,
                    });
                    stack.push(dst);
                }
            }
            if ops.len() > 1_000_000 {
                return Err(unsupported());
            }
        }
        let program = Program::compile(&ops, next_register.max(1), fusion)?;
        Ok(Self {
            program,
            parameters,
            result,
        })
    }

    pub fn parameter_types(&self) -> &[IntegerType] {
        &self.parameters
    }

    pub fn result_type(&self) -> IntegerType {
        self.result
    }

    /// Arguments and results carry integer bit patterns. I32 arguments use their
    /// low 32 bits; I32 results are zero extended. Signedness belongs to each
    /// WebAssembly operator, not to the function signature.
    pub fn run(&self, args: &[u64], fuel: u64) -> Result<Outcome, Error> {
        if args.len() != self.parameters.len() {
            return Err(Error::InvalidArguments);
        }
        let normalized;
        let args = if args
            .iter()
            .zip(&self.parameters)
            .any(|(&arg, &ty)| ty.normalize(arg) != arg)
        {
            normalized = args
                .iter()
                .zip(&self.parameters)
                .map(|(&arg, &ty)| ty.normalize(arg))
                .collect::<Vec<_>>();
            &normalized
        } else {
            args
        };
        self.program.run(args, &mut [], fuel)
    }

    pub fn fused_pairs(&self) -> usize {
        self.program.fused_pairs()
    }
}

fn integer_kind(op: &Operator<'_>) -> Option<crate::Int64> {
    use crate::Int64::*;
    Some(match op {
        Operator::I64Sub => Sub,
        Operator::I64And => And,
        Operator::I64Or => Or,
        Operator::I64Xor => Xor,
        Operator::I64Shl => Shl,
        Operator::I64ShrS => ShrS,
        Operator::I64ShrU => ShrU,
        Operator::I64Rotl => Rotl,
        Operator::I64Rotr => Rotr,
        Operator::I64Eq => Eq,
        Operator::I64Ne => Ne,
        Operator::I64LtS => LtS,
        Operator::I64LtU => LtU,
        Operator::I64GtS => GtS,
        Operator::I64GtU => GtU,
        Operator::I64LeS => LeS,
        Operator::I64LeU => LeU,
        Operator::I64GeS => GeS,
        Operator::I64GeU => GeU,
        Operator::I64DivU => DivU,
        Operator::I64RemS => RemS,
        Operator::I64RemU => RemU,
        _ => return None,
    })
}

fn unary_kind(op: &Operator<'_>) -> Option<crate::Unary64> {
    use crate::Unary64::*;
    Some(match op {
        Operator::I32WrapI64 => Wrap32,
        Operator::I32Clz => Clz32,
        Operator::I32Ctz => Ctz32,
        Operator::I32Popcnt => Popcnt32,
        Operator::I32Extend8S => Extend8S32,
        Operator::I32Extend16S => Extend16S32,
        Operator::I64Clz => Clz,
        Operator::I64Ctz => Ctz,
        Operator::I64Popcnt => Popcnt,
        Operator::I64Extend8S => Extend8S,
        Operator::I64Extend16S => Extend16S,
        Operator::I64Extend32S | Operator::I64ExtendI32S => Extend32S,
        _ => return None,
    })
}

fn integer32_kind(op: &Operator<'_>) -> Option<crate::Int32> {
    use crate::Int32::*;
    Some(match op {
        Operator::I32Add => Add,
        Operator::I32Mul => Mul,
        Operator::I32DivS => DivS,
        Operator::I32Sub => Sub,
        Operator::I32And => And,
        Operator::I32Or => Or,
        Operator::I32Xor => Xor,
        Operator::I32Shl => Shl,
        Operator::I32ShrS => ShrS,
        Operator::I32ShrU => ShrU,
        Operator::I32Rotl => Rotl,
        Operator::I32Rotr => Rotr,
        Operator::I32Eq => Eq,
        Operator::I32Ne => Ne,
        Operator::I32LtS => LtS,
        Operator::I32LtU => LtU,
        Operator::I32GtS => GtS,
        Operator::I32GtU => GtU,
        Operator::I32LeS => LeS,
        Operator::I32LeU => LeU,
        Operator::I32GeS => GeS,
        Operator::I32GeU => GeU,
        Operator::I32DivU => DivU,
        Operator::I32RemS => RemS,
        Operator::I32RemU => RemU,
        _ => return None,
    })
}
