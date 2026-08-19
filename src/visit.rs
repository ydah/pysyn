//! AST visitor and non-recursive traversal support.

//! [`Visitor`] is read-only and recursive by default; [`preorder`] adds a
//! type-erased event stream suitable for indexing every node kind.
use crate::ast::*;

/// Public API item.
pub trait Visitor<'a> {
    /// Visits or transforms the node.
    fn visit_stmt(&mut self, node: &'a Stmt) {
        walk_stmt(self, node);
    }
    /// Visits or transforms the node.
    fn visit_expr(&mut self, node: &'a Expr) {
        walk_expr(self, node);
    }
    /// Visits or transforms the node.
    fn visit_pattern(&mut self, node: &'a Pattern) {
        walk_pattern(self, node);
    }
}

/// Performs this public operation.
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

    for default in &parameters.defaults {
        visitor.visit_expr(default);
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
    if let Some(default) = fallback_default {
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

/// Performs this public operation.
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

/// Performs this public operation.
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

/// Performs this public operation.
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

/// Owns an AST transformation callback.
///
/// The default callbacks preserve their input. Use the transformation
/// functions in this module to get a post-order traversal; each child is
/// transformed before its parent callback.
pub trait Transformer {
    /// Replaces or preserves an expression after its children were transformed.
    fn transform_expr(&mut self, node: Expr) -> Expr {
        node
    }
    /// Replaces or preserves a statement after its children were transformed.
    fn transform_stmt(&mut self, node: Stmt) -> Stmt {
        node
    }
    /// Replaces or preserves a pattern after its children were transformed.
    fn transform_pattern(&mut self, node: Pattern) -> Pattern {
        node
    }
}

/// Transforms every statement in an owned module in source order.
pub fn transform_module<T: Transformer>(transformer: &mut T, mut module: ModModule) -> ModModule {
    module.body =
        module.body.into_iter().map(|statement| transform_stmt(transformer, statement)).collect();
    module
}

/// Transforms one owned statement and all of its descendants.
pub fn transform_stmt<T: Transformer>(transformer: &mut T, statement: Stmt) -> Stmt {
    let statement = match statement {
        Stmt::FunctionDef(mut node) => {
            node.decorator_list = transform_expressions(transformer, node.decorator_list);
            node.type_params = transform_type_params(transformer, node.type_params);
            node.args = transform_parameters(transformer, node.args);
            node.returns = node.returns.map(|expr| Box::new(transform_expr(transformer, *expr)));
            node.body = transform_statements(transformer, node.body);
            Stmt::FunctionDef(node)
        }
        Stmt::AsyncFunctionDef(mut node) => {
            node.decorator_list = transform_expressions(transformer, node.decorator_list);
            node.type_params = transform_type_params(transformer, node.type_params);
            node.args = transform_parameters(transformer, node.args);
            node.returns = node.returns.map(|expr| Box::new(transform_expr(transformer, *expr)));
            node.body = transform_statements(transformer, node.body);
            Stmt::AsyncFunctionDef(node)
        }
        Stmt::ClassDef(mut node) => {
            node.bases = transform_expressions(transformer, node.bases);
            node.keywords = node
                .keywords
                .into_iter()
                .map(|keyword| transform_keyword(transformer, keyword))
                .collect();
            node.decorator_list = transform_expressions(transformer, node.decorator_list);
            node.type_params = transform_type_params(transformer, node.type_params);
            node.body = transform_statements(transformer, node.body);
            Stmt::ClassDef(node)
        }
        Stmt::Return(mut node) => {
            node.value = node.value.map(|expr| Box::new(transform_expr(transformer, *expr)));
            Stmt::Return(node)
        }
        Stmt::Delete(mut node) => {
            node.targets = transform_expressions(transformer, node.targets);
            Stmt::Delete(node)
        }
        Stmt::Assign(mut node) => {
            node.targets = transform_expressions(transformer, node.targets);
            node.value = Box::new(transform_expr(transformer, *node.value));
            Stmt::Assign(node)
        }
        Stmt::TypeAlias(mut node) => {
            node.name = Box::new(transform_expr(transformer, *node.name));
            node.type_params = transform_type_params(transformer, node.type_params);
            node.value = Box::new(transform_expr(transformer, *node.value));
            Stmt::TypeAlias(node)
        }
        Stmt::AugAssign(mut node) => {
            node.target = Box::new(transform_expr(transformer, *node.target));
            node.value = Box::new(transform_expr(transformer, *node.value));
            Stmt::AugAssign(node)
        }
        Stmt::AnnAssign(mut node) => {
            node.target = Box::new(transform_expr(transformer, *node.target));
            node.annotation = Box::new(transform_expr(transformer, *node.annotation));
            node.value = node.value.map(|expr| Box::new(transform_expr(transformer, *expr)));
            Stmt::AnnAssign(node)
        }
        Stmt::For(mut node) => {
            node.target = Box::new(transform_expr(transformer, *node.target));
            node.iter = Box::new(transform_expr(transformer, *node.iter));
            node.body = transform_statements(transformer, node.body);
            node.orelse = transform_statements(transformer, node.orelse);
            Stmt::For(node)
        }
        Stmt::AsyncFor(mut node) => {
            node.target = Box::new(transform_expr(transformer, *node.target));
            node.iter = Box::new(transform_expr(transformer, *node.iter));
            node.body = transform_statements(transformer, node.body);
            node.orelse = transform_statements(transformer, node.orelse);
            Stmt::AsyncFor(node)
        }
        Stmt::While(mut node) => {
            node.test = Box::new(transform_expr(transformer, *node.test));
            node.body = transform_statements(transformer, node.body);
            node.orelse = transform_statements(transformer, node.orelse);
            Stmt::While(node)
        }
        Stmt::If(mut node) => {
            node.test = Box::new(transform_expr(transformer, *node.test));
            node.body = transform_statements(transformer, node.body);
            node.orelse = transform_statements(transformer, node.orelse);
            Stmt::If(node)
        }
        Stmt::With(mut node) => {
            node.items =
                node.items.into_iter().map(|item| transform_with_item(transformer, item)).collect();
            node.body = transform_statements(transformer, node.body);
            Stmt::With(node)
        }
        Stmt::AsyncWith(mut node) => {
            node.items =
                node.items.into_iter().map(|item| transform_with_item(transformer, item)).collect();
            node.body = transform_statements(transformer, node.body);
            Stmt::AsyncWith(node)
        }
        Stmt::Match(mut node) => {
            node.subject = Box::new(transform_expr(transformer, *node.subject));
            node.cases = node
                .cases
                .into_iter()
                .map(|case| {
                    let MatchCase { range, pattern, guard, body } = case;
                    MatchCase {
                        range,
                        pattern: transform_pattern(transformer, pattern),
                        guard: guard.map(|expr| transform_expr(transformer, expr)),
                        body: transform_statements(transformer, body),
                    }
                })
                .collect();
            Stmt::Match(node)
        }
        Stmt::Raise(mut node) => {
            node.exc = node.exc.map(|expr| Box::new(transform_expr(transformer, *expr)));
            node.cause = node.cause.map(|expr| Box::new(transform_expr(transformer, *expr)));
            Stmt::Raise(node)
        }
        Stmt::Try(mut node) => {
            node.body = transform_statements(transformer, node.body);
            node.handlers = node
                .handlers
                .into_iter()
                .map(|handler| transform_except_handler(transformer, handler))
                .collect();
            node.orelse = transform_statements(transformer, node.orelse);
            node.finalbody = transform_statements(transformer, node.finalbody);
            Stmt::Try(node)
        }
        Stmt::TryStar(mut node) => {
            node.body = transform_statements(transformer, node.body);
            node.handlers = node
                .handlers
                .into_iter()
                .map(|handler| transform_except_handler(transformer, handler))
                .collect();
            node.orelse = transform_statements(transformer, node.orelse);
            node.finalbody = transform_statements(transformer, node.finalbody);
            Stmt::TryStar(node)
        }
        Stmt::Assert(mut node) => {
            node.test = Box::new(transform_expr(transformer, *node.test));
            node.msg = node.msg.map(|expr| Box::new(transform_expr(transformer, *expr)));
            Stmt::Assert(node)
        }
        Stmt::Expr(mut node) => {
            node.value = Box::new(transform_expr(transformer, *node.value));
            Stmt::Expr(node)
        }
        other => other,
    };
    transformer.transform_stmt(statement)
}

fn transform_statements<T: Transformer>(transformer: &mut T, body: Vec<Stmt>) -> Vec<Stmt> {
    body.into_iter().map(|statement| transform_stmt(transformer, statement)).collect()
}

/// Transforms one owned expression and all of its descendants.
pub fn transform_expr<T: Transformer>(transformer: &mut T, expression: Expr) -> Expr {
    let expression = match expression {
        Expr::BoolOp(mut node) => {
            node.values = transform_expressions(transformer, node.values);
            Expr::BoolOp(node)
        }
        Expr::NamedExpr(mut node) => {
            node.target = Box::new(transform_expr(transformer, *node.target));
            node.value = Box::new(transform_expr(transformer, *node.value));
            Expr::NamedExpr(node)
        }
        Expr::BinOp(mut node) => {
            node.left = Box::new(transform_expr(transformer, *node.left));
            node.right = Box::new(transform_expr(transformer, *node.right));
            Expr::BinOp(node)
        }
        Expr::UnaryOp(mut node) => {
            node.operand = Box::new(transform_expr(transformer, *node.operand));
            Expr::UnaryOp(node)
        }
        Expr::Lambda(mut node) => {
            node.args = transform_parameters(transformer, node.args);
            node.body = Box::new(transform_expr(transformer, *node.body));
            Expr::Lambda(node)
        }
        Expr::IfExp(mut node) => {
            node.body = Box::new(transform_expr(transformer, *node.body));
            node.test = Box::new(transform_expr(transformer, *node.test));
            node.orelse = Box::new(transform_expr(transformer, *node.orelse));
            Expr::IfExp(node)
        }
        Expr::Dict(mut node) => {
            node.keys = node
                .keys
                .into_iter()
                .map(|expr| expr.map(|expr| transform_expr(transformer, expr)))
                .collect();
            node.values = transform_expressions(transformer, node.values);
            Expr::Dict(node)
        }
        Expr::Set(mut node) => {
            node.elts = transform_expressions(transformer, node.elts);
            Expr::Set(node)
        }
        Expr::ListComp(mut node) => {
            node = transform_comprehension(transformer, node);
            Expr::ListComp(node)
        }
        Expr::SetComp(mut node) => {
            node = transform_comprehension(transformer, node);
            Expr::SetComp(node)
        }
        Expr::DictComp(mut node) => {
            node = transform_comprehension(transformer, node);
            Expr::DictComp(node)
        }
        Expr::GeneratorExp(mut node) => {
            node = transform_comprehension(transformer, node);
            Expr::GeneratorExp(node)
        }
        Expr::Await(mut node) => {
            node.value = node.value.map(|expr| Box::new(transform_expr(transformer, *expr)));
            Expr::Await(node)
        }
        Expr::Yield(mut node) => {
            node.value = node.value.map(|expr| Box::new(transform_expr(transformer, *expr)));
            Expr::Yield(node)
        }
        Expr::YieldFrom(mut node) => {
            node.value = node.value.map(|expr| Box::new(transform_expr(transformer, *expr)));
            Expr::YieldFrom(node)
        }
        Expr::Compare(mut node) => {
            node.left = Box::new(transform_expr(transformer, *node.left));
            node.comparators = transform_expressions(transformer, node.comparators);
            Expr::Compare(node)
        }
        Expr::Call(mut node) => {
            node.func = Box::new(transform_expr(transformer, *node.func));
            node.args = transform_expressions(transformer, node.args);
            node.keywords = node
                .keywords
                .into_iter()
                .map(|keyword| transform_keyword(transformer, keyword))
                .collect();
            Expr::Call(node)
        }
        Expr::FString(mut node) => {
            node.values = transform_expressions(transformer, node.values);
            Expr::FString(node)
        }
        Expr::FormattedValue(mut node) => {
            node.value = Box::new(transform_expr(transformer, *node.value));
            node.format_spec =
                node.format_spec.map(|expr| Box::new(transform_expr(transformer, *expr)));
            Expr::FormattedValue(node)
        }
        Expr::Attribute(mut node) => {
            node.value = Box::new(transform_expr(transformer, *node.value));
            Expr::Attribute(node)
        }
        Expr::Subscript(mut node) => {
            node.value = Box::new(transform_expr(transformer, *node.value));
            node.slice = Box::new(transform_expr(transformer, *node.slice));
            Expr::Subscript(node)
        }
        Expr::Starred(mut node) => {
            node.value = Box::new(transform_expr(transformer, *node.value));
            Expr::Starred(node)
        }
        Expr::List(mut node) => {
            node.elts = transform_expressions(transformer, node.elts);
            Expr::List(node)
        }
        Expr::Tuple(mut node) => {
            node.elts = transform_expressions(transformer, node.elts);
            Expr::Tuple(node)
        }
        Expr::Slice(mut node) => {
            node.lower = node.lower.map(|expr| Box::new(transform_expr(transformer, *expr)));
            node.upper = node.upper.map(|expr| Box::new(transform_expr(transformer, *expr)));
            node.step = node.step.map(|expr| Box::new(transform_expr(transformer, *expr)));
            Expr::Slice(node)
        }
        leaf => leaf,
    };
    transformer.transform_expr(expression)
}

fn transform_expressions<T: Transformer>(transformer: &mut T, values: Vec<Expr>) -> Vec<Expr> {
    values.into_iter().map(|expr| transform_expr(transformer, expr)).collect()
}

fn transform_comprehension<T: Transformer>(
    transformer: &mut T,
    mut node: ExprComprehension,
) -> ExprComprehension {
    node.elt = Box::new(transform_expr(transformer, *node.elt));
    node.generators = node
        .generators
        .drain(..)
        .map(|mut generator| {
            generator.target = transform_expr(transformer, generator.target);
            generator.iter = transform_expr(transformer, generator.iter);
            generator.ifs = transform_expressions(transformer, generator.ifs);
            generator
        })
        .collect();
    node.key = node.key.take().map(|expr| Box::new(transform_expr(transformer, *expr)));
    node.value = node.value.take().map(|expr| Box::new(transform_expr(transformer, *expr)));
    node
}

/// Transforms one owned pattern and all of its descendants.
pub fn transform_pattern<T: Transformer>(transformer: &mut T, pattern: Pattern) -> Pattern {
    let pattern = match pattern {
        Pattern::Value(mut node) => {
            node.value = transform_expr(transformer, node.value);
            Pattern::Value(node)
        }
        Pattern::Singleton(mut node) => {
            node.value = transform_expr(transformer, node.value);
            Pattern::Singleton(node)
        }
        Pattern::Sequence(mut node) => {
            node.patterns = node
                .patterns
                .into_iter()
                .map(|pattern| transform_pattern(transformer, pattern))
                .collect();
            Pattern::Sequence(node)
        }
        Pattern::Mapping(mut node) => {
            node.keys = transform_expressions(transformer, node.keys);
            node.patterns = node
                .patterns
                .into_iter()
                .map(|pattern| transform_pattern(transformer, pattern))
                .collect();
            Pattern::Mapping(node)
        }
        Pattern::Class(mut node) => {
            node.cls = transform_expr(transformer, node.cls);
            node.patterns = node
                .patterns
                .into_iter()
                .map(|pattern| transform_pattern(transformer, pattern))
                .collect();
            node.kwd_patterns = node
                .kwd_patterns
                .into_iter()
                .map(|pattern| transform_pattern(transformer, pattern))
                .collect();
            Pattern::Class(node)
        }
        Pattern::As(mut node) => {
            node.pattern =
                node.pattern.map(|pattern| Box::new(transform_pattern(transformer, *pattern)));
            Pattern::As(node)
        }
        Pattern::Or(mut node) => {
            node.patterns = node
                .patterns
                .into_iter()
                .map(|pattern| transform_pattern(transformer, pattern))
                .collect();
            Pattern::Or(node)
        }
        leaf => leaf,
    };
    transformer.transform_pattern(pattern)
}

fn transform_parameters<T: Transformer>(
    transformer: &mut T,
    mut parameters: Parameters,
) -> Parameters {
    parameters.posonlyargs = parameters
        .posonlyargs
        .into_iter()
        .map(|parameter| transform_parameter(transformer, parameter))
        .collect();
    parameters.args = parameters
        .args
        .into_iter()
        .map(|parameter| transform_parameter(transformer, parameter))
        .collect();
    parameters.vararg =
        parameters.vararg.map(|parameter| transform_parameter(transformer, parameter));
    parameters.kwonlyargs = parameters
        .kwonlyargs
        .into_iter()
        .map(|parameter| transform_parameter(transformer, parameter))
        .collect();
    parameters.kw_defaults = parameters
        .kw_defaults
        .into_iter()
        .map(|expr| expr.map(|expr| transform_expr(transformer, expr)))
        .collect();
    parameters.kwarg =
        parameters.kwarg.map(|parameter| transform_parameter(transformer, parameter));
    parameters.defaults = transform_expressions(transformer, parameters.defaults);
    parameters
}

fn transform_parameter<T: Transformer>(transformer: &mut T, mut parameter: Parameter) -> Parameter {
    parameter.annotation =
        parameter.annotation.map(|expr| Box::new(transform_expr(transformer, *expr)));
    parameter
}

fn transform_type_params<T: Transformer>(
    transformer: &mut T,
    params: Vec<TypeParam>,
) -> Vec<TypeParam> {
    params
        .into_iter()
        .map(|param| match param {
            TypeParam::TypeVar(mut data) => {
                data.bound = data.bound.map(|expr| transform_expr(transformer, expr));
                data.default = data.default.map(|expr| transform_expr(transformer, expr));
                TypeParam::TypeVar(data)
            }
            TypeParam::ParamSpec(mut data) => {
                data.bound = data.bound.map(|expr| transform_expr(transformer, expr));
                data.default = data.default.map(|expr| transform_expr(transformer, expr));
                TypeParam::ParamSpec(data)
            }
            TypeParam::TypeVarTuple(mut data) => {
                data.bound = data.bound.map(|expr| transform_expr(transformer, expr));
                data.default = data.default.map(|expr| transform_expr(transformer, expr));
                TypeParam::TypeVarTuple(data)
            }
        })
        .collect()
}

fn transform_keyword<T: Transformer>(transformer: &mut T, mut keyword: Keyword) -> Keyword {
    keyword.value = transform_expr(transformer, keyword.value);
    keyword
}

fn transform_with_item<T: Transformer>(transformer: &mut T, mut item: WithItem) -> WithItem {
    item.context_expr = transform_expr(transformer, item.context_expr);
    item.optional_vars = item.optional_vars.map(|expr| transform_expr(transformer, expr));
    item
}

fn transform_except_handler<T: Transformer>(
    transformer: &mut T,
    mut handler: ExceptHandler,
) -> ExceptHandler {
    handler.typ = handler.typ.map(|expr| transform_expr(transformer, expr));
    handler.body = transform_statements(transformer, handler.body);
    handler
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
