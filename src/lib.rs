#![deny(
	clippy::disallowed_methods,
	clippy::str_to_string,
	clippy::string_to_string,
	clippy::todo,
	clippy::unimplemented
)]
#![warn(
	clippy::pedantic,
	clippy::nursery,
	clippy::redundant_type_annotations,
	clippy::try_err,
	clippy::impl_trait_in_params
)]
#![allow(
	clippy::missing_errors_doc,
	clippy::missing_panics_doc,
	clippy::missing_safety_doc,
	clippy::cast_possible_truncation,
	clippy::module_name_repetitions,
	clippy::cast_possible_wrap,
	clippy::needless_pass_by_value,
	clippy::cast_precision_loss,
	clippy::cast_sign_loss,
	clippy::significant_drop_tightening,
	clippy::similar_names,
	clippy::enum_glob_use,
	clippy::items_after_statements,
	clippy::unnecessary_wraps
)]
#![cfg_attr(
	docsrs,
	feature(doc_auto_cfg, doc_cfg),
	deny(rustdoc::broken_intra_doc_links)
)]

use std::{
	io::{self, Read as _, Write as _},
	ptr,
};

use cranelift::{
	codegen::{
		ir::{types::I8, Function, UserFuncName},
		isa::CallConv,
		settings, verify_function, Context,
	},
	prelude::*,
};
use target_lexicon::Triple;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instruction {
	Add(i8),
	Move(i32),
	Input,
	Output,
	JumpRight,
	JumpLeft,
	Clear,
	AddTo(i32),
}

pub struct Program {
	pub code: Vec<u8>,
	pub memory: [u8; 30_000],
}

