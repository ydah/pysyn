#![no_main]

use libfuzzer_sys::fuzz_target;

fn source_from_bytes(data: &[u8]) -> String {
    let mut source = String::new();
    for byte in data.iter().copied().take(256) {
        match byte % 8 {
            0 => source.push_str("name"),
            1 => source.push_str(" = "),
            2 => source.push_str(&(byte as usize % 100).to_string()),
            3 => source.push_str(" + "),
            4 => source.push_str("if value:\n    "),
            5 => source.push_str("pass\n"),
            6 => source.push('('),
            _ => source.push(')'),
        }
    }
    source
}

fuzz_target!(|data: &[u8]| {
    let source = source_from_bytes(data);
    let _ = pysyn::parse(
        &source,
        pysyn::ParseOptions {
            parse_mode: pysyn::ParseMode::Recover,
            max_depth: 64,
            max_nodes: 10_000,
            ..pysyn::ParseOptions::default()
        },
    );
});
