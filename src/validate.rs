//! Post-parse syntax and semantic validation.

#![allow(missing_docs)]

use crate::ast::{Expr, ExprContext, ModModule, Stmt};
use crate::error::{Diagnostic, DiagnosticCode};
use crate::visit::{walk_expr, Visitor};

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
        Stmt::Return(node) => {
            if level == ValidateLevel::Semantic && !in_function {
                diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Validation,
                    node.range,
                    "'return' outside function",
                ));
            }
            if let Some(value) = &node.value {
                validate_expr(value, diagnostics);
            }
        }
        Stmt::For(node) | Stmt::AsyncFor(node) => {
            validate_expr(&node.iter, diagnostics);
            validate_expr(&node.target, diagnostics);
            validate_block(&node.body, level, diagnostics, in_function, true);
            validate_block(&node.orelse, level, diagnostics, in_function, in_loop);
        }
        Stmt::While(node) => {
            validate_expr(&node.test, diagnostics);
            validate_block(&node.body, level, diagnostics, in_function, true);
            validate_block(&node.orelse, level, diagnostics, in_function, in_loop);
        }
        Stmt::FunctionDef(node) | Stmt::AsyncFunctionDef(node) => {
            for decorator in &node.decorator_list {
                validate_expr(decorator, diagnostics);
            }
            if let Some(returns) = &node.returns {
                validate_expr(returns, diagnostics);
            }
            for parameter in node.args.posonlyargs.iter().chain(&node.args.args) {
                if let Some(annotation) = &parameter.annotation {
                    validate_expr(annotation, diagnostics);
                }
                if let Some(default) = &parameter.default {
                    validate_expr(default, diagnostics);
                }
            }
            validate_block(&node.body, level, diagnostics, true, false);
        }
        Stmt::ClassDef(node) => {
            for base in &node.bases {
                validate_expr(base, diagnostics);
            }
            for keyword in &node.keywords {
                validate_expr(&keyword.value, diagnostics);
            }
            validate_block(&node.body, level, diagnostics, false, false);
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
            validate_expr(&node.test, diagnostics);
            validate_block(&node.body, level, diagnostics, in_function, in_loop);
            validate_block(&node.orelse, level, diagnostics, in_function, in_loop);
        }
        Stmt::With(node) | Stmt::AsyncWith(node) => {
            for item in &node.items {
                validate_expr(&item.context_expr, diagnostics);
                if let Some(target) = &item.optional_vars {
                    validate_expr(target, diagnostics);
                }
            }
            validate_block(&node.body, level, diagnostics, in_function, in_loop);
        }
        Stmt::Try(node) | Stmt::TryStar(node) => {
            validate_block(&node.body, level, diagnostics, in_function, in_loop);
            for handler in &node.handlers {
                if let Some(typ) = &handler.typ {
                    validate_expr(typ, diagnostics);
                }
                validate_block(&handler.body, level, diagnostics, in_function, in_loop);
            }
            validate_block(&node.orelse, level, diagnostics, in_function, in_loop);
            validate_block(&node.finalbody, level, diagnostics, in_function, in_loop);
        }
        Stmt::Match(node) => {
            validate_expr(&node.subject, diagnostics);
            for case in &node.cases {
                if let Some(guard) = &case.guard {
                    validate_expr(guard, diagnostics);
                }
                validate_block(&case.body, level, diagnostics, in_function, in_loop);
            }
        }
        Stmt::Raise(node) => {
            if let Some(value) = &node.exc {
                validate_expr(value, diagnostics);
            }
            if let Some(value) = &node.cause {
                validate_expr(value, diagnostics);
            }
        }
        Stmt::Assert(node) => {
            validate_expr(&node.test, diagnostics);
            if let Some(value) = &node.msg {
                validate_expr(value, diagnostics);
            }
        }
        Stmt::Delete(node) => {
            for target in &node.targets {
                validate_expr(target, diagnostics);
            }
        }
        Stmt::Assign(node) => {
            for target in &node.targets {
                validate_expr(target, diagnostics);
            }
            validate_expr(&node.value, diagnostics);
        }
        Stmt::AnnAssign(node) => {
            validate_expr(&node.target, diagnostics);
            validate_expr(&node.annotation, diagnostics);
            if let Some(value) = &node.value {
                validate_expr(value, diagnostics);
            }
        }
        Stmt::AugAssign(node) => {
            validate_expr(&node.target, diagnostics);
            validate_expr(&node.value, diagnostics);
        }
        Stmt::TypeAlias(node) => {
            validate_expr(&node.name, diagnostics);
            validate_expr(&node.value, diagnostics);
        }
        Stmt::Expr(node) => validate_expr(&node.value, diagnostics),
        Stmt::Nonlocal(node) if level == ValidateLevel::Semantic && !in_function => {
            diagnostics.push(Diagnostic::error(
                DiagnosticCode::Validation,
                node.range,
                "nonlocal declaration outside function",
            ));
        }
        _ => {}
    }
}

fn validate_block(
    body: &[Stmt],
    level: ValidateLevel,
    diagnostics: &mut Vec<Diagnostic>,
    in_function: bool,
    in_loop: bool,
) {
    for statement in body {
        validate_stmt(statement, level, diagnostics, in_function, in_loop);
    }
}

fn validate_expr(expr: &Expr, diagnostics: &mut Vec<Diagnostic>) {
    struct ExprValidator<'diagnostics> {
        diagnostics: &'diagnostics mut Vec<Diagnostic>,
    }
    impl<'tree, 'diagnostics> Visitor<'tree> for ExprValidator<'diagnostics> {
        fn visit_expr(&mut self, expr: &'tree Expr) {
            if let Expr::Name(node) = expr {
                if node.ctx != ExprContext::Load && node.id.is_empty() {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Validation,
                        node.range,
                        "empty identifier",
                    ));
                }
            }
            walk_expr(self, expr);
        }
    }
    let mut validator = ExprValidator { diagnostics };
    validator.visit_expr(expr);
}
