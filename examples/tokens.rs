#![allow(missing_docs)]

use pysyn::lexer::tokenize;

fn main() {
    for token in tokenize("if ready:\n    run()\n") {
        println!("{token:?}");
    }
}
