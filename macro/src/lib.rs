mod code_match;
mod mangle;
mod parse;
mod util;
mod tests;
mod buildcode;

use crate::buildcode::FFIBuilder;
use std::collections::HashSet;
use proc_macro::{TokenStream as TS0, TokenTree};
use std::env;
use regex::Regex;
use syn;


extern "C" {
	fn enable_msvc_debug_c();
}

#[proc_macro_attribute]
pub fn bridge(args: TS0, input: TS0) -> TS0 {
	let mut flags = HashSet::new();
	for tt in args.into_iter() {
		if let TokenTree::Ident(val) = tt {
			flags.insert(val.to_string());
		}
	}
	let mut bb = FFIBuilder::new(! flags.contains("goon") );
	match bb.build_bridge_code(input.into()) {
		Ok(code) => code.into(),
		Err(e) => syn::Error::new(proc_macro2::Span::call_site(), e).to_compile_error().into()
	}
}

#[proc_macro_attribute]
pub fn enable_msvc_debug(args: TS0, _input: TS0) -> TS0
{
	let enable_ = if let Some(TokenTree::Ident(val)) = args.into_iter().next() {
		val.to_string().parse::<i32>().unwrap_or(-1)
	} else {
		-1
	};
	let is_debug = match enable_ {
		0 => false,
		1 => true,
		_ => {
			match env::var("OUT_DIR") {
				Ok(profile) => Regex::new(r"[\\/]target[\\/]debug[\\/]").unwrap().is_match(&profile),
				Err(_) => false,
			}
		}
	};
	if is_debug {
		unsafe{ enable_msvc_debug_c(); }
	}
	TS0::new()
}
