//! AST dump and Python source generation.

#![allow(missing_docs)]

use crate::ast::*;
use crate::source::TextRange;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DumpOptions {
    pub include_attributes: bool,
    pub indent: Option<usize>,
}

impl Default for DumpOptions {
    fn default() -> Self {
        Self { include_attributes: true, indent: Some(2) }
    }
}

pub fn dump(module: &ModModule, options: DumpOptions) -> String {
    let mut output = String::new();
    dump_module(module, &options, 0, &mut output);
    output
}

pub fn unparse(module: &ModModule) -> String {
    let mut printer = Unparser { output: String::new(), indent: 0 };
    for statement in &module.body {
        printer.statement(statement);
    }
    printer.output
}

pub fn pyrepr(value: &str) -> String {
    repr_string(value)
}

/// Formats a floating-point value using Python's common repr thresholds.
pub fn pyrepr_float(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() { "-inf".into() } else { "inf".into() };
    }
    let raw = value.to_string();
    let absolute = value.abs();
    let mut result = if raw.contains('e') || raw.contains('E') {
        normalize_exponent(&raw)
    } else if absolute != 0.0 && !(1e-4..1e16).contains(&absolute) {
        scientific_from_fixed(&raw)
    } else {
        raw
    };
    if !result.contains('.') && !result.contains('e') && !result.contains('E') {
        result.push_str(".0");
    }
    result
}

/// Formats a byte string with Python-style `b'...'` escaping.
pub fn pyrepr_bytes(value: &[u8]) -> String {
    let mut output = String::from("b'");
    for byte in value {
        match byte {
            b'\\' => output.push_str("\\\\"),
            b'\'' => output.push_str("\\'"),
            b'\n' => output.push_str("\\n"),
            b'\r' => output.push_str("\\r"),
            b'\t' => output.push_str("\\t"),
            0x20..=0x7e => output.push(*byte as char),
            other => output.push_str(&format!("\\x{other:02x}")),
        }
    }
    output.push('\'');
    output
}

fn dump_module(module: &ModModule, options: &DumpOptions, level: usize, output: &mut String) {
    output.push_str("Module(body=[");
    for (index, statement) in module.body.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        dump_stmt(statement, options, level + 1, output);
    }
    output.push_str("])");
}

fn dump_stmt_list(statements: &[Stmt], options: &DumpOptions, level: usize, output: &mut String) {
    for (index, statement) in statements.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        dump_stmt(statement, options, level + 1, output);
    }
}

fn dump_expr_list(expressions: &[Expr], options: &DumpOptions, level: usize, output: &mut String) {
    for (index, expression) in expressions.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        dump_expr(expression, options, level, output);
    }
}

fn dump_pattern_list(
    patterns: &[Pattern],
    options: &DumpOptions,
    level: usize,
    output: &mut String,
) {
    for (index, pattern) in patterns.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        dump_pattern(pattern, options, level, output);
    }
}

fn dump_alias_list(aliases: &[Alias], output: &mut String) {
    for (index, alias) in aliases.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        output.push_str("alias(name=");
        output.push_str(&repr_string(&alias.name));
        output.push_str(", asname=");
        dump_optional_string(alias.asname.as_deref(), output);
        output.push(')');
    }
}

fn dump_keyword_list(
    keywords: &[Keyword],
    options: &DumpOptions,
    level: usize,
    output: &mut String,
) {
    for (index, keyword) in keywords.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        output.push_str("keyword(arg=");
        dump_optional_string(keyword.arg.as_deref(), output);
        output.push_str(", value=");
        dump_expr(&keyword.value, options, level, output);
        output.push(')');
    }
}

fn dump_with_item_list(
    items: &[WithItem],
    options: &DumpOptions,
    level: usize,
    output: &mut String,
) {
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        output.push_str("withitem(context_expr=");
        dump_expr(&item.context_expr, options, level, output);
        output.push_str(", optional_vars=");
        if let Some(value) = &item.optional_vars {
            dump_expr(value, options, level, output);
        } else {
            output.push_str("None");
        }
        output.push(')');
    }
}

fn dump_optional_string(value: Option<&str>, output: &mut String) {
    if let Some(value) = value {
        output.push_str(&repr_string(value));
    } else {
        output.push_str("None");
    }
}

fn dump_optional_expr(
    value: Option<&Expr>,
    options: &DumpOptions,
    level: usize,
    output: &mut String,
) {
    if let Some(value) = value {
        dump_expr(value, options, level, output);
    } else {
        output.push_str("None");
    }
}

