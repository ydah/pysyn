//! AST visitor and non-recursive traversal support.

#![allow(missing_docs)]

use crate::ast::{AnyNodeRef, Expr, ModModule, Parameters, Pattern, Ranged, Stmt, TypeParam};

pub trait Visitor<'a> {
    fn visit_stmt(&mut self, node: &'a Stmt) {
        walk_stmt(self, node);
    }
    fn visit_expr(&mut self, node: &'a Expr) {
        walk_expr(self, node);
    }
    fn visit_pattern(&mut self, node: &'a Pattern) {
        walk_pattern(self, node);
    }
}

pub fn walk_stmt<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, node: &'a Stmt) {
    match node {
        Stmt::Expr(stmt) => visitor.visit_expr(&stmt.value),
        Stmt::Assign(stmt) => {
            for target in &stmt.targets {
                visitor.visit_expr(target);
            }
            visitor.visit_expr(&stmt.value);
        }
        Stmt::AnnAssign(stmt) => {
            visitor.visit_expr(&stmt.target);
            visitor.visit_expr(&stmt.annotation);
            if let Some(value) = &stmt.value {
                visitor.visit_expr(value);
            }
        }
        Stmt::AugAssign(stmt) => {
            visitor.visit_expr(&stmt.target);
            visitor.visit_expr(&stmt.value);
        }
        Stmt::Return(stmt) => {
            if let Some(value) = &stmt.value {
                visitor.visit_expr(value);
            }
        }
        Stmt::Delete(stmt) => {
            for target in &stmt.targets {
                visitor.visit_expr(target);
            }
        }
        Stmt::If(stmt) => {
            visitor.visit_expr(&stmt.test);
            for child in &stmt.body {
                visitor.visit_stmt(child);
            }
            for child in &stmt.orelse {
                visitor.visit_stmt(child);
            }
        }
        Stmt::While(stmt) => {
            visitor.visit_expr(&stmt.test);
            for child in &stmt.body {
                visitor.visit_stmt(child);
            }
            for child in &stmt.orelse {
                visitor.visit_stmt(child);
            }
        }
        Stmt::For(stmt) | Stmt::AsyncFor(stmt) => {
            visitor.visit_expr(&stmt.target);
            visitor.visit_expr(&stmt.iter);
            for child in &stmt.body {
                visitor.visit_stmt(child);
            }
            for child in &stmt.orelse {
                visitor.visit_stmt(child);
            }
        }
        Stmt::With(stmt) | Stmt::AsyncWith(stmt) => {
            for item in &stmt.items {
                visitor.visit_expr(&item.context_expr);
                if let Some(value) = &item.optional_vars {
                    visitor.visit_expr(value);
                }
            }
            for child in &stmt.body {
                visitor.visit_stmt(child);
            }
        }
        Stmt::FunctionDef(stmt) | Stmt::AsyncFunctionDef(stmt) => {
            for decorator in &stmt.decorator_list {
                visitor.visit_expr(decorator);
            }
            walk_type_params(visitor, &stmt.type_params);
            walk_parameters(visitor, &stmt.args);
            if let Some(value) = &stmt.returns {
                visitor.visit_expr(value);
            }
            for child in &stmt.body {
                visitor.visit_stmt(child);
            }
        }
        Stmt::ClassDef(stmt) => {
            for decorator in &stmt.decorator_list {
                visitor.visit_expr(decorator);
            }
            for base in &stmt.bases {
                visitor.visit_expr(base);
            }
            for keyword in &stmt.keywords {
                visitor.visit_expr(&keyword.value);
            }
            walk_type_params(visitor, &stmt.type_params);
            for child in &stmt.body {
                visitor.visit_stmt(child);
            }
        }
        Stmt::Assert(stmt) => {
            visitor.visit_expr(&stmt.test);
            if let Some(value) = &stmt.msg {
                visitor.visit_expr(value);
            }
        }
        Stmt::Raise(stmt) => {
            if let Some(value) = &stmt.exc {
                visitor.visit_expr(value);
            }
            if let Some(value) = &stmt.cause {
                visitor.visit_expr(value);
            }
        }
        Stmt::Import(_)
        | Stmt::ImportFrom(_)
        | Stmt::Global(_)
        | Stmt::Nonlocal(_)
        | Stmt::Pass(_)
        | Stmt::Break(_)
        | Stmt::Continue(_)
        | Stmt::Invalid(_) => {}
        Stmt::Try(stmt) | Stmt::TryStar(stmt) => {
            for child in &stmt.body {
                visitor.visit_stmt(child);
            }
            for handler in &stmt.handlers {
                if let Some(typ) = &handler.typ {
                    visitor.visit_expr(typ);
                }
                for child in &handler.body {
                    visitor.visit_stmt(child);
                }
            }
            for child in &stmt.orelse {
                visitor.visit_stmt(child);
            }
            for child in &stmt.finalbody {
                visitor.visit_stmt(child);
            }
        }
        Stmt::Match(stmt) => {
            visitor.visit_expr(&stmt.subject);
            for case in &stmt.cases {
                visitor.visit_pattern(&case.pattern);
                if let Some(guard) = &case.guard {
                    visitor.visit_expr(guard);
                }
                for child in &case.body {
                    visitor.visit_stmt(child);
                }
            }
        }
        Stmt::TypeAlias(stmt) => {
            visitor.visit_expr(&stmt.name);
            walk_type_params(visitor, &stmt.type_params);
            visitor.visit_expr(&stmt.value);
        }
    }
}

