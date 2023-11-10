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
	clippy::items_after_statements
)]
#![cfg_attr(
	docsrs,
	feature(doc_auto_cfg, doc_cfg),
	deny(rustdoc::broken_intra_doc_links)
)]

use std::io::{Read, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instruction {
	Add(u8),
	Move(isize),
	Input,
	Output,
	JumpRight(usize),
	JumpLeft(usize),
	Clear,
	AddTo(isize),
	MoveUntil(isize),
}

#[derive(Debug, Default)]
#[cfg(feature = "profile")]
pub struct Profile {
	pub add: u64,
	pub mov: u64,
	pub jr: u64,
	pub jl: u64,
	pub inp: u64,
	pub out: u64,
	pub clear: u64,
	pub addto: u64,
	pub movuntil: u64,
	pub loops: std::collections::HashMap<std::ops::Range<usize>, usize>,
}

pub struct Program {
	pub program_counter: usize,
	pub pointer: usize,
	pub instructions: Vec<Instruction>,
	pub memory: [u8; 30_000],
	#[cfg(feature = "profile")]
	pub profile: Profile,
}

impl Program {
	pub fn new(source: &[u8]) -> Result<Self, RunError> {
		let mut instructions = Vec::new();
		let mut bracket_stack = Vec::new();

		for b in source {
			let instruction = match b {
				b'+' | b'-' => {
					let inc = if *b == b'+' { 1 } else { 1u8.wrapping_neg() };
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
				b'[' => {
					let curr_address = instructions.len();
					bracket_stack.push(curr_address);
					Instruction::JumpRight(0)
				}
				b']' => {
					let curr_address = instructions.len();
					match bracket_stack.pop() {
						Some(pair_address) => {
							instructions[pair_address] = Instruction::JumpRight(curr_address);

							use Instruction::*;
							match instructions.as_slice() {
								[.., JumpRight(_), Add(n)] if n % 2 == 1 => {
									let len = instructions.len();
									instructions.drain(len - 2..);
									Instruction::Clear
								}
								&[.., JumpRight(_), Add(255), Move(x), Add(1), Move(y)]
									if x == -y =>
								{
									let len = instructions.len();
									instructions.drain(len - 5..);
									Instruction::AddTo(x)
								}
								&[.., JumpRight(_), Move(n)] => {
									let len = instructions.len();
									instructions.drain(len - 2..);
									Instruction::MoveUntil(n)
								}
								_ => Instruction::JumpLeft(pair_address),
							}
						}
						None => return Err(RunError::UnbalancedBrackets(']', curr_address)),
					}
				}
				_ => continue,
			};
			instructions.push(instruction);
		}

		if let Some(unpaired_bracket) = bracket_stack.pop() {
			return Err(RunError::UnbalancedBrackets('[', unpaired_bracket));
		}

		Ok(Self {
			program_counter: 0,
			pointer: 0,
			instructions,
			memory: [0; 30_000],
			#[cfg(feature = "profile")]
			profile: Profile::default(),
		})
	}

	#[allow(clippy::match_on_vec_items)]
	pub fn run(&mut self) -> std::io::Result<()> {
		let mut stdout = std::io::stdout().lock();
		let mut stdin = std::io::stdin().lock();
		'program: loop {
			use Instruction::*;

			#[allow(clippy::range_plus_one)]
			#[cfg(feature = "profile")]
			{
				match self.instructions[self.program_counter] {
					Add(_) => self.profile.add += 1,
					Output => self.profile.out += 1,
					Input => self.profile.inp += 1,
					Move(_) => self.profile.mov += 1,
					JumpRight(_) => self.profile.jr += 1,
					Clear => self.profile.clear += 1,
					AddTo(_) => self.profile.addto += 1,
					JumpLeft(pair) => {
						self.profile.jl += 1;
						*self
							.profile
							.loops
							.entry(pair..self.program_counter + 1)
							.or_default() += 1;
					}
					MoveUntil(_) => self.profile.movuntil += 1,
				}
			}

			match self.instructions[self.program_counter] {
				Add(n) => {
					self.memory[self.pointer] = self.memory[self.pointer].wrapping_add(n);
				}
				Output => {
					let value = self.memory[self.pointer];
					if !cfg!(target_os = "windows") || value < 128 {
						stdout.write_all(&[value])?;
						stdout.flush()?;
					}
				}
				Input => loop {
					let err = stdin.read_exact(&mut self.memory[self.pointer..=self.pointer]);
					match err.as_ref().map_err(std::io::Error::kind) {
						Err(std::io::ErrorKind::UnexpectedEof) => {
							self.memory[self.pointer] = 0;
						}
						_ => err?,
					}
					if cfg!(target_os = "windows") && self.memory[self.pointer] == b'\r' {
						continue;
					}
					break;
				},
				Move(n) => {
					let len = self.memory.len() as isize;
					let n = (len + n % len) as usize;
					self.pointer = (self.pointer + n) % len as usize;
				}
				JumpRight(pair_address) => {
					if self.memory[self.pointer] == 0 {
						self.program_counter = pair_address;
					}
				}
				JumpLeft(pair_address) => {
					if self.memory[self.pointer] != 0 {
						self.program_counter = pair_address;
					}
				}
				Clear => self.memory[self.pointer] = 0,
				AddTo(n) => {
					let len = self.memory.len() as isize;
					let n = (len + n % len) as usize;
					let to = (self.pointer + n) % len as usize;

					self.memory[to] = self.memory[to].wrapping_add(self.memory[self.pointer]);
					self.memory[self.pointer] = 0;
				}
				MoveUntil(n) => {
					let len = self.memory.len() as isize;
					let n = (len + n % len) as usize;
					loop {
						if self.memory[self.pointer] == 0 {
							break;
						}

						self.pointer = (self.pointer + n) % len as usize;
					}
				}
			}
			self.program_counter += 1;

			if self.instructions.len() == self.program_counter {
				break 'program;
			}
		}

		Ok(())
	}
}

#[derive(Debug)]
pub enum RunError {
	UnbalancedBrackets(char, usize),
}