fn dump_stmt(statement: &Stmt, options: &DumpOptions, level: usize, output: &mut String) {
    match statement {
        Stmt::Pass(_) => output.push_str("Pass()"),
        Stmt::Break(_) => output.push_str("Break()"),
        Stmt::Continue(_) => output.push_str("Continue()"),
        Stmt::Expr(node) => {
            output.push_str("Expr(value=");
            dump_expr(&node.value, options, level, output);
            output.push(')');
        }
        Stmt::Assign(node) => {
            output.push_str("Assign(targets=[");
            dump_expr_list(&node.targets, options, level, output);
            output.push_str("], value=");
            dump_expr(&node.value, options, level, output);
            output.push_str(", type_comment=");
            dump_optional_string(node.type_comment.as_deref(), output);
            output.push(')');
        }
        Stmt::AnnAssign(node) => {
            output.push_str("AnnAssign(target=");
            dump_expr(&node.target, options, level, output);
            output.push_str(", annotation=");
            dump_expr(&node.annotation, options, level, output);
            output.push_str(", value=");
            if let Some(value) = &node.value {
                dump_expr(value, options, level, output);
            } else {
                output.push_str("None");
            }
            output.push_str(", simple=");
            output.push_str(if node.simple { "1)" } else { "0)" });
        }
        Stmt::Return(node) => {
            output.push_str("Return(value=");
            if let Some(value) = &node.value {
                dump_expr(value, options, level, output);
            } else {
                output.push_str("None");
            }
            output.push(')');
        }
        Stmt::Delete(node) => {
            output.push_str("Delete(targets=[");
            dump_expr_list(&node.targets, options, level, output);
            output.push_str("])");
        }
        Stmt::If(node) => {
            output.push_str("If(test=");
            dump_expr(&node.test, options, level, output);
            output.push_str(", body=[");
            dump_stmt_list(&node.body, options, level, output);
            output.push_str("], orelse=[");
            dump_stmt_list(&node.orelse, options, level, output);
            output.push_str("])");
        }
        Stmt::While(node) => {
            output.push_str("While(test=");
            dump_expr(&node.test, options, level, output);
            output.push_str(", body=[");
            dump_stmt_list(&node.body, options, level, output);
            output.push_str("], orelse=[");
            dump_stmt_list(&node.orelse, options, level, output);
            output.push_str("])");
        }
        Stmt::For(node) | Stmt::AsyncFor(node) => {
            output.push_str(if matches!(statement, Stmt::AsyncFor(_)) {
                "AsyncFor(target="
            } else {
                "For(target="
            });
            dump_expr(&node.target, options, level, output);
            output.push_str(", iter=");
            dump_expr(&node.iter, options, level, output);
            output.push_str(", body=[");
            dump_stmt_list(&node.body, options, level, output);
            output.push_str("], orelse=[");
            dump_stmt_list(&node.orelse, options, level, output);
            output.push_str("], type_comment=");
            dump_optional_string(node.type_comment.as_deref(), output);
            output.push(')');
        }
        Stmt::FunctionDef(node) | Stmt::AsyncFunctionDef(node) => {
            output.push_str(if matches!(statement, Stmt::AsyncFunctionDef(_)) {
                "AsyncFunctionDef(name="
            } else {
                "FunctionDef(name="
            });
            output.push_str(&repr_string(&node.name));
            output.push_str(", args=");
            dump_parameters(&node.args, options, level, output);
            output.push_str(", body=[");
            dump_stmt_list(&node.body, options, level, output);
            output.push_str("], decorator_list=[");
            dump_expr_list(&node.decorator_list, options, level, output);
            output.push_str("], returns=");
            if let Some(returns) = &node.returns {
                dump_expr(returns, options, level, output);
            } else {
                output.push_str("None");
            }
            output.push_str(", type_comment=");
            dump_optional_string(node.type_comment.as_deref(), output);
            output.push_str(", type_params=");
            dump_type_params(&node.type_params, options, level, output);
            output.push(')');
        }
        Stmt::ClassDef(node) => {
            output.push_str("ClassDef(name=");
            output.push_str(&repr_string(&node.name));
            output.push_str(", bases=[");
            dump_expr_list(&node.bases, options, level, output);
            output.push_str("], keywords=[");
            dump_keyword_list(&node.keywords, options, level, output);
            output.push_str("], body=[");
            dump_stmt_list(&node.body, options, level, output);
            output.push_str("], decorator_list=[");
            dump_expr_list(&node.decorator_list, options, level, output);
            output.push_str("], type_params=");
            dump_type_params(&node.type_params, options, level, output);
            output.push(')');
        }
        Stmt::Assert(node) => {
            output.push_str("Assert(test=");
            dump_expr(&node.test, options, level, output);
            output.push_str(", msg=");
            if let Some(msg) = &node.msg {
                dump_expr(msg, options, level, output);
            } else {
                output.push_str("None");
            }
            output.push(')');
        }
        Stmt::Raise(node) => {
            output.push_str("Raise(exc=");
            if let Some(exc) = &node.exc {
                dump_expr(exc, options, level, output);
            } else {
                output.push_str("None");
            }
            output.push_str(", cause=");
            if let Some(cause) = &node.cause {
                dump_expr(cause, options, level, output);
            } else {
                output.push_str("None");
            }
            output.push(')');
        }
        Stmt::Import(node) => {
            output.push_str("Import(names=[");
            dump_alias_list(&node.names, output);
            output.push_str("])");
        }
        Stmt::ImportFrom(node) => {
            output.push_str("ImportFrom(module=");
            if let Some(module) = &node.module {
                output.push_str(&repr_string(module));
            } else {
                output.push_str("None");
            }
            output.push_str(", names=[");
            dump_alias_list(&node.names, output);
            output.push_str("], level=");
            output.push_str(&node.level.to_string());
            output.push(')');
        }
        Stmt::Global(node) => {
            output.push_str("Global(names=[");
            for (index, name) in node.names.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                output.push_str(&repr_string(name));
            }
            output.push_str("])");
        }
        Stmt::Nonlocal(node) => {
            output.push_str("Nonlocal(names=[");
            for (index, name) in node.names.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                output.push_str(&repr_string(name));
            }
            output.push_str("])");
        }
        Stmt::With(node) | Stmt::AsyncWith(node) => {
            output.push_str(if matches!(statement, Stmt::AsyncWith(_)) {
                "AsyncWith(items=["
            } else {
                "With(items=["
            });
            dump_with_item_list(&node.items, options, level, output);
            output.push_str("], body=[");
            dump_stmt_list(&node.body, options, level, output);
            output.push_str("], type_comment=");
            dump_optional_string(node.type_comment.as_deref(), output);
            output.push(')');
        }
        Stmt::Try(node) | Stmt::TryStar(node) => {
            output.push_str(if matches!(statement, Stmt::TryStar(_)) {
                "TryStar(body=["
            } else {
                "Try(body=["
            });
            dump_stmt_list(&node.body, options, level, output);
            output.push_str("], handlers=[");
            for (index, handler) in node.handlers.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                output.push_str("ExceptHandler(type=");
                if let Some(typ) = &handler.typ {
                    dump_expr(typ, options, level, output);
                } else {
                    output.push_str("None");
                }
                if let Some(name) = &handler.name {
                    output.push_str(", name=");
                    output.push_str(&repr_string(name));
                }
                output.push_str(", body=[");
                dump_stmt_list(&handler.body, options, level, output);
                output.push_str("])");
            }
            output.push_str("], orelse=[");
            dump_stmt_list(&node.orelse, options, level, output);
            output.push_str("], finalbody=[");
            dump_stmt_list(&node.finalbody, options, level, output);
            output.push_str("])");
        }
        Stmt::AugAssign(node) => {
            output.push_str("AugAssign(target=");
            dump_expr(&node.target, options, level, output);
            output.push_str(", op=");
            output.push_str(binary_name(node.op));
            output.push_str(", value=");
            dump_expr(&node.value, options, level, output);
            output.push(')');
        }
        Stmt::Match(node) => {
            output.push_str("Match(subject=");
            dump_expr(&node.subject, options, level, output);
            output.push_str(", cases=[");
            for (index, case) in node.cases.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                output.push_str("match_case(pattern=");
                dump_pattern(&case.pattern, options, level, output);
                output.push_str(", guard=");
                if let Some(guard) = &case.guard {
                    dump_expr(guard, options, level, output);
                } else {
                    output.push_str("None");
                }
                output.push_str(", body=[");
                dump_stmt_list(&case.body, options, level, output);
                output.push_str("])");
            }
            output.push_str("])");
        }
        Stmt::TypeAlias(node) => {
            output.push_str("TypeAlias(name=");
            dump_expr(&node.name, options, level, output);
            output.push_str(", type_params=");
            dump_type_params(&node.type_params, options, level, output);
            output.push_str(", value=");
            dump_expr(&node.value, options, level, output);
            output.push(')');
        }
        Stmt::Invalid(node) => {
            output.push_str("Invalid(message=");
            output.push_str(&repr_string(&node.message));
            output.push(')');
        }
    }
}

fn dump_parameters(
    parameters: &Parameters,
    options: &DumpOptions,
    level: usize,
    output: &mut String,
) {
    output.push_str("arguments(posonlyargs=[");
    dump_parameter_list(&parameters.posonlyargs, options, level, output);
    output.push_str("], args=[");
    dump_parameter_list(&parameters.args, options, level, output);
    output.push_str("], vararg=");
    if let Some(parameter) = &parameters.vararg {
        dump_parameter(parameter, options, level, output);
    } else {
        output.push_str("None");
    }
    output.push_str(", kwonlyargs=[");
    dump_parameter_list(&parameters.kwonlyargs, options, level, output);
    output.push_str("], kw_defaults=[");
    for (index, value) in parameters.kw_defaults.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        if let Some(value) = value {
            dump_expr(value, options, level, output);
        } else {
            output.push_str("None");
        }
    }
    output.push_str("], kwarg=");
    if let Some(parameter) = &parameters.kwarg {
        dump_parameter(parameter, options, level, output);
    } else {
        output.push_str("None");
    }
    output.push_str(", defaults=[");
    for (index, value) in parameters.defaults.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        dump_expr(value, options, level, output);
    }
    output.push_str("])");
}