fn walk_parameters<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, parameters: &'a Parameters) {
    for parameter in parameters.posonlyargs.iter().chain(&parameters.args) {
        walk_parameter(visitor, parameter, None);
    }

    for (index, parameter) in parameters.kwonlyargs.iter().enumerate() {
        let fallback = parameters.kw_defaults.get(index).and_then(Option::as_ref);
        walk_parameter(visitor, parameter, fallback);
    }

    if let Some(parameter) = &parameters.vararg {
        walk_parameter(visitor, parameter, None);
    }
    if let Some(parameter) = &parameters.kwarg {
        walk_parameter(visitor, parameter, None);
    }

    let has_positional_defaults = parameters
        .posonlyargs
        .iter()
        .chain(&parameters.args)
        .any(|parameter| parameter.default.is_some());
    if !has_positional_defaults {
        for default in &parameters.defaults {
            visitor.visit_expr(default);
        }
    }
}

fn walk_parameter<'a, V: Visitor<'a> + ?Sized>(
    visitor: &mut V,
    parameter: &'a crate::ast::Parameter,
    fallback_default: Option<&'a Expr>,
) {
    if let Some(annotation) = &parameter.annotation {
        visitor.visit_expr(annotation);
    }
    if let Some(default) = &parameter.default {
        visitor.visit_expr(default);
    } else if let Some(default) = fallback_default {
        visitor.visit_expr(default);
    }
}

fn walk_type_params<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, type_params: &'a [TypeParam]) {
    for type_param in type_params {
        let data = match type_param {
            TypeParam::TypeVar(data)
            | TypeParam::ParamSpec(data)
            | TypeParam::TypeVarTuple(data) => data,
        };
        if let Some(bound) = &data.bound {
            visitor.visit_expr(bound);
        }
        if let Some(default) = &data.default {
            visitor.visit_expr(default);
        }
    }
}

pub fn walk_pattern<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, node: &'a Pattern) {
    match node {
        Pattern::Value(node) => visitor.visit_expr(&node.value),
        Pattern::Singleton(node) => visitor.visit_expr(&node.value),
        Pattern::Sequence(node) => {
            for pattern in &node.patterns {
                visitor.visit_pattern(pattern);
            }
        }
        Pattern::Mapping(node) => {
            for key in &node.keys {
                visitor.visit_expr(key);
            }
            for pattern in &node.patterns {
                visitor.visit_pattern(pattern);
            }
        }
        Pattern::Class(node) => {
            visitor.visit_expr(&node.cls);
            for pattern in &node.patterns {
                visitor.visit_pattern(pattern);
            }
            for pattern in &node.kwd_patterns {
                visitor.visit_pattern(pattern);
            }
        }
        Pattern::As(node) => {
            if let Some(pattern) = &node.pattern {
                visitor.visit_pattern(pattern);
            }
        }
        Pattern::Or(node) => {
            for pattern in &node.patterns {
                visitor.visit_pattern(pattern);
            }
        }
        Pattern::Star(_) | Pattern::Invalid(_) => {}
    }
}