impl Program {
	#[allow(clippy::while_let_on_iterator)]
	pub fn new(source: &[u8], clir: bool) -> Result<Self, RunError> {
		let mut instructions = Vec::new();

		for b in source {
			let instr = match b {
				b'+' | b'-' => {
					let inc = if *b == b'+' { 1 } else { -1 };
					if let Some(Instruction::Add(value)) = instructions.last_mut() {
						*value = value.wrapping_add(inc);
						continue;
					}
					Instruction::Add(inc)
				}
				b'.' => Instruction::Output,
				b',' => Instruction::Input,
				b'>' | b'<' => {
					let inc = if *b == b'>' { 1 } else { -1 };
					if let Some(Instruction::Move(value)) = instructions.last_mut() {
						*value += inc;
						continue;
					}
					Instruction::Move(inc)
				}
				b'[' => Instruction::JumpRight,
				b']' => {
					use Instruction::*;
					match instructions.as_slice() {
						[.., JumpRight, Add(n)] if *n as u8 % 2 == 1 => {
							let len = instructions.len();
							instructions.drain(len - 2..);
							Instruction::Clear
						}
						&[.., JumpRight, Add(-1), Move(x), Add(1), Move(y)] if x == -y => {
							let len = instructions.len();
							instructions.drain(len - 5..);
							Instruction::AddTo(x)
						}
						_ => Instruction::JumpLeft,
					}
				}
				_ => continue,
			};
			instructions.push(instr);
		}

		let mut builder = settings::builder();
		builder.set("opt_level", "speed").unwrap();
		builder.set("preserve_frame_pointers", "false").unwrap();
		let flags = settings::Flags::new(builder);

		let isa = isa::lookup(Triple::host()).map_or_else(
			|_| panic!("x86_64 ISA is not available"),
			|isa_builder| isa_builder.finish(flags).unwrap(),
		);

		let pointer_type = isa.pointer_type();

		let call_conv = CallConv::triple_default(isa.triple());

		let mut sig = Signature::new(call_conv);
		sig.params.push(AbiParam::new(pointer_type));
		sig.returns.push(AbiParam::new(pointer_type));

		let mut func = Function::with_name_signature(UserFuncName::user(0, 0), sig);

		let mut func_ctx = FunctionBuilderContext::new();
		let mut builder = FunctionBuilder::new(&mut func, &mut func_ctx);

		let pointer = Variable::new(0);
		builder.declare_var(pointer, pointer_type);

		let exit_block = builder.create_block();
		builder.append_block_param(exit_block, pointer_type);

		let block = builder.create_block();
		builder.seal_block(block);

		builder.append_block_params_for_function_params(block);
		builder.switch_to_block(block);

		let memory_address = builder.block_params(block)[0];

		let zero_byte = builder.ins().iconst(I8, 0);
		let zero = builder.ins().iconst(pointer_type, 0);
		builder.def_var(pointer, zero);

		let mem_flags = MemFlags::new();

		let (write_sig, write_address) = {
			let mut write_sig = Signature::new(call_conv);
			write_sig.params.push(AbiParam::new(I8));
			write_sig.returns.push(AbiParam::new(pointer_type));
			let write_sig = builder.import_signature(write_sig);

			let write_address = write as *const () as i64;
			let write_address = builder.ins().iconst(pointer_type, write_address);
			(write_sig, write_address)
		};

		let (read_sig, read_address) = {
			let mut read_sig = Signature::new(call_conv);
			read_sig.params.push(AbiParam::new(pointer_type));
			read_sig.returns.push(AbiParam::new(pointer_type));
			let read_sig = builder.import_signature(read_sig);

			let read_address = read as *const () as i64;
			let read_address = builder.ins().iconst(pointer_type, read_address);
			(read_sig, read_address)
		};

		let mut stack = Vec::new();

		for (i, instr) in instructions.into_iter().enumerate() {
			match instr {
				Instruction::Add(n) => {
					let n = i64::from(n);
					let pointer_value = builder.use_var(pointer);
					let cell_address = builder.ins().iadd(memory_address, pointer_value);
					let cell_value = builder.ins().load(I8, mem_flags, cell_address, 0);
					let cell_value = builder.ins().iadd_imm(cell_value, n);
					builder.ins().store(mem_flags, cell_value, cell_address, 0);
				}
				Instruction::Move(n) => {
					let n = i64::from(n);
					let pointer_value = builder.use_var(pointer);
					let pointer_plus = builder.ins().iadd_imm(pointer_value, n);

					let pointer_value = if n > 0 {
						let wrapped = builder.ins().iadd_imm(pointer_value, n - 30_000);
						let cmp =
							builder
								.ins()
								.icmp_imm(IntCC::SignedLessThan, pointer_plus, 30_000);
						builder.ins().select(cmp, pointer_plus, wrapped)
					} else {
						let wrapped = builder.ins().iadd_imm(pointer_value, n + 30_000);
						let cmp = builder
							.ins()
							.icmp_imm(IntCC::SignedLessThan, pointer_plus, 0);
						builder.ins().select(cmp, wrapped, pointer_plus)
					};

					builder.def_var(pointer, pointer_value);
				}
				Instruction::Output => {
					let pointer_value = builder.use_var(pointer);
					let cell_address = builder.ins().iadd(memory_address, pointer_value);
					let cell_value = builder.ins().load(I8, mem_flags, cell_address, 0);

					let inst = builder
						.ins()
						.call_indirect(write_sig, write_address, &[cell_value]);
					let result = builder.inst_results(inst)[0];

					let after_block = builder.create_block();

					builder.ins().brnz(result, exit_block, &[result]);
					builder.ins().jump(after_block, &[]);

					builder.seal_block(after_block);
					builder.switch_to_block(after_block);
				}
				Instruction::Input => {
					let pointer_value = builder.use_var(pointer);
					let cell_address = builder.ins().iadd(memory_address, pointer_value);

					let inst = builder
						.ins()
						.call_indirect(read_sig, read_address, &[cell_address]);
					let result = builder.inst_results(inst)[0];

					let after_block = builder.create_block();

					builder.ins().brnz(result, exit_block, &[result]);
					builder.ins().jump(after_block, &[]);

					builder.seal_block(after_block);
					builder.switch_to_block(after_block);
				}
				Instruction::JumpRight => {
					let inner_block = builder.create_block();
					let after_block = builder.create_block();

					let pointer_value = builder.use_var(pointer);
					let cell_address = builder.ins().iadd(memory_address, pointer_value);
					let cell_value = builder.ins().load(I8, mem_flags, cell_address, 0);

					builder.ins().brz(cell_value, after_block, &[]);
					builder.ins().jump(inner_block, &[]);

					builder.switch_to_block(inner_block);

					stack.push((inner_block, after_block));
				}
				Instruction::JumpLeft => {
					let Some((inner_block, after_block)) = stack.pop() else {
						return Err(RunError::UnbalancedBrackets(']', i));
					};

					let pointer_value = builder.use_var(pointer);
					let cell_address = builder.ins().iadd(memory_address, pointer_value);
					let cell_value = builder.ins().load(I8, mem_flags, cell_address, 0);

					builder.ins().brnz(cell_value, inner_block, &[]);
					builder.ins().jump(after_block, &[]);

					builder.seal_block(inner_block);
					builder.seal_block(after_block);

					builder.switch_to_block(after_block);
				}
				Instruction::Clear => {
					let pointer_value = builder.use_var(pointer);
					let cell_address =builder.ins().iadd(memory_address, pointer_value);
					builder.ins().store(mem_flags, zero_byte, cell_address, 0);
				}
				Instruction::AddTo(n) => {
					let n = i64::from(n);
					let pointer_value = builder.use_var(pointer);
					let to_add = builder.ins().iadd_imm(pointer_value, n);

					let to_add = if n > 0 {
						let wrapped = builder.ins().iadd_imm(pointer_value, n - 30_000);
						let cmp = builder.ins().icmp_imm(IntCC::SignedLessThan, to_add, 30_000);
						builder.ins().select(cmp, to_add, wrapped)
					} else {
						let wrapped =  builder.ins().iadd_imm(pointer_value, n + 30_000);
						let cmp = builder.ins().icmp_imm(IntCC::SignedLessThan, to_add, 0);
						builder.ins().select(cmp, wrapped,to_add)
					};

					let from_address = builder.ins().iadd(memory_address, pointer_value);
					let to_address = builder.ins().iadd(memory_address, to_add);

					let from_value = builder.ins().load(I8, mem_flags, from_address, 0);
					let to_value = builder.ins().load(I8, mem_flags, to_address, 0);

					let sum = builder.ins().iadd(to_value, from_value);

					builder.ins().store(mem_flags, zero_byte, from_address, 0);
					builder.ins().store(mem_flags, sum, to_address, 0);
				}
			}
		}

		if !stack.is_empty() {
			return Err(RunError::UnbalancedBrackets(']', source.len()));
		}

		builder.ins().return_(&[zero]);

		builder.switch_to_block(exit_block);
		builder.seal_block(exit_block);

		let result = builder.block_params(exit_block)[0];
		builder.ins().return_(&[result]);

		builder.finalize();

		let res = verify_function(&func, &*isa);

		if clir {
			println!("{}", func.display());
		}

		if let Err(errors) = res {
			panic!("{errors}");
		}

		let mut ctx = Context::for_function(func);
		let code = match ctx.compile(&*isa) {
			Ok(x) => x,
			Err(e) => {
				eprintln!("error compiling: {e:?}");
				if clir {
					println!("{}", ctx.func.display());
				}
				std::process::exit(4);
			}
		};

		let code = code.code_buffer().to_vec();

		if clir {
			println!("{}", ctx.func.display());
		}

		Ok(Self {
			code,
			memory: [0; 30_000],
		})
	}