fn dump_type_params(
    type_params: &[TypeParam],
    options: &DumpOptions,
    level: usize,
    output: &mut String,
) {
    output.push('[');
    for (index, type_param) in type_params.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        let (name, data) = match type_param {
            TypeParam::TypeVar(data)
            | TypeParam::ParamSpec(data)
            | TypeParam::TypeVarTuple(data) => (type_param, data),
        };
        output.push_str(match name {
            TypeParam::TypeVar(_) => "TypeVar(name=",
            TypeParam::ParamSpec(_) => "ParamSpec(name=",
            TypeParam::TypeVarTuple(_) => "TypeVarTuple(name=",
        });
        output.push_str(&repr_string(&data.name));
        output.push_str(", bound=");
        if let Some(bound) = &data.bound {
            dump_expr(bound, options, level, output);
        } else {
            output.push_str("None");
        }
        output.push_str(", default_value=");
        if let Some(default) = &data.default {
            dump_expr(default, options, level, output);
        } else {
            output.push_str("None");
        }
        output.push(')');
    }
    output.push(']');
}

fn dump_parameter_list(
    parameters: &[Parameter],
    options: &DumpOptions,
    level: usize,
    output: &mut String,
) {
    for (index, parameter) in parameters.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        dump_parameter(parameter, options, level, output);
    }
}

fn dump_parameter(parameter: &Parameter, options: &DumpOptions, level: usize, output: &mut String) {
    output.push_str("arg(arg=");
    output.push_str(&repr_string(&parameter.name));
    output.push_str(", annotation=");
    if let Some(annotation) = &parameter.annotation {
        dump_expr(annotation, options, level, output);
    } else {
        output.push_str("None");
    }
    output.push_str(", type_comment=");
    dump_optional_string(parameter.type_comment.as_deref(), output);
    output.push(')');
}

fn dump_pattern(pattern: &Pattern, options: &DumpOptions, level: usize, output: &mut String) {
    match pattern {
        Pattern::As(node) => {
            if let Some(pattern) = &node.pattern {
                output.push_str("MatchAs(pattern=");
                dump_pattern(pattern, options, level, output);
                if let Some(name) = &node.name {
                    output.push_str(", name=");
                    output.push_str(&repr_string(name));
                }
                output.push(')');
            } else if let Some(name) = &node.name {
                output.push_str("MatchAs(name=");
                output.push_str(&repr_string(name));
                output.push(')');
            } else {
                output.push_str("MatchAs()");
            }
        }
        Pattern::Singleton(node) => {
            output.push_str("MatchSingleton(value=");
            dump_expr(&node.value, options, level, output);
            output.push(')');
        }
        Pattern::Value(node) => {
            output.push_str("MatchValue(value=");
            dump_expr(&node.value, options, level, output);
            output.push(')');
        }
        Pattern::Sequence(node) => {
            output.push_str("MatchSequence(patterns=[");
            dump_pattern_list(&node.patterns, options, level, output);
            output.push_str("])");
        }
        Pattern::Or(node) => {
            output.push_str("MatchOr(patterns=[");
            dump_pattern_list(&node.patterns, options, level, output);
            output.push_str("])");
        }
        Pattern::Star(node) => {
            if let Some(name) = &node.name {
                output.push_str("MatchStar(name=");
                output.push_str(&repr_string(name));
                output.push(')');
            } else {
                output.push_str("MatchStar()");
            }
        }
        Pattern::Mapping(node) => {
            output.push_str("MatchMapping(keys=[");
            dump_expr_list(&node.keys, options, level, output);
            output.push_str("], patterns=[");
            dump_pattern_list(&node.patterns, options, level, output);
            output.push_str("], rest=");
            if let Some(rest) = &node.rest {
                output.push_str(&repr_string(rest));
            } else {
                output.push_str("None");
            }
            output.push(')');
        }
        Pattern::Class(node) => {
            output.push_str("MatchClass(cls=");
            dump_expr(&node.cls, options, level, output);
            output.push_str(", patterns=[");
            dump_pattern_list(&node.patterns, options, level, output);
            output.push_str("], kwd_attrs=[");
            for (index, attr) in node.kwd_attrs.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                output.push_str(&repr_string(attr));
            }
            output.push_str("], kwd_patterns=[");
            dump_pattern_list(&node.kwd_patterns, options, level, output);
            output.push_str("])");
        }
        Pattern::Invalid(_) => output.push_str("Invalid()"),
    }
}

fn dump_generators(
    generators: &[Comprehension],
    options: &DumpOptions,
    level: usize,
    output: &mut String,
) {
    output.push('[');
    for (index, generator) in generators.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        output.push_str("comprehension(target=");
        dump_expr(&generator.target, options, level, output);
        output.push_str(", iter=");
        dump_expr(&generator.iter, options, level, output);
        output.push_str(", ifs=[");
        for (condition_index, condition) in generator.ifs.iter().enumerate() {
            if condition_index > 0 {
                output.push_str(", ");
            }
            dump_expr(condition, options, level, output);
        }
        output.push_str("], is_async=");
        output.push_str(if generator.is_async { "1)" } else { "0)" });
    }
    output.push(']');
}

