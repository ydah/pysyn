#![allow(missing_docs)]

use std::env;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn pysyn_binary() -> PathBuf {
    if let Some(path) = env::var_os("CARGO_BIN_EXE_pysyn") {
        return path.into();
    }
    let target_dir = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target"));
    let executable = if cfg!(windows) { "pysyn.exe" } else { "pysyn" };
    target_dir.join("debug").join(executable)
}

fn check_rejects(source: &str) -> bool {
    let mut child = Command::new(pysyn_binary())
        .args(["check", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn pysyn");
    child.stdin.as_mut().expect("stdin").write_all(source.as_bytes()).expect("write source");
    !child.wait().expect("wait for pysyn").success()
}

#[test]
fn rejects_compiler_error_cases_from_test_syntax() {
    let cases = [
        "del __debug__\n",
        "match ...:\n    case {**rest, \"key\": value}:\n       ...\n",
        "match ...:\n    case {**_}:\n       ...\n",
        "def foo(a,/,/,b,c):\n   pass\n",
        "def foo(a,*b,c,/,d,e):\n   pass\n",
        "def foo(a,*a, b, **c, *d):\n   pass\n",
        "lambda /,a,b,c: None\n",
        "lambda a,d=3,c: None\n",
        "f(x for x in L, 1)\n",
        "f(L, x for x in L)\n",
        "f((x)=2)\n",
        "f(__debug__=1)\n",
        "f(a=23, a=234)\n",
        "(): int\n",
        "[]: int\n",
        "3 + not 3\n",
        "- not 3\n",
        "3 + - not 3\n",
        "3 + not -1\n",
    ];
    for source in cases {
        assert!(check_rejects(source), "pysyn accepted invalid source: {source:?}");
    }
}
