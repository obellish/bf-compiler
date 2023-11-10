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

use std::{
	error::Error,
	fmt::{Display, Formatter, Result as FmtResult, Write as _},
	io::{Error as IoError, Write as _},
};

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

pub struct Program {
	pub code: Vec<u8>,
	pub memory: [u8; 30_000],
}

impl Program {
	pub fn new(source: &[u8]) -> Result<Self, RunError> {
		let mut code = Vec::new();
		let mut bracket_stack = Vec::new();

		code.write_all(&[
			0x55, 0x48, 0x89, 0xe5, 0x41, 0x54, 0x41, 0x55, 0x49, 0x89, 0xfc, 0x4d, 0x31, 0xed,
		])?;

		for b in source {
			match b {
				b'+' => code.write_all(&[0x43, 0x80, 0x04, 0x2c, 0x01])?,
				b'-' => code.write_all(&[0x43, 0x80, 0x04, 0x2c, 0xff])?,
				b'.' => code.write_all(&[
					0xb8, 0x01, 0x00, 0x00, 0x00, 0xbf, 0x01, 0x00, 0x00, 0x00, 0x4b, 0x8d, 0x34,
					0x2c, 0xba, 0x01, 0x00, 0x00, 0x00, 0x0f, 0x05,
				])?,
				b',' => code.write_all(&[
					0xb8, 0x00, 0x00, 0x00, 0x00, 0xbf, 0x00, 0x00, 0x00, 0x00, 0x4b, 0x8d, 0x34,
					0x2c, 0xba, 0x01, 0x00, 0x00, 0x00, 0x0f, 0x05,
				])?,
				b'<' => code.write_all(&[
					0x49, 0x83, 0xed, 0x01, 0xb8, 0x2f, 0x75, 0x00, 0x00, 0x4c, 0x0f, 0x42, 0xe8,
				])?,
				b'>' => code.write_all(&[
					0x49, 0x83, 0xc5, 0x01, 0x31, 0xc0, 0x49, 0x81, 0xfd, 0x30, 0x75, 0x00, 0x00,
					0x4c, 0x0f, 0x44, 0xe8,
				])?,
				b'[' => {
					code.write_all(&[
						0x43, 0x80, 0x3c, 0x2c, 0x00, 0x0f, 0x84, 0x00, 0x00, 0x00, 0x00,
					])?;

					bracket_stack.push(code.len() as u32);
				}
				b']' => {
					let left = match bracket_stack.pop() {
						Some(x) => x as usize,
						None => return Err(RunError::UnbalancedBrackets(']', code.len()))
					};

					code.write_all(&[0x43, 0x80, 0x3c, 0x2c, 0x00, 0x0f, 0x85, 0xf1, 0xff, 0xff, 0xff])?;

					let right = code.len();

					let offset = right as i32 - left as i32;

					code[left - 4..left].copy_from_slice(&offset.to_le_bytes());
					code[right - 4..right].copy_from_slice(&(-offset).to_le_bytes());
				},
				_ => continue,
			}
		}

		code.write_all(&[0x41, 0x5d, 0x41, 0x5c, 0x5d, 0xc3])?;

		Ok(Self {
			code,
			memory: [0; 30_000],
		})
	}

	#[allow(clippy::match_on_vec_items)]
	pub fn run(&mut self) -> std::io::Result<()> {
		unsafe {
			let len = self.code.len();
			let mem = libc::mmap();
		}

		Ok(())
	}
}

#[derive(Debug)]
pub enum RunError {
	Io(IoError),
	UnbalancedBrackets(char, usize),
}

impl Display for RunError {
	fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
		match self {
			Self::Io(e) => Display::fmt(&e, f),
			Self::UnbalancedBrackets(bracket, loc) => {
				f.write_str("unmatched bracket ")?;
				f.write_char(*bracket)?;
				f.write_str(" at location ")?;
				Display::fmt(&loc, f)
			}
		}
	}
}

impl Error for RunError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		match self {
			Self::Io(e) => Some(e),
			Self::UnbalancedBrackets(..) => None,
		}
	}
}

impl From<IoError> for RunError {
	fn from(value: IoError) -> Self {
		Self::Io(value)
	}
}
