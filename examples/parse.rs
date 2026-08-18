#![allow(missing_docs)]

use pysyn::{parse_module, printer};

fn main() -> Result<(), pysyn::ParseError> {
    let module = parse_module("answer = 40 + 2\n")?;
    println!("{}", printer::unparse(&module));
    Ok(())
}
