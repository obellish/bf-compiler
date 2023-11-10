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

use bf_compiler::{Program, RunError};

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
		Err(RunError::Io(e)) => {
			eprintln!("IO error: {e}");
			return ExitCode::from(3);
		}
	};
	if let Err(err) = program.run() {
		eprintln!("IO error: {err}");
	}

	ExitCode::from(0)
}
