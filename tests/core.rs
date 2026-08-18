#![allow(missing_docs)]

use pysyn::ast::{Expr, ExprContext, Stmt};
use pysyn::lexer::{tokenize_with, LexMode, LexOptions};
use pysyn::parser::{parse, ParseMode, ParseOptions};
use pysyn::token::{PythonVersion, TokenKind};

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
