#![allow(missing_docs)]

use std::mem::size_of;

use pysyn::ast::{Expr, ExprContext, Stmt};
use pysyn::error::Severity;
use pysyn::lexer::{tokenize_with, LexMode, LexOptions};
use pysyn::parser::{parse, ParseMode, ParseOptions};
use pysyn::token::{PythonVersion, TokenKind};
use pysyn::validate::{validate, ValidateLevel};

#[test]
fn ast_enums_remain_compact() {
    assert!(size_of::<Expr>() <= 64, "Expr is {} bytes", size_of::<Expr>());
    assert!(size_of::<Stmt>() <= 96, "Stmt is {} bytes", size_of::<Stmt>());
}

#[test]
fn tokenizes_keywords_soft_keywords_and_indentation() {
    let source = "if match:\n\tvalue = 0x2a\n";
    let tokens =
        tokenize_with(source, LexOptions { mode: LexMode::Parse, version: PythonVersion::Py313 })
            .filter_map(Result::ok)
            .map(|token| token.kind)
            .collect::<Vec<_>>();
    assert_eq!(tokens[0], TokenKind::If);
    assert_eq!(tokens[1], TokenKind::Name);
    assert!(tokens.contains(&TokenKind::Indent));
    assert!(tokens.contains(&TokenKind::Dedent));
}

#[test]
fn parses_core_statements_and_marks_assignment_targets() {
    let module = pysyn::parse_module("x, y = 1e-3, 42\nfor item in values:\n    item += 1\n")
        .expect("valid source");
    let Stmt::Assign(assign) = &module.body[0] else { panic!("expected assignment") };
    let Expr::Tuple(targets) = &assign.targets[0] else { panic!("expected tuple target") };
    assert!(targets
        .elts
        .iter()
        .all(|expr| matches!(expr, Expr::Name(node) if node.ctx == ExprContext::Store)));
    let Stmt::For(for_stmt) = &module.body[1] else { panic!("expected for") };
    assert!(matches!(&*for_stmt.target, Expr::Name(node) if node.ctx == ExprContext::Store));
}

#[test]
fn recovers_invalid_source_without_panicking() {
    let parsed = parse(
        "def broken(:\n    pass\nnext = 1\n",
        ParseOptions { parse_mode: ParseMode::Recover, ..ParseOptions::default() },
    );
    assert!(!parsed.errors.is_empty());
    assert!(!parsed.module.body.is_empty());
}

#[test]
fn keeps_comments_as_side_table() {
    let parsed = parse("# header\nx = 1  # inline\n", ParseOptions::default());
    assert_eq!(parsed.comments.len(), 2);
}

