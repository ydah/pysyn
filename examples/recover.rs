#![allow(missing_docs)]

use pysyn::parser::{parse, ParseMode, ParseOptions};

fn main() {
    let parsed = parse(
        "def broken(:\n    pass\n",
        ParseOptions { parse_mode: ParseMode::Recover, ..ParseOptions::default() },
    );
    for error in parsed.errors {
        eprintln!("{error}");
    }
}