fn dump_expr(expression: &Expr, options: &DumpOptions, level: usize, output: &mut String) {
    match expression {
        Expr::Name(node) => {
            output.push_str("Name(id=");
            output.push_str(&repr_string(&node.id));
            output.push_str(", ctx=");
            output.push_str(context_name(node.ctx));
            output.push(')');
        }
        Expr::NumberLiteral(node) => {
            output.push_str("Constant(value=");
            output.push_str(&number_repr(&node.value));
            output.push(')');
        }
        Expr::StringLiteral(node) => {
            output.push_str("Constant(value=");
            output.push_str(&repr_string(&node.value.to_str()));
            if node.value.parts.first().is_some_and(|part| part.flags.prefix.is_unicode()) {
                output.push_str(", kind='u'");
            }
            output.push(')');
        }
        Expr::BytesLiteral(node) => {
            output.push_str("Constant(value=");
            output.push_str(&repr_bytes_string(&node.value.to_str()));
            output.push(')');
        }
        Expr::BooleanLiteral(node) => output.push_str(if node.value {
            "Constant(value=True)"
        } else {
            "Constant(value=False)"
        }),
        Expr::NoneLiteral(_) => output.push_str("Constant(value=None)"),
        Expr::EllipsisLiteral(_) => output.push_str("Constant(value=Ellipsis)"),
        Expr::BinOp(node) => {
            output.push_str("BinOp(left=");
            dump_expr(&node.left, options, level, output);
            output.push_str(", op=");
            output.push_str(binary_name(node.op));
            output.push_str(", right=");
            dump_expr(&node.right, options, level, output);
            output.push(')');
        }
        Expr::UnaryOp(node) => {
            output.push_str("UnaryOp(op=");
            output.push_str(unary_name(node.op));
            output.push_str(", operand=");
            dump_expr(&node.operand, options, level, output);
            output.push(')');
        }
        Expr::BoolOp(node) => {
            output.push_str("BoolOp(op=");
            output.push_str(match node.op {
                BoolOperator::And => "And()",
                BoolOperator::Or => "Or()",
            });
            output.push_str(", values=[");
            for (index, value) in node.values.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                dump_expr(value, options, level, output);
            }
            output.push_str("])");
        }
        Expr::Compare(node) => {
            output.push_str("Compare(left=");
            dump_expr(&node.left, options, level, output);
            output.push_str(", ops=[");
            for (index, op) in node.ops.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                output.push_str(compare_name(*op));
            }
            output.push_str("], comparators=[");
            for (index, value) in node.comparators.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                dump_expr(value, options, level, output);
            }
            output.push_str("])");
        }
        Expr::Call(node) => {
            output.push_str("Call(func=");
            dump_expr(&node.func, options, level, output);
            output.push_str(", args=[");
            dump_expr_list(&node.args, options, level, output);
            output.push_str("], keywords=[");
            dump_keyword_list(&node.keywords, options, level, output);
            output.push_str("])");
        }
        Expr::Attribute(node) => {
            output.push_str("Attribute(value=");
            dump_expr(&node.value, options, level, output);
            output.push_str(", attr=");
            output.push_str(&repr_string(&node.attr));
            output.push_str(", ctx=");
            output.push_str(context_name(node.ctx));
            output.push(')');
        }
        Expr::Subscript(node) => {
            output.push_str("Subscript(value=");
            dump_expr(&node.value, options, level, output);
            output.push_str(", slice=");
            dump_expr(&node.slice, options, level, output);
            output.push_str(", ctx=");
            output.push_str(context_name(node.ctx));
            output.push(')');
        }
        Expr::List(node) => {
            dump_sequence_with_context("List", &node.elts, node.ctx, options, level, output)
        }
        Expr::Tuple(node) => {
            dump_sequence_with_context("Tuple", &node.elts, node.ctx, options, level, output)
        }
        Expr::Set(node) => dump_sequence("Set", &node.elts, options, level, output),
        Expr::Dict(node) => {
            output.push_str("Dict(keys=[");
            for (index, key) in node.keys.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                if let Some(key) = key {
                    dump_expr(key, options, level, output);
                } else {
                    output.push_str("None");
                }
            }
            output.push_str("], values=[");
            for (index, value) in node.values.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                dump_expr(value, options, level, output);
            }
            output.push_str("])");
        }
        Expr::IfExp(node) => {
            output.push_str("IfExp(test=");
            dump_expr(&node.test, options, level, output);
            output.push_str(", body=");
            dump_expr(&node.body, options, level, output);
            output.push_str(", orelse=");
            dump_expr(&node.orelse, options, level, output);
            output.push(')');
        }
        Expr::Starred(node) => {
            output.push_str("Starred(value=");
            dump_expr(&node.value, options, level, output);
            output.push_str(", ctx=");
            output.push_str(context_name(node.ctx));
            output.push(')');
        }
        Expr::FString(node) => {
            output.push_str("JoinedStr(values=[");
            for (index, value) in node.values.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                dump_expr(value, options, level, output);
            }
            output.push_str("])");
        }
        Expr::FormattedValue(node) => {
            output.push_str("FormattedValue(value=");
            dump_expr(&node.value, options, level, output);
            output.push_str(", conversion=");
            output.push_str(
                &node
                    .conversion
                    .map_or_else(|| "-1".to_owned(), |value| (value as u32).to_string()),
            );
            output.push_str(", format_spec=");
            if let Some(spec) = &node.format_spec {
                dump_expr(spec, options, level, output);
            } else {
                output.push_str("None");
            }
            output.push(')');
        }
        Expr::Lambda(node) => {
            output.push_str("Lambda(args=");
            dump_parameters(&node.args, options, level, output);
            output.push_str(", body=");
            dump_expr(&node.body, options, level, output);
            output.push(')');
        }
        Expr::NamedExpr(node) => {
            output.push_str("NamedExpr(target=");
            dump_expr(&node.target, options, level, output);
            output.push_str(", value=");
            dump_expr(&node.value, options, level, output);
            output.push(')');
        }
        Expr::Await(node) => {
            output.push_str("Await(value=");
            dump_optional_expr(node.value.as_deref(), options, level, output);
            output.push(')');
        }
        Expr::Yield(node) => {
            output.push_str("Yield(value=");
            dump_optional_expr(node.value.as_deref(), options, level, output);
            output.push(')');
        }
        Expr::YieldFrom(node) => {
            output.push_str("YieldFrom(value=");
            dump_optional_expr(node.value.as_deref(), options, level, output);
            output.push(')');
        }
        Expr::ListComp(node) => {
            output.push_str("ListComp(elt=");
            dump_expr(&node.elt, options, level, output);
            output.push_str(", generators=");
            dump_generators(&node.generators, options, level, output);
            output.push(')');
        }
        Expr::SetComp(node) => {
            output.push_str("SetComp(elt=");
            dump_expr(&node.elt, options, level, output);
            output.push_str(", generators=");
            dump_generators(&node.generators, options, level, output);
            output.push(')');
        }
        Expr::DictComp(node) => {
            output.push_str("DictComp(key=");
            if let Some(key) = &node.key {
                dump_expr(key, options, level, output);
            } else {
                output.push_str("None");
            }
            output.push_str(", value=");
            if let Some(value) = &node.value {
                dump_expr(value, options, level, output);
            } else {
                dump_expr(&node.elt, options, level, output);
            }
            output.push_str(", generators=");
            dump_generators(&node.generators, options, level, output);
            output.push(')');
        }
        Expr::GeneratorExp(node) => {
            output.push_str("GeneratorExp(elt=");
            dump_expr(&node.elt, options, level, output);
            output.push_str(", generators=");
            dump_generators(&node.generators, options, level, output);
            output.push(')');
        }
        Expr::Slice(node) => {
            output.push_str("Slice(lower=");
            if let Some(value) = &node.lower {
                dump_expr(value, options, level, output);
            } else {
                output.push_str("None");
            }
            output.push_str(", upper=");
            if let Some(value) = &node.upper {
                dump_expr(value, options, level, output);
            } else {
                output.push_str("None");
            }
            output.push_str(", step=");
            if let Some(value) = &node.step {
                dump_expr(value, options, level, output);
            } else {
                output.push_str("None");
            }
            output.push(')');
        }
        Expr::Invalid(node) => {
            output.push_str("Invalid(message=");
            output.push_str(&repr_string(&node.message));
            output.push(')');
        }
    }
    let _ = (options, level);
}

fn dump_sequence(
    name: &str,
    values: &[Expr],
    options: &DumpOptions,
    level: usize,
    output: &mut String,
) {
    output.push_str(name);
    output.push_str("(elts=[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        dump_expr(value, options, level, output);
    }
    output.push_str("])");
}

fn dump_sequence_with_context(
    name: &str,
    values: &[Expr],
    context: ExprContext,
    options: &DumpOptions,
    level: usize,
    output: &mut String,
) {
    output.push_str(name);
    output.push_str("(elts=[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        dump_expr(value, options, level, output);
    }
    output.push_str("], ctx=");
    output.push_str(context_name(context));
    output.push(')');
}