#[test]
fn preserves_enabled_type_comments_and_invalid_escape_warnings() {
    let source = "x = 1  # type: int\nfor item in values:  # type: str\n    pass\n".to_owned()
        + "def convert(value):  # type: (int) -> str\n    return value\n";
    let parsed = parse(&source, ParseOptions { type_comments: true, ..ParseOptions::default() });
    let Stmt::Assign(assign) = &parsed.module.body[0] else { panic!("expected assignment") };
    assert_eq!(assign.type_comment.as_deref(), Some("int"));
    let Stmt::For(for_stmt) = &parsed.module.body[1] else { panic!("expected for") };
    assert_eq!(for_stmt.type_comment.as_deref(), Some("str"));
    let Stmt::FunctionDef(function) = &parsed.module.body[2] else { panic!("expected function") };
    assert_eq!(function.type_comment.as_deref(), Some("(int) -> str"));
    assert!(!parsed.errors.iter().any(|error| error.severity == Severity::Error));

    let parsed = parse("value = \"\\q\"\n", ParseOptions::default());
    assert!(parsed.errors.iter().any(|error| {
        error.code == pysyn::DiagnosticCode::InvalidEscape && error.severity == Severity::Warning
    }));
    let parsed = parse(&(r#"value = f"\q{x}""#.to_owned() + "\n"), ParseOptions::default());
    assert!(parsed.errors.iter().any(|error| {
        error.code == pysyn::DiagnosticCode::InvalidEscape && error.severity == Severity::Warning
    }));
    let parsed = parse("value = \"\\400\"\n", ParseOptions::default());
    assert!(parsed.errors.iter().any(|error| {
        error.code == pysyn::DiagnosticCode::InvalidEscape && error.severity == Severity::Warning
    }));
    let parsed = parse("value = r\"\\q\"\n", ParseOptions::default());
    assert!(!parsed.errors.iter().any(|error| error.code == pysyn::DiagnosticCode::InvalidEscape));
}

#[test]
fn preserves_type_ignore_comments_when_type_comments_are_enabled() {
    let parsed = parse(
        "# type: ignore\nvalue = 1  # type: ignore[assignment]\n",
        ParseOptions { type_comments: true, ..ParseOptions::default() },
    );
    assert_eq!(parsed.module.type_ignores.len(), 2);
    assert_eq!(parsed.module.type_ignores[0].lineno, 1);
    assert_eq!(parsed.module.type_ignores[0].tag.as_ref(), "\n");
    assert_eq!(parsed.module.type_ignores[1].tag.as_ref(), "[assignment]");
    let dump = pysyn::printer::dump(&parsed.module, Default::default());
    assert!(dump.contains("TypeIgnore(lineno=1, tag='\\n')"));
}

#[test]
fn dump_includes_source_locations() {
    let module = pysyn::parse_module("value = 1\n").expect("valid source");
    let dump = pysyn::printer::dump(&module, Default::default());
    assert!(dump.contains("Assign(targets=[Name(id='value'"));
    assert!(dump.contains("lineno=1, col_offset=0, end_lineno=1, end_col_offset=9"));
    assert!(dump.contains("Constant(value=1"));

    let pretty = pysyn::printer::dump(
        &module,
        pysyn::printer::DumpOptions::with_attributes(false).with_indent(Some(2)),
    );
    assert!(pretty.contains('\n'));
}

#[test]
fn tokenizes_and_parses_nested_f_strings() {
    let source = "f\"a{x!r:>{width}}b\"";
    let tokens = pysyn::lexer::tokenize(source).filter_map(Result::ok).collect::<Vec<_>>();
    assert!(matches!(tokens.first().map(|token| token.kind), Some(TokenKind::FStringStart { .. })));
    assert!(tokens.iter().any(|token| matches!(token.kind, TokenKind::FStringEnd { .. })));
    let module = pysyn::parse_module(&format!("value = {source}\n")).expect("valid f-string");
    let dump = pysyn::printer::dump(&module, Default::default());
    assert!(dump.contains("FormattedValue(value=Name"));
    assert!(dump.contains("conversion=114"));
}

#[test]
fn tokenizes_raw_fstring_fields_and_unicode_name_escapes() {
    let raw = r#"fr'\N{AMPERSAND}'"#;
    let kinds =
        tokenize_with(raw, LexOptions { mode: LexMode::Full, version: PythonVersion::Py313 })
            .filter_map(Result::ok)
            .map(|token| token.kind)
            .filter(|kind| {
                matches!(
                    kind,
                    TokenKind::FStringStart { .. }
                        | TokenKind::FStringMiddle
                        | TokenKind::FStringEnd { .. }
                        | TokenKind::LBrace
                        | TokenKind::RBrace
                        | TokenKind::Name
                )
            })
            .collect::<Vec<_>>();
    assert!(matches!(kinds[0], TokenKind::FStringStart { .. }));
    assert_eq!(kinds[1], TokenKind::FStringMiddle);
    assert_eq!(kinds[2], TokenKind::LBrace);
    assert_eq!(kinds[3], TokenKind::Name);
    assert_eq!(kinds[4], TokenKind::RBrace);
    assert!(matches!(kinds[5], TokenKind::FStringEnd { .. }));

    let unicode_name = r#"f'\N{AMPERSAND}3'"#;
    let middle_ranges = tokenize_with(
        unicode_name,
        LexOptions { mode: LexMode::Full, version: PythonVersion::Py313 },
    )
    .filter_map(Result::ok)
    .filter(|token| token.kind == TokenKind::FStringMiddle)
    .map(|token| token.range)
    .collect::<Vec<_>>();
    assert_eq!(middle_ranges.len(), 2);

    let module = pysyn::parse_module("value = fr'\\N{AMPERSAND}'\n")
        .expect("raw f-string field should parse");
    let dump = pysyn::printer::dump(&module, Default::default());
    assert!(dump.contains("Constant(value='\\\\N'"));
    assert!(dump.contains("FormattedValue(value=Name(id='AMPERSAND'"));
}

#[test]
fn parses_soft_keyword_statement_boundaries() {
    let source = "match value:\n    case 1:\n        result = 1\ntype Alias = list[int]\nmatch = 2\nvalue = type(thing)\n";
    let module = pysyn::parse_module(source).expect("valid soft-keyword source");
    assert!(matches!(module.body[0], Stmt::Match(_)));
    assert!(matches!(module.body[1], Stmt::TypeAlias(_)));
    assert!(matches!(module.body[2], Stmt::Assign(_)));
    assert!(matches!(module.body[3], Stmt::Assign(_)));
}

#[test]
fn parses_comprehensions_and_parameter_sections() {
    let module = pysyn::parse_module("def f(x: int = 1, /, *args, flag: bool = True, **kwargs):\n    return [x for x in args if x]\n").expect("valid function");
    let Stmt::FunctionDef(function) = &module.body[0] else { panic!("expected function") };
    assert_eq!(function.args.posonlyargs.len(), 1);
    assert_eq!(function.args.kwonlyargs.len(), 1);
    let Stmt::Return(return_stmt) = &function.body[0] else { panic!("expected return") };
    assert!(matches!(&**return_stmt.value.as_ref().expect("return value"), Expr::ListComp(_)));
}

#[test]
fn parses_multiline_and_delimited_constructs() {
    let source = concat!(
        "from package import (first as one, second,)\n",
        "class Config(**options):\n    pass\n",
        "def collect(*args: tuple[str, ...], **kwargs: dict[str, int]):\n",
        "    return args[0], kwargs\n",
        "for item, in values:\n    item += value,\n",
        "with (resource() as handle, other() as backup):\n    pass\n",
    );
    let module = pysyn::parse_module(source).expect("valid delimited source");
    assert!(matches!(module.body[0], Stmt::ImportFrom(_)));
    assert!(matches!(module.body[1], Stmt::ClassDef(_)));
    assert!(matches!(module.body[2], Stmt::FunctionDef(_)));
    assert!(matches!(module.body[3], Stmt::For(_)));
    assert!(matches!(module.body[4], Stmt::With(_)));
}

#[test]
fn parses_pep695_type_parameters() {
    let source = concat!(
        "def identity[T: Base = int, *Ts, **P](value: T) -> T:\n",
        "    return value\n",
        "class Box[T](Base):\n    pass\n",
        "type Alias[T = int] = list[T]\n",
    );
    let module = pysyn::parse_module(source).expect("valid type parameter source");
    let Stmt::FunctionDef(function) = &module.body[0] else { panic!("expected function") };
    assert_eq!(function.type_params.len(), 3);
    let Stmt::ClassDef(class) = &module.body[1] else { panic!("expected class") };
    assert_eq!(class.type_params.len(), 1);
    let Stmt::TypeAlias(alias) = &module.body[2] else { panic!("expected type alias") };
    assert_eq!(alias.type_params.len(), 1);
    let unparsed = pysyn::printer::unparse(&module);
    assert!(unparsed.contains("def identity[T: Base = int, *Ts, **P](value: T) -> T:"));
    assert!(unparsed.contains("type Alias[T = int] = list[T]"));
}

#[test]
fn builds_match_pattern_variants() {
    let source = concat!(
        "match value:\n",
        "    case {\"kind\": item, **rest}:\n        pass\n",
        "    case Point(x, y as point_y):\n        pass\n",
        "    case [*items]:\n        pass\n",
        "    case 0 | 1:\n        pass\n",
    );
    let module = pysyn::parse_module(source).expect("valid pattern source");
    let Stmt::Match(statement) = &module.body[0] else { panic!("expected match") };
    assert!(matches!(statement.cases[0].pattern, pysyn::ast::Pattern::Mapping(_)));
    assert!(matches!(statement.cases[1].pattern, pysyn::ast::Pattern::Class(_)));
    assert!(matches!(statement.cases[2].pattern, pysyn::ast::Pattern::Sequence(_)));
    assert!(matches!(statement.cases[3].pattern, pysyn::ast::Pattern::Or(_)));
}

#[test]
fn validates_nested_statement_contexts() {
    let module = pysyn::parse_module(
        "try:\n    return 1\nexcept Exception:\n    break\nnonlocal value\ndef ok():\n    return 1\n",
    )
    .expect("valid source for semantic validation");
    let diagnostics = validate(&module, ValidateLevel::Semantic);
    assert_eq!(diagnostics.len(), 3);
    assert!(diagnostics
        .iter()
        .all(|diagnostic| diagnostic.code == pysyn::DiagnosticCode::Validation));
}

#[test]
fn handles_raw_strings_and_backslash_continuations() {
    let source =
        concat!("value = r'[\\w!\"\\'&.,?]' \\\n", "other = first + \\\n", "    second\n",);
    assert!(pysyn::parse_module(source).is_ok());
}

#[test]
fn parses_fstring_literal_concatenation_and_inline_suites() {
    let source = concat!(
        "key = (rf\"Software\\\\Python\\\\{sys.winver}\" r\"\\\\Help\")\n",
        "if enabled: first = 1; second = 2\n",
    );
    let module = pysyn::parse_module(source).expect("valid f-string and suite source");
    let Stmt::If(statement) = &module.body[1] else { panic!("expected if") };
    assert_eq!(statement.body.len(), 2);
}

#[test]
fn preserves_python_expression_precedence_and_literal_kinds() {
    let source = "value = -number - 1\n".to_owned()
        + "grouped = (left == right) != other\n"
        + "mixed = \"prefix \" f\"value={name!r}\"\n"
        + "magic = 0x13579ace\n";
    let module = pysyn::parse_module(&source).expect("valid precedence source");
    let dump = pysyn::printer::dump(&module, Default::default());
    assert!(dump.contains("BinOp(left=UnaryOp(op=USub(), operand=Name(id='number'"));
    assert!(dump.contains("Compare(left=Compare(left=Name(id='left'"));
    assert!(dump.contains("JoinedStr(values=[Constant(value='prefix value='"));
    assert!(dump.contains("Constant(value=0x13579ace"));
}

#[test]
fn records_annotation_simple_flag_and_extended_patterns() {
    let source = concat!(
        "value: int = 1\n",
        "obj.value: int = 2\n",
        "match value:\n",
        "    case ():\n        pass\n",
        "    case 0, *rest:\n        pass\n",
    );
    let module = pysyn::parse_module(source).expect("valid annotation and pattern source");
    let Stmt::AnnAssign(simple) = &module.body[0] else { panic!("expected simple annotation") };
    assert!(simple.simple);
    let Stmt::AnnAssign(attribute) = &module.body[1] else {
        panic!("expected attribute annotation")
    };
    assert!(!attribute.simple);
    let Stmt::Match(statement) = &module.body[2] else { panic!("expected match") };
    assert!(matches!(statement.cases[0].pattern, pysyn::ast::Pattern::Sequence(_)));
    assert!(matches!(statement.cases[1].pattern, pysyn::ast::Pattern::Sequence(_)));
}

#[test]
fn accepts_pep701_fstring_forms_and_unicode_names() {
    let source = concat!(
        "debug = f'{value=}'\n",
        "comment = f'{value  # explain\n}'\n",
        "newline = f'{(value +\n 1)}'\n",
        "named = f'\\N{GREEK CAPITAL LETTER DELTA}'\n",
    );
    let module = pysyn::parse_module(source).expect("valid PEP 701 source");
    let dump = pysyn::printer::dump(&module, Default::default());
    assert!(dump.contains("conversion=114"));
    assert!(dump.contains("Constant(value='Δ'"));
}

#[test]
fn recovers_non_ascii_fstring_fields_without_panicking() {
    let parsed = parse(
        "F'😀{value}'\n",
        ParseOptions { parse_mode: ParseMode::Recover, ..ParseOptions::default() },
    );
    assert!(!parsed.module.body.is_empty());
}

#[test]
fn bounds_deep_input_without_stack_overflow() {
    let source = format!("value = {}\n", "1+".repeat(10_000) + "1");
    let parsed = parse(&source, ParseOptions { max_depth: 32, ..ParseOptions::default() });
    assert!(parsed.errors.iter().any(|error| error.code == pysyn::DiagnosticCode::TooDeep));
}

#[test]
fn detects_python_source_encoding_markers() {
    assert_eq!(
        pysyn::detect_encoding(b"# coding: cp932\n"),
        Ok(pysyn::SourceEncoding::Declared("cp932".into()))
    );
    assert_eq!(
        pysyn::detect_encoding(b"\xef\xbb\xbf# coding: utf-8\n"),
        Ok(pysyn::SourceEncoding::Utf8Bom)
    );
    assert!(pysyn::detect_encoding(b"\xef\xbb\xbf# coding: latin-1\n").is_err());
}

#[cfg(feature = "encoding")]
#[test]
fn decodes_declared_non_utf8_source() {
    let source =
        pysyn::SourceFile::from_bytes("cp932.py", b"# coding: cp932\n\x93\xfa\x96\x7b = 1\n")
            .expect("cp932 source should decode");
    assert!(source.text().contains("日本 = 1"));
}

#[test]
fn keeps_tokens_when_requested_and_formats_python_values() {
    let parsed =
        parse("value = 1e-5\n", ParseOptions { keep_tokens: true, ..ParseOptions::default() });
    assert!(!parsed.tokens.is_empty());
    assert_eq!(pysyn::printer::pyrepr_float(1e-5), "1e-05");
    assert_eq!(pysyn::printer::pyrepr("it's"), "\"it's\"");
}

#[cfg(feature = "nfkc")]
#[test]
fn normalizes_unicode_identifiers_for_ast_storage() {
    let module = pysyn::parse_module("ﬁ = 1\n").expect("valid identifier");
    let Stmt::Assign(assign) = &module.body[0] else { panic!("expected assignment") };
    assert!(matches!(&assign.targets[0], Expr::Name(node) if node.id.as_ref() == "fi"));
}

#[cfg(not(feature = "nfkc"))]
#[test]
fn preserves_unicode_identifiers_without_nfkc_feature() {
    let module = pysyn::parse_module("ﬁ = 1\n").expect("valid identifier");
    let Stmt::Assign(assign) = &module.body[0] else { panic!("expected assignment") };
    assert!(matches!(&assign.targets[0], Expr::Name(node) if node.id.as_ref() == "ﬁ"));
}
