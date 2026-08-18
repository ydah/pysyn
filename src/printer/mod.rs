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
            for (index, target) in node.targets.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                dump_expr(target, options, level, output);
            }
            output.push_str("], value=");
            dump_expr(&node.value, options, level, output);
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
            output.push_str("Return(return=");
            if let Some(value) = &node.value {
                dump_expr(value, options, level, output);
            } else {
                output.push_str("None");
            }
            output.push(')');
        }
        Stmt::Delete(node) => {
            output.push_str("Delete(targets=[");
            for (index, target) in node.targets.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                dump_expr(target, options, level, output);
            }
            output.push_str("])");
        }
        Stmt::If(node) => {
            output.push_str("If(test=");
            dump_expr(&node.test, options, level, output);
            output.push_str(", body=[");
            for (index, child) in node.body.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                dump_stmt(child, options, level + 1, output);
            }
            output.push_str("], orelse=[");
            for (index, child) in node.orelse.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                dump_stmt(child, options, level + 1, output);
            }
            output.push_str("])");
        }
        Stmt::While(node) => {
            output.push_str("While(test=");
            dump_expr(&node.test, options, level, output);
            output.push_str(", body=[");
            for (index, child) in node.body.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                dump_stmt(child, options, level + 1, output);
            }
            output.push_str("], orelse=[])");
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
            for (index, child) in node.body.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                dump_stmt(child, options, level + 1, output);
            }
            output.push_str("], orelse=[])");
        }
        Stmt::FunctionDef(node) | Stmt::AsyncFunctionDef(node) => {
            output.push_str(if matches!(statement, Stmt::AsyncFunctionDef(_)) {
                "AsyncFunctionDef(name="
            } else {
                "FunctionDef(name="
            });
            output.push_str(&repr_string(&node.name));
            output.push_str(", body=[");
            for (index, child) in node.body.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                dump_stmt(child, options, level + 1, output);
            }
            output.push_str("])");
        }
        Stmt::ClassDef(node) => {
            output.push_str("ClassDef(name=");
            output.push_str(&repr_string(&node.name));
            output.push_str(", body=[");
            for (index, child) in node.body.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                dump_stmt(child, options, level + 1, output);
            }
            output.push_str("])");
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
            for (index, alias) in node.names.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                output.push_str("alias(name=");
                output.push_str(&repr_string(&alias.name));
                output.push(')');
            }
            output.push_str("])");
        }
        Stmt::ImportFrom(node) => {
            output.push_str("ImportFrom(module=");
            if let Some(module) = &node.module {
                output.push_str(&repr_string(module));
            } else {
                output.push_str("None");
            }
            output.push_str(", names=[])");
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
            for (index, item) in node.items.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                output.push_str("withitem(context_expr=");
                dump_expr(&item.context_expr, options, level, output);
                output.push_str(", optional_vars=None)");
            }
            output.push_str("], body=[])");
        }
        Stmt::Try(_) | Stmt::TryStar(_) => output
            .push_str(if matches!(statement, Stmt::TryStar(_)) { "TryStar()" } else { "Try()" }),
        Stmt::AugAssign(_) => output.push_str("AugAssign()"),
        Stmt::Match(_) => output.push_str("Match()"),
        Stmt::TypeAlias(_) => output.push_str("TypeAlias()"),
        Stmt::Invalid(_) => output.push_str("Invalid()"),
    }
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
            output.push(')');
        }
        Expr::BytesLiteral(node) => {
            output.push_str("Constant(value=");
            output.push('b');
            output.push_str(&repr_string(&node.value.to_str()));
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
            for (index, value) in node.args.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                dump_expr(value, options, level, output);
            }
            output.push_str("], keywords=[])");
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
        Expr::Lambda(_) => output.push_str("Lambda()"),
        Expr::NamedExpr(_) => output.push_str("NamedExpr()"),
        Expr::Await(_) => output.push_str("Await()"),
        Expr::Yield(_) => output.push_str("Yield()"),
        Expr::YieldFrom(_) => output.push_str("YieldFrom()"),
        Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) | Expr::GeneratorExp(_) => {
            output.push_str("comprehension()")
        }
        Expr::Slice(_) => output.push_str("Slice()"),
        Expr::Invalid(_) => output.push_str("Invalid()"),
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
        Number::Float(value) => value.to_string(),
        Number::Complex { real, imag } => format!("({real}+{imag}j)"),
    }
}
fn repr_string(value: &str) -> String {
    let mut output = String::from("'");
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '\'' => output.push_str("\\'"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            other => output.push(other),
        }
    }
    output.push('\'');
    output
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
                self.line(&format!("{targets} = {}", self.expression(&node.value)));
            }
            Stmt::AnnAssign(node) => self.line(&format!(
                "{}: {}{}",
                self.expression(&node.target),
                self.expression(&node.annotation),
                node.value
                    .as_ref()
                    .map(|value| format!(" = {}", self.expression(value)))
                    .unwrap_or_default()
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
                node.names
                    .iter()
                    .map(|alias| alias.name.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            Stmt::ImportFrom(node) => self.line(&format!(
                "from {} import {}",
                node.module.as_deref().unwrap_or("."),
                node.names
                    .iter()
                    .map(|alias| alias.name.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            Stmt::Global(node) => self.line(&format!(
                "global {}",
                node.names.iter().map(|name| name.as_ref()).collect::<Vec<_>>().join(", ")
            )),
            Stmt::Nonlocal(node) => self.line(&format!(
                "nonlocal {}",
                node.names.iter().map(|name| name.as_ref()).collect::<Vec<_>>().join(", ")
            )),
            Stmt::If(node) => {
                self.block_header(&format!("if {}:", self.expression(&node.test)), &node.body);
                for child in &node.orelse {
                    if let Stmt::If(nested) = child {
                        self.block_header(
                            &format!("elif {}:", self.expression(&nested.test)),
                            &nested.body,
                        );
                    } else {
                        self.block_header("else:", &node.orelse);
                        break;
                    }
                }
            }
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
                        "{prefix}for {} in {}:",
                        self.expression(&node.target),
                        self.expression(&node.iter)
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
                self.block_header(&format!("{prefix}with {items}:"), &node.body);
            }
            Stmt::FunctionDef(node) | Stmt::AsyncFunctionDef(node) => {
                let prefix =
                    if matches!(statement, Stmt::AsyncFunctionDef(_)) { "async " } else { "" };
                self.block_header(&format!("{prefix}def {}():", node.name), &node.body);
            }
            Stmt::ClassDef(node) => self.block_header(&format!("class {}:", node.name), &node.body),
            _ => self.line("pass"),
        }
    }
    fn block_header(&mut self, header: &str, body: &[Stmt]) {
        self.line(header);
        self.indent += 1;
        for statement in body {
            self.statement(statement);
        }
        self.indent = self.indent.saturating_sub(1);
    }
    fn line(&mut self, value: &str) {
        self.output.push_str(&"    ".repeat(self.indent));
        self.output.push_str(value);
        self.output.push('\n');
    }
    fn expression(&self, expression: &Expr) -> String {
        match expression {
            Expr::Name(node) => node.id.to_string(),
            Expr::NumberLiteral(node) => node.raw.to_string(),
            Expr::StringLiteral(node) => repr_string(&node.value.to_str()),
            Expr::BytesLiteral(node) => format!("b{}", repr_string(&node.value.to_str())),
            Expr::BooleanLiteral(node) => node.value.to_string(),
            Expr::NoneLiteral(_) => "None".into(),
            Expr::EllipsisLiteral(_) => "...".into(),
            Expr::Attribute(node) => format!("{}.{}", self.expression(&node.value), node.attr),
            Expr::Call(node) => format!(
                "{}({})",
                self.expression(&node.func),
                node.args.iter().map(|arg| self.expression(arg)).collect::<Vec<_>>().join(", ")
            ),
            Expr::Subscript(node) => {
                format!("{}[{}]", self.expression(&node.value), self.expression(&node.slice))
            }
            Expr::BinOp(node) => format!(
                "{} {} {}",
                self.expression(&node.left),
                binary_text(node.op),
                self.expression(&node.right)
            ),
            Expr::UnaryOp(node) => {
                format!("{}{}", unary_text(node.op), self.expression(&node.operand))
            }
            Expr::BoolOp(node) => node
                .values
                .iter()
                .map(|value| self.expression(value))
                .collect::<Vec<_>>()
                .join(if node.op == BoolOperator::And { " and " } else { " or " }),
            Expr::Compare(node) => {
                let mut result = self.expression(&node.left);
                for (op, value) in node.ops.iter().zip(&node.comparators) {
                    result.push(' ');
                    result.push_str(compare_text(*op));
                    result.push(' ');
                    result.push_str(&self.expression(value));
                }
                result
            }
            Expr::List(node) => format!(
                "[{}]",
                node.elts.iter().map(|value| self.expression(value)).collect::<Vec<_>>().join(", ")
            ),
            Expr::Tuple(node) => format!(
                "({})",
                node.elts.iter().map(|value| self.expression(value)).collect::<Vec<_>>().join(", ")
            ),
            Expr::Set(node) => format!(
                "{{{}}}",
                node.elts.iter().map(|value| self.expression(value)).collect::<Vec<_>>().join(", ")
            ),
            Expr::Dict(node) => node
                .keys
                .iter()
                .zip(&node.values)
                .map(|(key, value)| {
                    format!(
                        "{}: {}",
                        key.as_ref().map(|key| self.expression(key)).unwrap_or_default(),
                        self.expression(value)
                    )
                })
                .collect::<Vec<_>>()
                .join(", "),
            Expr::IfExp(node) => format!(
                "{} if {} else {}",
                self.expression(&node.body),
                self.expression(&node.test),
                self.expression(&node.orelse)
            ),
            Expr::FString(node) => format!("f\"{}\"", self.fstring_text(&node.values)),
            Expr::FormattedValue(node) => format!("{{{}}}", self.expression(&node.value)),
            Expr::Starred(node) => format!("*{}", self.expression(&node.value)),
            _ => "...".into(),
        }
    }

    fn fstring_text(&self, values: &[Expr]) -> String {
        values
            .iter()
            .map(|value| match value {
                Expr::StringLiteral(node) => node.value.to_str().into_owned(),
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
                _ => self.expression(value),
            })
            .collect()
    }
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
