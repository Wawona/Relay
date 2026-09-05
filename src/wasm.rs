//! Experimental pure i64 function frontend. Unsupported features are rejected
//! before execution; this module makes no WASI or full-WASM conformance claim.
use crate::{Error, Op, Outcome, Program};
use wasmparser::{ExternalKind, Operator, Parser, Payload, ValType, Validator};

pub struct Function {
    program: Program,
    parameters: usize,
}

impl Function {
    /// Validate the entire module, then lower one exported pure function to SSA
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
        if ty.params().iter().any(|v| *v != ValType::I64) || ty.results() != [ValType::I64] {
            return Err(unsupported());
        }
        let parameters = ty.params().len();
        let body = &bodies[index];
        let mut local_count = parameters;
        for local in body.get_locals_reader().map_err(|_| unsupported())? {
            let (count, ty) = local.map_err(|_| unsupported())?;
            if ty != ValType::I64 {
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
                Operator::LocalGet { local_index } => stack.push(locals[local_index as usize]),
                Operator::LocalSet { local_index } => {
                    locals[local_index as usize] = stack.pop().ok_or_else(unsupported)?;
                }
                Operator::LocalTee { local_index } => {
                    locals[local_index as usize] = *stack.last().ok_or_else(unsupported)?;
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
                _ => return Err(unsupported()),
            }
            if ops.len() > 1_000_000 {
                return Err(unsupported());
            }
        }
        let program = Program::compile(&ops, next_register.max(1), fusion)?;
        Ok(Self {
            program,
            parameters,
        })
    }

    pub fn run(&self, args: &[u64], fuel: u64) -> Result<Outcome, Error> {
        if args.len() != self.parameters {
            return Err(Error::InvalidArguments);
        }
        self.program.run(args, &mut [], fuel)
    }

    pub fn fused_pairs(&self) -> usize {
        self.program.fused_pairs()
    }
}
