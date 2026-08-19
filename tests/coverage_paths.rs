#![allow(missing_docs)]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use pysyn::parser::{parse, ParseMode, ParseOptions};
use pysyn::printer::{dump, dump_with_source, unparse, DumpOptions};
use pysyn::source::SourceFile;
use pysyn::validate::{validate, ValidateLevel};
use pysyn::visit::preorder;

fn collect_python_files(path: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_python_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "py") {
            files.push(path);
        }
    }
}

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

#[test]
fn exercises_stdlib_when_a_corpus_is_configured() {
    let Some(root) = env::var_os("PYSYN_COVERAGE_STDLIB") else {
        eprintln!(
            "coverage corpus not configured; set PYSYN_COVERAGE_STDLIB to exercise stdlib paths"
        );
        return;
    };
    let root = PathBuf::from(root);
    assert!(root.is_dir(), "configured corpus is not a directory: {}", root.display());

    let mut files = Vec::new();
    collect_python_files(&root, &mut files);
    files.sort();
    assert!(files.len() >= 1_000, "expected a Python standard library corpus");

    let mut decoded = 0usize;
    let mut nodes = 0usize;
    for path in files {
        let Ok(bytes) = fs::read(&path) else { continue };
        let Ok(source) = SourceFile::from_bytes(path.to_string_lossy(), &bytes) else { continue };
        decoded += 1;
        let parsed = parse(
            source.text(),
            ParseOptions {
                parse_mode: ParseMode::Recover,
                keep_tokens: true,
                type_comments: true,
                ..ParseOptions::default()
            },
        );
        let module = &parsed.module;
        nodes += preorder(module).count();
        let _ = dump(module, DumpOptions::default());
        let _ = dump(module, DumpOptions::with_attributes(false).with_indent(Some(2)));
        let _ = dump_with_source(module, source.text(), DumpOptions::default());
        let _ = unparse(module);
        let _ = validate(module, ValidateLevel::Syntax);
        let _ = validate(module, ValidateLevel::Semantic);
    }

    assert!(decoded >= 1_000, "expected decoded Python files");
    assert!(nodes >= 10_000, "expected a non-trivial AST corpus");
}

fn run_cli(arguments: &[&str], input: &str) -> Output {
    let mut child = Command::new(pysyn_binary())
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pysyn");
    std::io::Write::write_all(child.stdin.as_mut().expect("stdin"), input.as_bytes())
        .expect("write stdin");
    child.wait_with_output().expect("wait for pysyn")
}

#[test]
fn exercises_cli_success_and_failure_paths() {
    assert!(run_cli(&["--version"], "").status.success());
    assert!(run_cli(&["check", "-"], "value = 1\n").status.success());
    assert!(run_cli(&["parse", "-"], "value = 1\n").status.success());
    assert!(run_cli(&["parse", "--target-version=3.11", "-"], "value = f\"{name}\"\n")
        .status
        .success());
    assert!(run_cli(&["dump", "--no-attributes", "-"], "value = 1\n").status.success());
    assert!(run_cli(&["unparse", "-"], "value = 1\n").status.success());
    assert!(run_cli(&["tokenize", "--format=cpython", "-"], "value = 1\n").status.success());
    let legacy_tokens = run_cli(
        &["tokenize", "--target-version=3.11", "--format=cpython", "-"],
        "value = f\"{name}\"\n",
    );
    assert!(legacy_tokens.status.success());
    assert!(String::from_utf8_lossy(&legacy_tokens.stdout).contains("3 (1, 8) (1, 17)"));
    assert!(!run_cli(&["check", "-"], "value =\n").status.success());
    assert!(!run_cli(&["unknown", "-"], "").status.success());
}
