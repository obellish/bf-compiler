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
	clippy::similar_names
)]
#![cfg_attr(
	docsrs,
	feature(doc_auto_cfg, doc_cfg),
	deny(rustdoc::broken_intra_doc_links)
)]

use std::{env, fs, process::ExitCode};

use bf_compiler::{Instruction, Program, RunError};

#[allow(clippy::match_on_vec_items)]
fn main() -> ExitCode {
	let mut args = env::args();
	if args.len() != 2 {
		eprintln!("expected a single file path as argument");
		return ExitCode::from(1);
	}

	let file_name = args.nth(1).unwrap();
	let source = match fs::read(&file_name) {
		Ok(x) => x,
		Err(e) => {
			eprintln!("Error reading '{file_name}': {e}");
			return ExitCode::from(2);
		}
	};

	let mut program = match Program::new(&source) {
		Ok(x) => x,
		Err(RunError::UnbalancedBrackets(c, address)) => {
			eprintln!(
				"Error parsing file: didn't find pair for `{c}` at instruction index {address}"
			);
			return ExitCode::from(3);
		}
	};
	if let Err(err) = program.run() {
		eprintln!("IO error: {err}");
	}

	#[cfg(feature = "profile")]
	{
		let profile = std::mem::take(&mut program.profile);
		println!("profile:");
		println!(" +: {}", profile.add);
		println!(" >: {}", profile.mov);
		println!(" [: {}", profile.jr);
		println!(" ]: {}", profile.jl);
		println!(" .: {}", profile.out);
		println!(" ,: {}", profile.inp);
		println!(" x: {}", profile.clear);
		println!("+>: {}", profile.addto);
		println!(">>: {}", profile.movuntil);
		println!("loops:");

		let to_string = |range: std::ops::Range<usize>| {
			program.instructions[range]
				.iter()
				.map(|x| match x {
					Instruction::Add(n) => {
						if *n >= 128 {
							format!("-{}", n.wrapping_neg())
						} else {
							format!("+{n}")
						}
					}
					Instruction::Move(n) => {
						if *n < 0 {
							format!("<{}", -n)
						} else {
							format!(">{n}")
						}
					}
					Instruction::Input => ",".to_owned(),
					Instruction::Output => ".".to_owned(),
					Instruction::JumpRight(_) => "[".to_owned(),
					Instruction::JumpLeft(_) => "]".to_owned(),
					Instruction::Clear => "x".to_owned(),
					Instruction::AddTo(n) => {
						if *n < 0 {
							format!("+<{}", -n)
						} else {
							format!("+>{n}")
						}
					}
					Instruction::MoveUntil(n) => {
						if *n < 0 {
							format!("<<{}", -n)
						} else {
							format!(">>{n}")
						}
					}
				})
				.fold(String::new(), |a, b| a + &b)
		};

		let mut loops: Vec<_> = profile
			.loops
			.into_iter()
			.map(|(range, count)| (to_string(range), count))
			.collect();

		loops.sort_by(|(a, _), (b, _)| a.cmp(b));

		for i in 1..loops.len() {
			if loops[i - 1].0 == loops[i].0 {
				loops[i].1 += loops[i - 1].1;
				loops[i - 1].1 = 0;
			}
		}

		loops.retain(|x| x.1 > 0);

		loops.sort_by_key(|x| x.1);

		for (code, count) in loops.into_iter().rev().take(20) {
			println!("{count:10}: {code}");
		}
	}

	ExitCode::from(0)
}