pub fn walk_expr<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, node: &'a Expr) {
    match node {
        Expr::BoolOp(node) => {
            for value in &node.values {
                visitor.visit_expr(value);
            }
        }
        Expr::NamedExpr(node) => {
            visitor.visit_expr(&node.target);
            visitor.visit_expr(&node.value);
        }
        Expr::BinOp(node) => {
            visitor.visit_expr(&node.left);
            visitor.visit_expr(&node.right);
        }
        Expr::UnaryOp(node) => visitor.visit_expr(&node.operand),
        Expr::Lambda(node) => {
            walk_parameters(visitor, &node.args);
            visitor.visit_expr(&node.body);
        }
        Expr::IfExp(node) => {
            visitor.visit_expr(&node.body);
            visitor.visit_expr(&node.test);
            visitor.visit_expr(&node.orelse);
        }
        Expr::Dict(node) => {
            for key in node.keys.iter().flatten() {
                visitor.visit_expr(key);
            }
            for value in &node.values {
                visitor.visit_expr(value);
            }
        }
        Expr::Set(node) => {
            for value in &node.elts {
                visitor.visit_expr(value);
            }
        }
        Expr::List(node) | Expr::Tuple(node) => {
            for value in &node.elts {
                visitor.visit_expr(value);
            }
        }
        Expr::ListComp(node)
        | Expr::SetComp(node)
        | Expr::DictComp(node)
        | Expr::GeneratorExp(node) => {
            visitor.visit_expr(&node.elt);
            for generator in &node.generators {
                visitor.visit_expr(&generator.target);
                visitor.visit_expr(&generator.iter);
                for condition in &generator.ifs {
                    visitor.visit_expr(condition);
                }
            }
            if let Some(key) = &node.key {
                visitor.visit_expr(key);
            }
            if let Some(value) = &node.value {
                visitor.visit_expr(value);
            }
        }
        Expr::Await(node) | Expr::Yield(node) | Expr::YieldFrom(node) => {
            if let Some(value) = &node.value {
                visitor.visit_expr(value);
            }
        }
        Expr::Compare(node) => {
            visitor.visit_expr(&node.left);
            for value in &node.comparators {
                visitor.visit_expr(value);
            }
        }
        Expr::Call(node) => {
            visitor.visit_expr(&node.func);
            for value in &node.args {
                visitor.visit_expr(value);
            }
            for keyword in &node.keywords {
                visitor.visit_expr(&keyword.value);
            }
        }
        Expr::FString(node) => {
            for value in &node.values {
                visitor.visit_expr(value);
            }
        }
        Expr::FormattedValue(node) => {
            visitor.visit_expr(&node.value);
            if let Some(value) = &node.format_spec {
                visitor.visit_expr(value);
            }
        }
        Expr::Attribute(node) => visitor.visit_expr(&node.value),
        Expr::Subscript(node) => {
            visitor.visit_expr(&node.value);
            visitor.visit_expr(&node.slice);
        }
        Expr::Starred(node) => visitor.visit_expr(&node.value),
        Expr::StringLiteral(_)
        | Expr::BytesLiteral(_)
        | Expr::NumberLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::NoneLiteral(_)
        | Expr::EllipsisLiteral(_)
        | Expr::Name(_)
        | Expr::Slice(_)
        | Expr::Invalid(_) => {}
    }
}

pub fn preorder(module: &ModModule) -> impl Iterator<Item = AnyNodeRef<'_>> {
    let mut nodes = Vec::new();
    for statement in &module.body {
        collect_stmt(statement, &mut nodes);
    }
    nodes.into_iter()
}

fn collect_stmt<'tree>(statement: &'tree Stmt, nodes: &mut Vec<AnyNodeRef<'tree>>) {
    nodes.push(AnyNodeRef::Stmt(statement));
    struct Collector<'tree, 'vec>(&'vec mut Vec<AnyNodeRef<'tree>>);
    impl<'tree, 'vec> Visitor<'tree> for Collector<'tree, 'vec> {
        fn visit_expr(&mut self, node: &'tree Expr) {
            self.0.push(AnyNodeRef::Expr(node));
            walk_expr(self, node);
        }
        fn visit_stmt(&mut self, node: &'tree Stmt) {
            self.0.push(AnyNodeRef::Stmt(node));
            walk_stmt(self, node);
        }
        fn visit_pattern(&mut self, node: &'tree Pattern) {
            self.0.push(AnyNodeRef::Pattern(node));
            walk_pattern(self, node);
        }
    }
    let mut collector = Collector(nodes);
    walk_stmt(&mut collector, statement);
}

pub trait Transformer {
    fn transform_expr(&mut self, node: Expr) -> Expr {
        node
    }
}

impl Ranged for AnyNodeRef<'_> {
    fn range(&self) -> crate::source::TextRange {
        match self {
            Self::Stmt(node) => node.range(),
            Self::Expr(node) => node.range(),
            Self::Pattern(node) => node.range(),
        }
    }
}