fn binary_name(operator: BinaryOperator) -> &'static str {
    match operator {
        BinaryOperator::Add => "Add()",
        BinaryOperator::Sub => "Sub()",
        BinaryOperator::Mult => "Mult()",
        BinaryOperator::MatMult => "MatMult()",
        BinaryOperator::Div => "Div()",
        BinaryOperator::FloorDiv => "FloorDiv()",
        BinaryOperator::Mod => "Mod()",
        BinaryOperator::Pow => "Pow()",
        BinaryOperator::LShift => "LShift()",
        BinaryOperator::RShift => "RShift()",
        BinaryOperator::BitOr => "BitOr()",
        BinaryOperator::BitXor => "BitXor()",
        BinaryOperator::BitAnd => "BitAnd()",
    }
}
fn unary_name(operator: UnaryOperator) -> &'static str {
    match operator {
        UnaryOperator::Invert => "Invert()",
        UnaryOperator::Not => "Not()",
        UnaryOperator::UAdd => "UAdd()",
        UnaryOperator::USub => "USub()",
    }
}
fn compare_name(operator: CompareOperator) -> &'static str {
    match operator {
        CompareOperator::Eq => "Eq()",
        CompareOperator::NotEq => "NotEq()",
        CompareOperator::Lt => "Lt()",
        CompareOperator::LtE => "LtE()",
        CompareOperator::Gt => "Gt()",
        CompareOperator::GtE => "GtE()",
        CompareOperator::In => "In()",
        CompareOperator::NotIn => "NotIn()",
        CompareOperator::Is => "Is()",
        CompareOperator::IsNot => "IsNot()",
    }
}
fn context_name(context: ExprContext) -> &'static str {
    match context {
        ExprContext::Load => "Load()",
        ExprContext::Store => "Store()",
        ExprContext::Del => "Del()",
    }
}
fn number_repr(number: &Number) -> String {
    match number {
        Number::Int(value) => value.to_string(),
        Number::Float(value) => pyrepr_float(*value),
        Number::Complex { real, imag } => {
            if *real == 0.0 {
                let mut value = pyrepr_float(*imag);
                if value.ends_with(".0") {
                    value.truncate(value.len() - 2);
                }
                format!("{value}j")
            } else {
                format!("({}+{}j)", pyrepr_float(*real), pyrepr_float(*imag))
            }
        }
    }
}
fn repr_string(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') { '"' } else { '\'' };
    let mut output = String::new();
    output.push(quote);
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            character if character == quote => {
                output.push('\\');
                output.push(character);
            }
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            other if other.is_control() => {
                let code = other as u32;
                if code <= 0xff {
                    output.push_str(&format!("\\x{code:02x}"));
                } else if code <= 0xffff {
                    output.push_str(&format!("\\u{code:04x}"));
                } else {
                    output.push_str(&format!("\\U{code:08x}"));
                }
            }
            other => output.push(other),
        }
    }
    output.push(quote);
    output
}

fn repr_bytes_string(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') { '"' } else { '\'' };
    let mut output = String::from("b");
    output.push(quote);
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            character if character == quote => {
                output.push('\\');
                output.push(character);
            }
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            other if other.is_ascii() && !other.is_ascii_control() => output.push(other),
            other => output.push_str(&format!("\\x{code:02x}", code = other as u32 & 0xff)),
        }
    }
    output.push(quote);
    output
}

fn normalize_exponent(value: &str) -> String {
    let Some((mantissa, exponent)) = value.split_once(['e', 'E']) else {
        return value.to_owned();
    };
    let exponent = exponent.parse::<i32>().unwrap_or(0);
    let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
    format!("{mantissa}e{exponent:+03}")
}

fn scientific_from_fixed(value: &str) -> String {
    let negative = value.starts_with('-');
    let digits = value.trim_start_matches('-');
    let decimal_position = digits.find('.').unwrap_or(digits.len());
    let digits = digits.replace('.', "");
    let Some(first) = digits.find(|character: char| character != '0') else {
        return if negative { "-0.0".into() } else { "0.0".into() };
    };
    let exponent = decimal_position as i32 - first as i32 - 1;
    let rest = digits[first + 1..].trim_end_matches('0');
    let mantissa = if rest.is_empty() {
        digits[first..first + 1].to_owned()
    } else {
        format!("{}.{}", &digits[first..first + 1], rest)
    };
    format!("{}{}e{exponent:+03}", if negative { "-" } else { "" }, mantissa)
}

struct Unparser {
    output: String,
    indent: usize,
}

