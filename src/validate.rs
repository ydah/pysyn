//! Post-parse syntax and semantic validation.

#![allow(missing_docs)]

use crate::ast::{Expr, ExprContext, ModModule, Stmt};
use crate::error::{Diagnostic, DiagnosticCode};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ValidateLevel {
    Syntax,
    Semantic,
}

pub fn validate(module: &ModModule, level: ValidateLevel) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for statement in &module.body {
        validate_stmt(statement, level, &mut diagnostics, false, false);
    }
    diagnostics
}

fn validate_stmt(
    statement: &Stmt,
    level: ValidateLevel,
    diagnostics: &mut Vec<Diagnostic>,
    in_function: bool,
    in_loop: bool,
) {
    match statement {
        Stmt::Return(node) if level == ValidateLevel::Semantic && !in_function => diagnostics.push(
            Diagnostic::error(DiagnosticCode::Validation, node.range, "'return' outside function"),
        ),
        Stmt::For(node) | Stmt::AsyncFor(node) => {
            validate_expr(&node.target, diagnostics);
            for child in &node.body {
                validate_stmt(child, level, diagnostics, in_function, true);
            }
            for child in &node.orelse {
                validate_stmt(child, level, diagnostics, in_function, in_loop);
            }
        }
        Stmt::While(node) => {
            for child in &node.body {
                validate_stmt(child, level, diagnostics, in_function, true);
            }
            for child in &node.orelse {
                validate_stmt(child, level, diagnostics, in_function, in_loop);
            }
        }
        Stmt::FunctionDef(node) | Stmt::AsyncFunctionDef(node) => {
            for child in &node.body {
                validate_stmt(child, level, diagnostics, true, false);
            }
        }
        Stmt::Break(node) | Stmt::Continue(node)
            if level == ValidateLevel::Semantic && !in_loop =>
        {
            diagnostics.push(Diagnostic::error(
                DiagnosticCode::Validation,
                node.range,
                "loop control statement outside loop",
            ))
        }
        Stmt::If(node) => {
            for child in &node.body {
                validate_stmt(child, level, diagnostics, in_function, in_loop);
            }
            for child in &node.orelse {
                validate_stmt(child, level, diagnostics, in_function, in_loop);
            }
        }
        Stmt::Expr(node) => validate_expr(&node.value, diagnostics),
        _ => {}
    }
}

fn validate_expr(expr: &Expr, diagnostics: &mut Vec<Diagnostic>) {
    if let Expr::Name(node) = expr {
        if node.ctx != ExprContext::Load && node.id.is_empty() {
            diagnostics.push(Diagnostic::error(
                DiagnosticCode::Validation,
                node.range,
                "empty identifier",
            ));
        }
    }
}
