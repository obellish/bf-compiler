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

use std::{env, process::ExitCode};

use bf_compiler::{Program, RunError};

#[allow(clippy::match_on_vec_items)]
fn main() -> ExitCode {
	let mut args = env::args();

	let mut dump = None;
	let mut source = None;
	let mut clir = false;
	while let Some(arg) = args.next() {
		match arg.as_str() {
			"-d" | "--dump" => {
				dump = args.next();
				assert!(dump.is_some());
			}
			"--CLIR" => {
				clir = true;
			}
			_ => source = Some(arg),
		}
	}

	let Some(source) = source else {
		eprintln!("expected a file path as an argument");
		return ExitCode::from(1);
	};

	let source = match std::fs::read(&source) {
		Ok(x) => x,
		Err(err) => {
			eprintln!("Error reading '{source}': {err}");
			return ExitCode::from(2);
		}
	};

	let mut program = match Program::new(&source, clir) {
		Ok(x) => x,
		Err(RunError::UnbalancedBrackets(c, address)) => {
			eprintln!("Error parsing file: didn't find pair for `{c}` at byte index {address}");
			return ExitCode::from(3);
		}
	};

	if let Some(dump) = &dump {
		std::fs::write(dump, program.code.as_slice()).unwrap();
	}

	if dump.is_some() || clir {
		return ExitCode::from(0);
	}

	if let Err(err) = program.run() {
		eprintln!("IO error: {err}");
	}

	ExitCode::from(0)
}
