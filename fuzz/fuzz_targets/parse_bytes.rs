#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let source = String::from_utf8_lossy(data);
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