	#[allow(clippy::match_on_vec_items)]
	pub fn run(&mut self) -> io::Result<()> {
		let mut buffer = memmap2::MmapOptions::new().len(self.code.len()).map_anon().unwrap();

		buffer.copy_from_slice(self.code.as_slice());

		let buffer = buffer.make_exec().unwrap();

		unsafe {
			let code_fn: unsafe extern "C" fn (*mut u8) -> *mut io::Error = std::mem::transmute(buffer.as_ptr());

			let error = code_fn(self.memory.as_mut_ptr());

			if !error.is_null() {
				return Err(*Box::from_raw(error));
			}
		}

		Ok(())
	}
}

#[derive(Debug)]
pub enum RunError {
	UnbalancedBrackets(char, usize),
}

extern "sysv64" fn write(value: u8) -> *mut io::Error {
	if cfg!(target_os = "windows") && value >= 128 {
		return ptr::null_mut();
	}

	let mut stdout = io::stdout().lock();

	let result = stdout.write_all(&[value]).and_then(|()| stdout.flush());

	match result {
		Err(err) => Box::into_raw(Box::new(err)),
		_ => ptr::null_mut(),
	}
}

unsafe extern "sysv64" fn read(buf: *mut u8) -> *mut io::Error {
	let mut stdin = io::stdin().lock();
	loop {
		let mut value = 0;
		let err = stdin.read_exact(std::slice::from_mut(&mut value));

		if let Err(err) = err {
			if err.kind() != io::ErrorKind::UnexpectedEof {
				return Box::into_raw(Box::new(err));
			}
			value = 0;
		}

		if cfg!(target_os = "windows") && value == b'\r' {
			continue;
		}

		*buf = value;

		return ptr::null_mut();
	}
}