impl Unparser {
    fn statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Pass(_) => self.line("pass"),
            Stmt::Break(_) => self.line("break"),
            Stmt::Continue(_) => self.line("continue"),
            Stmt::Expr(node) => self.line(&self.expression(&node.value)),
            Stmt::Assign(node) => {
                let targets = node
                    .targets
                    .iter()
                    .map(|target| self.expression(target))
                    .collect::<Vec<_>>()
                    .join(" = ");
                self.line(&format!(
                    "{targets} = {}{}",
                    self.expression(&node.value),
                    type_comment_suffix(node.type_comment.as_deref())
                ));
            }
            Stmt::Delete(node) => self.line(&format!(
                "del {}",
                node.targets
                    .iter()
                    .map(|target| self.expression(target))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            Stmt::AnnAssign(node) => self.line(&format!(
                "{}: {}{}",
                if !node.simple && matches!(node.target.as_ref(), Expr::Name(_)) {
                    format!("({})", self.expression(&node.target))
                } else {
                    self.expression(&node.target)
                },
                self.expression(&node.annotation),
                node.value
                    .as_ref()
                    .map(|value| format!(" = {}", self.expression(value)))
                    .unwrap_or_default()
            )),
            Stmt::AugAssign(node) => self.line(&format!(
                "{} {}= {}",
                self.expression(&node.target),
                binary_text(node.op),
                self.expression(&node.value)
            )),
            Stmt::Return(node) => self.line(&format!(
                "return{}",
                node.value
                    .as_ref()
                    .map(|value| format!(" {}", self.expression(value)))
                    .unwrap_or_default()
            )),
            Stmt::Raise(node) => self.line(&format!(
                "raise{}{}",
                node.exc
                    .as_ref()
                    .map(|value| format!(" {}", self.expression(value)))
                    .unwrap_or_default(),
                node.cause
                    .as_ref()
                    .map(|value| format!(" from {}", self.expression(value)))
                    .unwrap_or_default()
            )),
            Stmt::Assert(node) => self.line(&format!(
                "assert {}{}",
                self.expression(&node.test),
                node.msg
                    .as_ref()
                    .map(|value| format!(", {}", self.expression(value)))
                    .unwrap_or_default()
            )),
            Stmt::Import(node) => self.line(&format!(
                "import {}",
                node.names.iter().map(alias_text).collect::<Vec<_>>().join(", ")
            )),
            Stmt::ImportFrom(node) => {
                let module = format!(
                    "{}{}",
                    ".".repeat(node.level as usize),
                    node.module.as_deref().unwrap_or("")
                );
                self.line(&format!(
                    "from {module} import {}",
                    node.names.iter().map(alias_text).collect::<Vec<_>>().join(", ")
                ))
            }
            Stmt::Global(node) => self.line(&format!(
                "global {}",
                node.names.iter().map(|name| name.as_ref()).collect::<Vec<_>>().join(", ")
            )),
            Stmt::Nonlocal(node) => self.line(&format!(
                "nonlocal {}",
                node.names.iter().map(|name| name.as_ref()).collect::<Vec<_>>().join(", ")
            )),
            Stmt::If(node) => self.if_statement(node),
            Stmt::While(node) => {
                self.block_header(&format!("while {}:", self.expression(&node.test)), &node.body);
                if !node.orelse.is_empty() {
                    self.block_header("else:", &node.orelse);
                }
            }
            Stmt::For(node) | Stmt::AsyncFor(node) => {
                let prefix = if matches!(statement, Stmt::AsyncFor(_)) { "async " } else { "" };
                self.block_header(
                    &format!(
                        "{prefix}for {} in {}:{}",
                        self.expression(&node.target),
                        self.expression(&node.iter),
                        type_comment_suffix(node.type_comment.as_deref())
                    ),
                    &node.body,
                );
                if !node.orelse.is_empty() {
                    self.block_header("else:", &node.orelse);
                }
            }
            Stmt::With(node) | Stmt::AsyncWith(node) => {
                let prefix = if matches!(statement, Stmt::AsyncWith(_)) { "async " } else { "" };
                let items = node
                    .items
                    .iter()
                    .map(|item| {
                        format!(
                            "{}{}",
                            self.expression(&item.context_expr),
                            item.optional_vars
                                .as_ref()
                                .map(|value| format!(" as {}", self.expression(value)))
                                .unwrap_or_default()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                self.block_header(
                    &format!(
                        "{prefix}with {items}:{}",
                        type_comment_suffix(node.type_comment.as_deref())
                    ),
                    &node.body,
                );
            }
            Stmt::Try(node) | Stmt::TryStar(node) => {
                self.try_statement(node, matches!(statement, Stmt::TryStar(_)))
            }
            Stmt::Match(node) => {
                self.line(&format!("match {}:", self.expression(&node.subject)));
                self.indent += 1;
                for case in &node.cases {
                    let guard = case
                        .guard
                        .as_ref()
                        .map(|value| format!(" if {}", self.expression(value)))
                        .unwrap_or_default();
                    self.line(&format!("case {}{}:", self.pattern(&case.pattern), guard));
                    self.indent += 1;
                    self.statements_or_pass(&case.body);
                    self.indent = self.indent.saturating_sub(1);
                }
                self.indent = self.indent.saturating_sub(1);
            }
            Stmt::FunctionDef(node) | Stmt::AsyncFunctionDef(node) => {
                for decorator in &node.decorator_list {
                    self.line(&format!("@{}", self.expression(decorator)));
                }
                let prefix =
                    if matches!(statement, Stmt::AsyncFunctionDef(_)) { "async " } else { "" };
                let returns = node
                    .returns
                    .as_ref()
                    .map(|value| format!(" -> {}", self.expression(value)))
                    .unwrap_or_default();
                self.block_header(
                    &format!(
                        "{prefix}def {}{}({}){}:{}",
                        node.name,
                        self.type_parameters(&node.type_params),
                        self.parameters(&node.args),
                        returns,
                        type_comment_suffix(node.type_comment.as_deref())
                    ),
                    &node.body,
                );
            }
            Stmt::ClassDef(node) => {
                for decorator in &node.decorator_list {
                    self.line(&format!("@{}", self.expression(decorator)));
                }
                let bases = node
                    .bases
                    .iter()
                    .map(|base| self.expression(base))
                    .chain(node.keywords.iter().map(|keyword| {
                        keyword
                            .arg
                            .as_ref()
                            .map(|arg| format!("{arg}={}", self.expression(&keyword.value)))
                            .unwrap_or_else(|| format!("**{}", self.expression(&keyword.value)))
                    }))
                    .collect::<Vec<_>>();
                let suffix = if bases.is_empty() {
                    String::new()
                } else {
                    format!("({})", bases.join(", "))
                };
                self.block_header(
                    &format!(
                        "class {}{}{}:",
                        node.name,
                        self.type_parameters(&node.type_params),
                        suffix
                    ),
                    &node.body,
                );
            }
            Stmt::TypeAlias(node) => {
                let name = self.expression(&node.name);
                self.line(&format!(
                    "type {}{} = {}",
                    name,
                    self.type_parameters(&node.type_params),
                    self.expression(&node.value)
                ));
            }
            Stmt::Invalid(node) => self.line(&format!("# invalid statement: {}", node.message)),
        }
    }
    fn if_statement(&mut self, node: &StmtIf) {
        self.line(&format!("if {}:", self.expression(&node.test)));
        self.indent += 1;
        self.statements_or_pass(&node.body);
        self.indent = self.indent.saturating_sub(1);
        self.if_orelse(&node.orelse);
    }
    fn if_orelse(&mut self, orelse: &[Stmt]) {
        if orelse.is_empty() {
            return;
        }
        if orelse.len() == 1 {
            if let Stmt::If(node) = &orelse[0] {
                self.line(&format!("elif {}:", self.expression(&node.test)));
                self.indent += 1;
                self.statements_or_pass(&node.body);
                self.indent = self.indent.saturating_sub(1);
                self.if_orelse(&node.orelse);
                return;
            }
        }
        self.line("else:");
        self.indent += 1;
        self.statements_or_pass(orelse);
        self.indent = self.indent.saturating_sub(1);
    }
    fn try_statement(&mut self, node: &StmtTry, is_star: bool) {
        self.line("try:");
        self.indent += 1;
        self.statements_or_pass(&node.body);
        self.indent = self.indent.saturating_sub(1);
        for handler in &node.handlers {
            let prefix = if is_star { "except*" } else { "except" };
            let exception = handler
                .typ
                .as_ref()
                .map(|value| format!(" {}", self.expression(value)))
                .unwrap_or_default();
            let name =
                handler.name.as_ref().map(|value| format!(" as {value}")).unwrap_or_default();
            self.line(&format!("{prefix}{exception}{name}:"));
            self.indent += 1;
            self.statements_or_pass(&handler.body);
            self.indent = self.indent.saturating_sub(1);
        }
        if !node.orelse.is_empty() {
            self.line("else:");
            self.indent += 1;
            self.statements_or_pass(&node.orelse);
            self.indent = self.indent.saturating_sub(1);
        }
        if !node.finalbody.is_empty() {
            self.line("finally:");
            self.indent += 1;
            self.statements_or_pass(&node.finalbody);
            self.indent = self.indent.saturating_sub(1);
        }
    }
    fn statements_or_pass(&mut self, statements: &[Stmt]) {
        if statements.is_empty() {
            self.line("pass");
            return;
        }
        for statement in statements {
            self.statement(statement);
        }
    }
    fn block_header(&mut self, header: &str, body: &[Stmt]) {
        self.line(header);
        self.indent += 1;
        self.statements_or_pass(body);
        self.indent = self.indent.saturating_sub(1);
    }
    fn line(&mut self, value: &str) {
        self.output.push_str(&"    ".repeat(self.indent));
        self.output.push_str(value);
        self.output.push('\n');
    }
    fn type_parameters(&self, type_params: &[TypeParam]) -> String {
        if type_params.is_empty() {
            return String::new();
        }
        let values = type_params
            .iter()
            .map(|type_param| {
                let (prefix, data) = match type_param {
                    TypeParam::TypeVar(data) => ("", data),
                    TypeParam::ParamSpec(data) => ("**", data),
                    TypeParam::TypeVarTuple(data) => ("*", data),
                };
                let bound = data
                    .bound
                    .as_ref()
                    .map(|value| format!(": {}", self.expression(value)))
                    .unwrap_or_default();
                let default = data
                    .default
                    .as_ref()
                    .map(|value| format!(" = {}", self.expression(value)))
                    .unwrap_or_default();
                format!("{prefix}{}{bound}{default}", data.name)
            })
            .collect::<Vec<_>>();
        format!("[{}]", values.join(", "))
    }
    fn parameters(&self, parameters: &Parameters) -> String {
        let mut values = Vec::new();
        values.extend(parameters.posonlyargs.iter().map(|parameter| self.parameter(parameter)));
        if !parameters.posonlyargs.is_empty() {
            values.push("/".into());
        }
        values.extend(parameters.args.iter().map(|parameter| self.parameter(parameter)));
        if let Some(vararg) = &parameters.vararg {
            values.push(format!("*{}", self.parameter(vararg)));
        } else if !parameters.kwonlyargs.is_empty() {
            values.push("*".into());
        }
        values.extend(parameters.kwonlyargs.iter().map(|parameter| self.parameter(parameter)));
        if let Some(kwarg) = &parameters.kwarg {
            values.push(format!("**{}", self.parameter(kwarg)));
        }
        values.join(", ")
    }
    fn parameter(&self, parameter: &Parameter) -> String {
        let annotation = parameter
            .annotation
            .as_ref()
            .map(|value| format!(": {}", self.expression(value)))
            .unwrap_or_default();
        let default = parameter
            .default
            .as_ref()
            .map(|value| format!(" = {}", self.expression(value)))
            .unwrap_or_default();
        format!("{}{annotation}{default}", parameter.name)
    }
    fn expression(&self, expression: &Expr) -> String {
        match expression {
            Expr::Name(node) => node.id.to_string(),
            Expr::NumberLiteral(node) => node.raw.to_string(),
            Expr::StringLiteral(node) => {
                let prefix = if node
                    .value
                    .parts
                    .first()
                    .is_some_and(|part| part.flags.prefix.is_unicode())
                {
                    "u"
                } else {
                    ""
                };
                format!("{prefix}{}", repr_string(&node.value.to_str()))
            }
            Expr::BytesLiteral(node) => repr_bytes_string(&node.value.to_str()),
            Expr::BooleanLiteral(node) => {
                if node.value {
                    "True".into()
                } else {
                    "False".into()
                }
            }
            Expr::NoneLiteral(_) => "None".into(),
            Expr::EllipsisLiteral(_) => "...".into(),
            Expr::Attribute(node) => {
                let value = match node.value.as_ref() {
                    Expr::NumberLiteral(_) => format!("({})", self.expression(&node.value)),
                    _ => self.expression(&node.value),
                };
                format!("{value}.{}", node.attr)
            }
            Expr::Call(node) => format!(
                "{}({})",
                self.expression(&node.func),
                node.args
                    .iter()
                    .map(|arg| self.expression(arg))
                    .chain(node.keywords.iter().map(|keyword| self.keyword(keyword)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Expr::Subscript(node) => {
                format!("{}[{}]", self.expression(&node.value), self.subscript_slice(&node.slice))
            }
            Expr::BinOp(node) => format!(
                "({} {} {})",
                self.expression(&node.left),
                binary_text(node.op),
                self.expression(&node.right)
            ),
            Expr::UnaryOp(node) => {
                format!("({}{})", unary_text(node.op), self.expression(&node.operand))
            }
            Expr::BoolOp(node) => format!(
                "({})",
                node.values
                    .iter()
                    .map(|value| self.expression(value))
                    .collect::<Vec<_>>()
                    .join(if node.op == BoolOperator::And { " and " } else { " or " })
            ),
            Expr::Compare(node) => {
                let mut result = self.expression(&node.left);
                for (op, value) in node.ops.iter().zip(&node.comparators) {
                    result.push(' ');
                    result.push_str(compare_text(*op));
                    result.push(' ');
                    result.push_str(&self.expression(value));
                }
                format!("({result})")
            }
            Expr::List(node) => format!(
                "[{}]",
                node.elts.iter().map(|value| self.expression(value)).collect::<Vec<_>>().join(", ")
            ),
            Expr::Tuple(node) => {
                let values =
                    node.elts.iter().map(|value| self.expression(value)).collect::<Vec<_>>();
                match values.len() {
                    0 => "()".into(),
                    1 => format!("({},)", values[0]),
                    _ => format!("({})", values.join(", ")),
                }
            }
            Expr::Set(node) => format!(
                "{{{}}}",
                node.elts.iter().map(|value| self.expression(value)).collect::<Vec<_>>().join(", ")
            ),
            Expr::Dict(node) => {
                let count = node.keys.len().max(node.values.len());
                let values = (0..count)
                    .map(|index| {
                        let key = node.keys.get(index).and_then(Option::as_ref);
                        let value = node.values.get(index);
                        match (key, value) {
                            (Some(key), Some(value)) => {
                                format!("{}: {}", self.expression(key), self.expression(value))
                            }
                            (None, Some(value)) => format!("**{}", self.expression(value)),
                            (Some(key), None) => format!("{}: None", self.expression(key)),
                            (None, None) => String::new(),
                        }
                    })
                    .filter(|value| !value.is_empty())
                    .collect::<Vec<_>>();
                format!("{{{}}}", values.join(", "))
            }
            Expr::IfExp(node) => format!(
                "({} if {} else {})",
                self.expression(&node.body),
                self.expression(&node.test),
                self.expression(&node.orelse)
            ),
            Expr::Lambda(node) => {
                format!("(lambda {}: {})", self.parameters(&node.args), self.expression(&node.body))
            }
            Expr::NamedExpr(node) => {
                format!("({} := {})", self.expression(&node.target), self.expression(&node.value))
            }
            Expr::Await(node) => format!(
                "(await {})",
                node.value.as_ref().map(|value| self.expression(value)).unwrap_or_default()
            ),
            Expr::Yield(node) => match &node.value {
                Some(value) => format!("(yield {})", self.expression(value)),
                None => "(yield)".into(),
            },
            Expr::YieldFrom(node) => match &node.value {
                Some(value) => format!("(yield from {})", self.expression(value)),
                None => "(yield from)".into(),
            },
            Expr::ListComp(node) => {
                format!("[{}{}]", self.expression(&node.elt), self.comprehensions(&node.generators))
            }
            Expr::SetComp(node) => {
                format!(
                    "{{{}{}}}",
                    self.expression(&node.elt),
                    self.comprehensions(&node.generators)
                )
            }
            Expr::DictComp(node) => {
                let key = node.key.as_ref().map(|value| self.expression(value)).unwrap_or_default();
                let value = node
                    .value
                    .as_ref()
                    .map(|value| self.expression(value))
                    .unwrap_or_else(|| self.expression(&node.elt));
                format!("{{{key}: {value}{}}}", self.comprehensions(&node.generators))
            }
            Expr::GeneratorExp(node) => {
                format!("({}{})", self.expression(&node.elt), self.comprehensions(&node.generators))
            }
            Expr::Slice(node) => format!(
                "{}:{}{}",
                node.lower.as_ref().map(|value| self.expression(value)).unwrap_or_default(),
                node.upper.as_ref().map(|value| self.expression(value)).unwrap_or_default(),
                node.step
                    .as_ref()
                    .map(|value| format!(":{}", self.expression(value)))
                    .unwrap_or_default()
            ),
            Expr::FString(node) => format!("f\"{}\"", self.fstring_text(&node.values)),
            Expr::FormattedValue(node) => {
                let conversion =
                    node.conversion.map(|value| format!("!{value}")).unwrap_or_default();
                let spec = node
                    .format_spec
                    .as_ref()
                    .map(|value| format!(":{}", self.expression(value)))
                    .unwrap_or_default();
                format!("{{{}{conversion}{spec}}}", self.expression(&node.value))
            }
            Expr::Starred(node) => format!("*{}", self.expression(&node.value)),
            Expr::Invalid(node) => format!("__pysyn_invalid__({})", repr_string(&node.message)),
        }
    }

    fn keyword(&self, keyword: &Keyword) -> String {
        match &keyword.arg {
            Some(arg) => format!("{arg}={}", self.expression(&keyword.value)),
            None => format!("**{}", self.expression(&keyword.value)),
        }
    }

    fn subscript_slice(&self, slice: &Expr) -> String {
        match slice {
            Expr::Tuple(node) if node.elts.is_empty() => "()".into(),
            Expr::Tuple(node) => {
                let values =
                    node.elts.iter().map(|value| self.expression(value)).collect::<Vec<_>>();
                if values.len() == 1 {
                    format!("{},", values[0])
                } else {
                    values.join(", ")
                }
            }
            other => self.expression(other),
        }
    }

    fn comprehensions(&self, generators: &[Comprehension]) -> String {
        generators
            .iter()
            .map(|generator| {
                let prefix = if generator.is_async { " async for " } else { " for " };
                let conditions = generator
                    .ifs
                    .iter()
                    .map(|condition| format!(" if {}", self.expression(condition)))
                    .collect::<String>();
                format!(
                    "{prefix}{} in {}{conditions}",
                    self.expression(&generator.target),
                    self.expression(&generator.iter)
                )
            })
            .collect()
    }

    fn pattern(&self, pattern: &Pattern) -> String {
        match pattern {
            Pattern::Value(node) => self.pattern_value_expression(&node.value),
            Pattern::Singleton(node) => self.expression(&node.value),
            Pattern::Sequence(node) => format!(
                "[{}]",
                node.patterns
                    .iter()
                    .map(|pattern| self.pattern(pattern))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Pattern::Mapping(node) => {
                let mut values = node
                    .keys
                    .iter()
                    .zip(&node.patterns)
                    .map(|(key, pattern)| {
                        format!("{}: {}", self.pattern_value_expression(key), self.pattern(pattern))
                    })
                    .collect::<Vec<_>>();
                if let Some(rest) = &node.rest {
                    values.push(format!("**{rest}"));
                }
                format!("{{{}}}", values.join(", "))
            }
            Pattern::Class(node) => {
                let positional = node.patterns.iter().map(|pattern| self.pattern(pattern));
                let keyword = node
                    .kwd_attrs
                    .iter()
                    .zip(&node.kwd_patterns)
                    .map(|(attr, pattern)| format!("{attr}={}", self.pattern(pattern)));
                format!(
                    "{}({})",
                    self.expression(&node.cls),
                    positional.chain(keyword).collect::<Vec<_>>().join(", ")
                )
            }
            Pattern::Star(node) => format!("*{}", node.name.as_deref().unwrap_or("_")),
            Pattern::As(node) => match (node.pattern.as_deref(), node.name.as_deref()) {
                (None, Some(name)) => name.into(),
                (None, None) => "_".into(),
                (Some(pattern), Some(name)) => {
                    let value = self.pattern(pattern);
                    if matches!(pattern, Pattern::As(_)) {
                        format!("({value}) as {name}")
                    } else {
                        format!("{value} as {name}")
                    }
                }
                (Some(pattern), None) => self.pattern(pattern),
            },
            Pattern::Or(node) => format!(
                "({})",
                node.patterns
                    .iter()
                    .map(|pattern| format!("({})", self.pattern(pattern)))
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
            Pattern::Invalid(_) => "_".into(),
        }
    }

    fn fstring_text(&self, values: &[Expr]) -> String {
        values
            .iter()
            .map(|value| match value {
                Expr::StringLiteral(node) => fstring_literal(&node.value.to_str()),
                Expr::FormattedValue(node) => {
                    let conversion =
                        node.conversion.map_or(String::new(), |value| format!("!{value}"));
                    let spec = node.format_spec.as_ref().map_or(String::new(), |value| match value
                        .as_ref()
                    {
                        Expr::FString(node) => format!(":{}", self.fstring_text(&node.values)),
                        other => format!(":{}", self.expression(other)),
                    });
                    format!("{{{}{conversion}{spec}}}", self.expression(&node.value))
                }
                _ => fstring_literal(&self.expression(value)),
            })
            .collect()
    }

    fn pattern_value_expression(&self, expression: &Expr) -> String {
        match expression {
            Expr::BinOp(node) => format!(
                "{} {} {}",
                self.pattern_value_expression(&node.left),
                binary_text(node.op),
                self.pattern_value_expression(&node.right)
            ),
            Expr::UnaryOp(node) => {
                format!("{}{}", unary_text(node.op), self.pattern_value_expression(&node.operand))
            }
            other => self.expression(other),
        }
    }
}

fn fstring_literal(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '{' => output.push_str("{{"),
            '}' => output.push_str("}}"),
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            other if other.is_control() => {
                let code = other as u32;
                if code <= 0xff {
                    output.push_str(&format!("\\x{code:02x}"));
                } else if code <= 0xffff {
                    output.push_str(&format!("\\u{code:04x}"));
                } else {
                    output.push_str(&format!("\\U{code:08x}"));
                }
            }
            other => output.push(other),
        }
    }
    output
}

fn alias_text(alias: &Alias) -> String {
    alias
        .asname
        .as_ref()
        .map(|asname| format!("{} as {asname}", alias.name))
        .unwrap_or_else(|| alias.name.to_string())
}

fn type_comment_suffix(type_comment: Option<&str>) -> String {
    type_comment.map(|value| format!(" # type: {value}")).unwrap_or_default()
}

fn binary_text(operator: BinaryOperator) -> &'static str {
    match operator {
        BinaryOperator::Add => "+",
        BinaryOperator::Sub => "-",
        BinaryOperator::Mult => "*",
        BinaryOperator::MatMult => "@",
        BinaryOperator::Div => "/",
        BinaryOperator::FloorDiv => "//",
        BinaryOperator::Mod => "%",
        BinaryOperator::Pow => "**",
        BinaryOperator::LShift => "<<",
        BinaryOperator::RShift => ">>",
        BinaryOperator::BitOr => "|",
        BinaryOperator::BitXor => "^",
        BinaryOperator::BitAnd => "&",
    }
}
fn unary_text(operator: UnaryOperator) -> &'static str {
    match operator {
        UnaryOperator::Invert => "~",
        UnaryOperator::Not => "not ",
        UnaryOperator::UAdd => "+",
        UnaryOperator::USub => "-",
    }
}
fn compare_text(operator: CompareOperator) -> &'static str {
    match operator {
        CompareOperator::Eq => "==",
        CompareOperator::NotEq => "!=",
        CompareOperator::Lt => "<",
        CompareOperator::LtE => "<=",
        CompareOperator::Gt => ">",
        CompareOperator::GtE => ">=",
        CompareOperator::In => "in",
        CompareOperator::NotIn => "not in",
        CompareOperator::Is => "is",
        CompareOperator::IsNot => "is not",
    }
}

#[allow(dead_code)]
fn _range(_node: &impl Ranged) -> TextRange {
    TextRange::default()
}
