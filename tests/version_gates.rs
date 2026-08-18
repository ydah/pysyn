#![allow(missing_docs)]

use pysyn::error::{DiagnosticCode, Severity};
use pysyn::parser::{parse, ParseOptions, Parsed};
use pysyn::token::PythonVersion;

fn parse_at_version(source: &str, version: PythonVersion) -> Parsed {
    parse(source, ParseOptions { version, ..ParseOptions::default() })
}

fn assert_unsupported(source: &str, version: PythonVersion) {
    let parsed = parse_at_version(source, version);
    assert!(
        parsed.errors.iter().any(|diagnostic| {
            diagnostic.code == DiagnosticCode::UnsupportedSyntax
                && diagnostic.severity == Severity::Unsupported
        }),
        "expected UnsupportedSyntax for Python {:?}, got {:?}",
        version,
        parsed.errors
    );
}

fn assert_supported(source: &str, version: PythonVersion) {
    let parsed = parse_at_version(source, version);
    assert!(
        parsed.errors.is_empty(),
        "expected no diagnostics for Python {:?}, got {:?}",
        version,
        parsed.errors
    );
}

fn assert_supported_from_py312(source: &str) {
    for version in [PythonVersion::Py312, PythonVersion::Py313] {
        assert_supported(source, version);
    }
}

#[test]
fn parenthesized_multiple_with_requires_python_310() {
    let source = "with (first() as left, second() as right):\n    pass\n";

    assert_unsupported(source, PythonVersion::Py39);
    assert_supported_from_py312(source);
}

#[test]
fn type_statement_requires_python_312() {
    let source = "type Alias = int\n";

    assert_unsupported(source, PythonVersion::Py311);
    assert_supported_from_py312(source);
}

#[test]
fn pep701_fstring_forms_require_python_312() {
    let same_quote = r#"value = f"{value["key"]}""#.to_owned() + "\n";
    let backslash = r#"value = f"{'\n'.join(items)}""#.to_owned() + "\n";
    let comment = "value = f\"\"\"{value # comment\n}\"\"\"\n";

    for source in [same_quote.as_str(), backslash.as_str(), comment] {
        assert_unsupported(source, PythonVersion::Py311);
        assert_supported_from_py312(source);
    }
}
