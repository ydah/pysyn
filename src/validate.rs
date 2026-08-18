//! Post-parse syntax and semantic validation.

#![allow(missing_docs)]

use std::collections::{HashMap, HashSet};

use crate::ast::{Expr, ExprContext, ModModule, Parameter, Parameters, Pattern, Stmt, TypeParam};
use crate::error::{Diagnostic, DiagnosticCode};
use crate::source::TextRange;
use crate::visit::{walk_expr, Visitor};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ValidateLevel {
    Syntax,
    Semantic,
}

pub fn validate(module: &ModModule, level: ValidateLevel) -> Vec<Diagnostic> {
    let mut validator = Validator::new(level);
    let bindings = collect_block_bindings(&module.body);
    validator.push_scope(ScopeKind::Module, bindings, Vec::new());
    validator.validate_block(&module.body);
    validator.pop_scope();
    validator.diagnostics
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ScopeKind {
    Module,
    Function,
    AsyncFunction,
    Lambda,
    Class,
}

impl ScopeKind {
    fn is_function(self) -> bool {
        matches!(self, Self::Function | Self::AsyncFunction | Self::Lambda)
    }

    fn is_async_function(self) -> bool {
        matches!(self, Self::AsyncFunction)
    }
}

struct ScopeState {
    kind: ScopeKind,
    all_bindings: HashSet<String>,
    seen_bindings: HashMap<String, TextRange>,
    globals: HashSet<String>,
    nonlocals: HashSet<String>,
    outer_loop_depth: usize,
}

struct Validator {
    level: ValidateLevel,
    diagnostics: Vec<Diagnostic>,
    scopes: Vec<ScopeState>,
    loop_depth: usize,
}

impl Validator {
    fn new(level: ValidateLevel) -> Self {
        Self { level, diagnostics: Vec::new(), scopes: Vec::new(), loop_depth: 0 }
    }

    fn push_scope(
        &mut self,
        kind: ScopeKind,
        all_bindings: HashSet<String>,
        initial_bindings: Vec<(String, TextRange)>,
    ) {
        let mut seen_bindings = HashMap::new();
        for (name, range) in initial_bindings {
            seen_bindings.entry(name).or_insert(range);
        }
        let outer_loop_depth = self.loop_depth;
        self.scopes.push(ScopeState {
            kind,
            all_bindings,
            seen_bindings,
            globals: HashSet::new(),
            nonlocals: HashSet::new(),
            outer_loop_depth,
        });
        self.loop_depth = 0;
    }

    fn pop_scope(&mut self) {
        if let Some(scope) = self.scopes.pop() {
            self.loop_depth = scope.outer_loop_depth;
        }
    }

    fn current_scope_kind(&self) -> ScopeKind {
        self.scopes.last().map_or(ScopeKind::Module, |scope| scope.kind)
    }

    fn record_binding(&mut self, name: &str, range: TextRange) {
        let Some(scope) = self.scopes.last_mut() else { return };
        if scope.globals.contains(name) || scope.nonlocals.contains(name) {
            return;
        }
        scope.seen_bindings.entry(name.to_owned()).or_insert(range);
    }

    fn declare_global(&mut self, name: &str, range: TextRange) {
        if self.level != ValidateLevel::Semantic {
            return;
        }
        let Some(scope) = self.scopes.last_mut() else { return };
        if scope.nonlocals.contains(name) {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Validation,
                range,
                format!("name '{name}' is nonlocal and global"),
            ));
        } else if scope.seen_bindings.contains_key(name) {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Validation,
                range,
                format!("name '{name}' is assigned to before global declaration"),
            ));
        }
        scope.globals.insert(name.to_owned());
    }

    fn declare_nonlocal(&mut self, name: &str, range: TextRange) {
        if self.level != ValidateLevel::Semantic {
            return;
        }
        let Some(scope) = self.scopes.last() else { return };
        let in_module = scope.kind == ScopeKind::Module;
        let has_global = scope.globals.contains(name);
        let assigned_before = scope.seen_bindings.contains_key(name);
        let has_enclosing_binding = self.has_enclosing_nonlocal_binding(name);
        if in_module {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Validation,
                range,
                "nonlocal declaration not allowed at module level",
            ));
        } else if has_global {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Validation,
                range,
                format!("name '{name}' is nonlocal and global"),
            ));
        } else if assigned_before {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Validation,
                range,
                format!("name '{name}' is assigned to before nonlocal declaration"),
            ));
        } else if !has_enclosing_binding {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Validation,
                range,
                format!("no binding for nonlocal '{name}' found"),
            ));
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.nonlocals.insert(name.to_owned());
        }
    }

    fn has_enclosing_nonlocal_binding(&self, name: &str) -> bool {
        self.scopes.iter().rev().skip(1).any(|scope| {
            (scope.kind.is_function() && scope.all_bindings.contains(name))
                || (scope.kind == ScopeKind::Class && name == "__class__")
        })
    }

    fn validate_block(&mut self, body: &[Stmt]) {
        for statement in body {
            self.validate_stmt(statement);
        }
    }

    fn validate_stmt(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Return(node) => {
                if self.level == ValidateLevel::Semantic && !self.current_scope_kind().is_function()
                {
                    self.error(node.range, "'return' outside function");
                }
                if let Some(value) = &node.value {
                    self.validate_expr(value);
                }
            }
            Stmt::For(node) | Stmt::AsyncFor(node) => {
                self.validate_expr(&node.iter);
                self.validate_expr(&node.target);
                self.record_target_bindings(&node.target);
                self.validate_loop(&node.body, &node.orelse);
            }
            Stmt::While(node) => {
                self.validate_expr(&node.test);
                self.validate_loop(&node.body, &node.orelse);
            }
            Stmt::FunctionDef(node) | Stmt::AsyncFunctionDef(node) => {
                self.record_binding(&node.name, node.range);
                for decorator in &node.decorator_list {
                    self.validate_expr(decorator);
                }
                self.validate_type_params(&node.type_params);
                self.validate_parameters(&node.args);
                if let Some(returns) = &node.returns {
                    self.validate_expr(returns);
                }

                let mut bindings = collect_block_bindings(&node.body);
                let parameter_bindings = parameter_bindings(&node.args);
                bindings.extend(parameter_bindings.iter().map(|(name, _)| name.clone()));
                let kind = if matches!(statement, Stmt::AsyncFunctionDef(_)) {
                    ScopeKind::AsyncFunction
                } else {
                    ScopeKind::Function
                };
                self.push_scope(kind, bindings, parameter_bindings);
                self.validate_block(&node.body);
                self.pop_scope();
            }
            Stmt::ClassDef(node) => {
                self.record_binding(&node.name, node.range);
                for decorator in &node.decorator_list {
                    self.validate_expr(decorator);
                }
                for base in &node.bases {
                    self.validate_expr(base);
                }
                for keyword in &node.keywords {
                    self.validate_expr(&keyword.value);
                }
                self.validate_type_params(&node.type_params);
                self.push_scope(ScopeKind::Class, collect_block_bindings(&node.body), Vec::new());
                self.validate_block(&node.body);
                self.pop_scope();
            }
            Stmt::Break(node) | Stmt::Continue(node)
                if self.level == ValidateLevel::Semantic && self.loop_depth == 0 =>
            {
                self.error(node.range, "loop control statement outside loop");
            }
            Stmt::If(node) => {
                self.validate_expr(&node.test);
                self.validate_block(&node.body);
                self.validate_block(&node.orelse);
            }
            Stmt::With(node) | Stmt::AsyncWith(node) => {
                for item in &node.items {
                    self.validate_expr(&item.context_expr);
                    if let Some(target) = &item.optional_vars {
                        self.validate_expr(target);
                        self.record_target_bindings(target);
                    }
                }
                self.validate_block(&node.body);
            }
            Stmt::Try(node) | Stmt::TryStar(node) => {
                self.validate_block(&node.body);
                for handler in &node.handlers {
                    if let Some(typ) = &handler.typ {
                        self.validate_expr(typ);
                    }
                    if let Some(name) = &handler.name {
                        self.record_binding(name, handler.range);
                    }
                    self.validate_block(&handler.body);
                }
                self.validate_block(&node.orelse);
                self.validate_block(&node.finalbody);
            }
            Stmt::Match(node) => {
                self.validate_expr(&node.subject);
                for case in &node.cases {
                    let bindings = self.validate_pattern(&case.pattern, false);
                    for (name, range) in bindings {
                        self.record_binding(&name, range);
                    }
                    if let Some(guard) = &case.guard {
                        self.validate_expr(guard);
                    }
                    self.validate_block(&case.body);
                }
            }
            Stmt::Raise(node) => {
                if let Some(value) = &node.exc {
                    self.validate_expr(value);
                }
                if let Some(value) = &node.cause {
                    self.validate_expr(value);
                }
            }
            Stmt::Assert(node) => {
                self.validate_expr(&node.test);
                if let Some(value) = &node.msg {
                    self.validate_expr(value);
                }
            }
            Stmt::Delete(node) => {
                for target in &node.targets {
                    self.validate_expr(target);
                    self.record_target_bindings(target);
                }
            }
            Stmt::Assign(node) => {
                for target in &node.targets {
                    self.validate_expr(target);
                    self.record_target_bindings(target);
                }
                self.validate_expr(&node.value);
            }
            Stmt::AnnAssign(node) => {
                self.validate_expr(&node.target);
                self.record_target_bindings(&node.target);
                self.validate_expr(&node.annotation);
                if let Some(value) = &node.value {
                    self.validate_expr(value);
                }
            }
            Stmt::AugAssign(node) => {
                self.validate_expr(&node.target);
                self.record_target_bindings(&node.target);
                self.validate_expr(&node.value);
            }
            Stmt::TypeAlias(node) => {
                self.validate_expr(&node.name);
                self.record_target_bindings(&node.name);
                self.validate_type_params(&node.type_params);
                self.validate_expr(&node.value);
            }
            Stmt::Expr(node) => self.validate_expr(&node.value),
            Stmt::Import(node) => {
                for alias in &node.names {
                    let name = alias
                        .asname
                        .as_deref()
                        .or_else(|| alias.name.split('.').next())
                        .unwrap_or_default();
                    if name != "*" {
                        self.record_binding(name, alias.range);
                    }
                }
            }
            Stmt::ImportFrom(node) => {
                for alias in &node.names {
                    let name = alias.asname.as_deref().unwrap_or(&alias.name);
                    if name != "*" {
                        self.record_binding(name, alias.range);
                    }
                }
            }
            Stmt::Global(node) => {
                for name in &node.names {
                    self.declare_global(name, node.range);
                }
            }
            Stmt::Nonlocal(node) => {
                for name in &node.names {
                    self.declare_nonlocal(name, node.range);
                }
            }
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Pass(_) | Stmt::Invalid(_) => {}
        }
    }

    fn validate_loop(&mut self, body: &[Stmt], orelse: &[Stmt]) {
        self.loop_depth += 1;
        self.validate_block(body);
        self.validate_block(orelse);
        self.loop_depth = self.loop_depth.saturating_sub(1);
    }

    fn validate_parameters(&mut self, parameters: &Parameters) {
        let positional: Vec<&Parameter> =
            parameters.posonlyargs.iter().chain(&parameters.args).collect();
        let default_start = positional.len().saturating_sub(parameters.defaults.len());
        let mut names = HashSet::new();
        for parameter in positional.iter().copied() {
            self.validate_parameter(parameter, None);
            self.check_parameter_name(parameter, &mut names);
        }

        let mut saw_default = false;
        for (index, parameter) in positional.iter().copied().enumerate() {
            let aggregate = parameters
                .defaults
                .get(index.saturating_sub(default_start))
                .filter(|_| index >= default_start);
            let has_default = parameter.default.is_some() || aggregate.is_some();
            if has_default {
                saw_default = true;
            } else if saw_default {
                self.error(parameter.range, "non-default argument follows default argument");
            }
            if parameter.default.is_none() {
                if let Some(default) = aggregate {
                    self.validate_expr(default);
                }
            }
        }
        if parameters.defaults.len() > positional.len() {
            self.error(
                positional.first().map_or_else(TextRange::default, |parameter| parameter.range),
                "too many positional defaults",
            );
        }

        for (index, parameter) in parameters.kwonlyargs.iter().enumerate() {
            let aggregate = parameters.kw_defaults.get(index).and_then(Option::as_ref);
            self.validate_parameter(parameter, aggregate);
            self.check_parameter_name(parameter, &mut names);
        }
        if let Some(parameter) = &parameters.vararg {
            self.validate_parameter(parameter, None);
            self.check_parameter_name(parameter, &mut names);
            if parameter.default.is_some() {
                self.error(parameter.range, "var-positional argument cannot have a default");
            }
        }
        if let Some(parameter) = &parameters.kwarg {
            self.validate_parameter(parameter, None);
            self.check_parameter_name(parameter, &mut names);
            if parameter.default.is_some() {
                self.error(parameter.range, "var-keyword argument cannot have a default");
            }
        }
    }

    fn validate_parameter(&mut self, parameter: &Parameter, fallback_default: Option<&Expr>) {
        if let Some(annotation) = &parameter.annotation {
            self.validate_expr(annotation);
        }
        if let Some(default) = &parameter.default {
            self.validate_expr(default);
        } else if let Some(default) = fallback_default {
            self.validate_expr(default);
        }
    }

    fn check_parameter_name(&mut self, parameter: &Parameter, names: &mut HashSet<String>) {
        if !names.insert(parameter.name.to_string()) {
            self.error(parameter.range, format!("duplicate argument '{}'", parameter.name));
        }
    }

    fn validate_type_params(&mut self, type_params: &[TypeParam]) {
        let mut names = HashSet::new();
        let mut saw_default = false;
        for type_param in type_params {
            let data = match type_param {
                TypeParam::TypeVar(data)
                | TypeParam::ParamSpec(data)
                | TypeParam::TypeVarTuple(data) => data,
            };
            if !names.insert(data.name.to_string()) {
                self.error(data.range, format!("duplicate type parameter '{}'", data.name));
            }
            if let Some(bound) = &data.bound {
                self.validate_expr(bound);
            }
            if let Some(default) = &data.default {
                saw_default = true;
                self.validate_expr(default);
            } else if saw_default {
                self.error(data.range, "non-default type parameter follows default");
            }
        }
    }

    fn validate_expr(&mut self, expr: &Expr) {
        struct ExprValidator<'validator> {
            validator: &'validator mut Validator,
            generator_depth: usize,
        }

        impl<'tree, 'validator> Visitor<'tree> for ExprValidator<'validator> {
            fn visit_expr(&mut self, expr: &'tree Expr) {
                match expr {
                    Expr::Name(node) if node.ctx != ExprContext::Load && node.id.is_empty() => {
                        self.validator.error(node.range, "empty identifier");
                        walk_expr(self, expr);
                    }
                    Expr::NamedExpr(node) => {
                        self.validator.record_target_bindings(&node.target);
                        walk_expr(self, expr);
                    }
                    Expr::Await(node) => {
                        if self.generator_depth == 0 {
                            self.validator.validate_await(node.range);
                        }
                        walk_expr(self, expr);
                    }
                    Expr::Yield(node) => {
                        self.validator.validate_yield(node.range, false);
                        walk_expr(self, expr);
                    }
                    Expr::YieldFrom(node) => {
                        self.validator.validate_yield(node.range, true);
                        walk_expr(self, expr);
                    }
                    Expr::Lambda(node) => {
                        self.validator.validate_parameters(&node.args);
                        let bindings = parameter_bindings(&node.args);
                        let all_bindings = bindings.iter().map(|(name, _)| name.clone()).collect();
                        self.validator.push_scope(ScopeKind::Lambda, all_bindings, bindings);
                        self.validator.validate_expr(&node.body);
                        self.validator.pop_scope();
                    }
                    Expr::GeneratorExp(_) => {
                        self.generator_depth += 1;
                        walk_expr(self, expr);
                        self.generator_depth = self.generator_depth.saturating_sub(1);
                    }
                    _ => walk_expr(self, expr),
                }
            }
        }

        let mut validator = ExprValidator { validator: self, generator_depth: 0 };
        validator.visit_expr(expr);
    }

    fn validate_await(&mut self, range: TextRange) {
        if self.level == ValidateLevel::Semantic && !self.current_scope_kind().is_async_function() {
            self.error(range, "'await' outside async function");
        }
    }

    fn validate_yield(&mut self, range: TextRange, from: bool) {
        if self.level != ValidateLevel::Semantic {
            return;
        }
        let kind = self.current_scope_kind();
        if !kind.is_function() {
            self.error(range, "'yield' outside function");
        } else if from && kind.is_async_function() {
            self.error(range, "'yield from' inside async function");
        }
    }

    fn validate_pattern(&mut self, pattern: &Pattern, star_allowed: bool) -> PatternBindings {
        match pattern {
            Pattern::Value(node) => {
                self.validate_expr(&node.value);
                PatternBindings::new()
            }
            Pattern::Singleton(node) => {
                self.validate_expr(&node.value);
                PatternBindings::new()
            }
            Pattern::Sequence(node) => {
                let mut bindings = PatternBindings::new();
                let mut stars = 0;
                for child in &node.patterns {
                    if matches!(child, Pattern::Star(_)) {
                        stars += 1;
                    }
                    let child_bindings = self.validate_pattern(child, true);
                    self.merge_pattern_bindings(&mut bindings, child_bindings);
                }
                if stars > 1 {
                    self.error(node.range, "multiple starred patterns in sequence pattern");
                }
                bindings
            }
            Pattern::Mapping(node) => {
                let mut bindings = PatternBindings::new();
                for key in &node.keys {
                    self.validate_expr(key);
                }
                for child in &node.patterns {
                    let child_bindings = self.validate_pattern(child, false);
                    self.merge_pattern_bindings(&mut bindings, child_bindings);
                }
                if let Some(name) = &node.rest {
                    self.add_pattern_binding(&mut bindings, name, node.range);
                }
                bindings
            }
            Pattern::Class(node) => {
                let mut bindings = PatternBindings::new();
                self.validate_expr(&node.cls);
                for child in &node.patterns {
                    let child_bindings = self.validate_pattern(child, false);
                    self.merge_pattern_bindings(&mut bindings, child_bindings);
                }
                let mut attributes = HashSet::new();
                for attribute in &node.kwd_attrs {
                    if !attributes.insert(attribute.to_string()) {
                        self.error(node.range, format!("duplicate keyword pattern '{attribute}'"));
                    }
                }
                for child in &node.kwd_patterns {
                    let child_bindings = self.validate_pattern(child, false);
                    self.merge_pattern_bindings(&mut bindings, child_bindings);
                }
                bindings
            }
            Pattern::Star(node) => {
                if !star_allowed {
                    self.error(node.range, "starred pattern must be inside a sequence pattern");
                }
                let mut bindings = PatternBindings::new();
                if let Some(name) = &node.name {
                    self.add_pattern_binding(&mut bindings, name, node.range);
                }
                bindings
            }
            Pattern::As(node) => {
                let mut bindings = PatternBindings::new();
                if let Some(child) = &node.pattern {
                    let child_bindings = self.validate_pattern(child, false);
                    self.merge_pattern_bindings(&mut bindings, child_bindings);
                }
                if let Some(name) = &node.name {
                    if name.is_empty() || (name.as_ref() == "_" && node.pattern.is_some()) {
                        self.error(node.range, "invalid capture target in pattern");
                    }
                    self.add_pattern_binding(&mut bindings, name, node.range);
                } else if node.pattern.is_some() {
                    self.error(node.range, "capture pattern requires a name");
                }
                bindings
            }
            Pattern::Or(node) => {
                let mut alternatives = Vec::with_capacity(node.patterns.len());
                for child in &node.patterns {
                    alternatives.push(self.validate_pattern(child, false));
                }
                if let Some(first) = alternatives.first() {
                    let expected: HashSet<&str> = first.keys().map(String::as_str).collect();
                    if alternatives.iter().skip(1).any(|bindings| {
                        bindings.keys().map(String::as_str).collect::<HashSet<_>>() != expected
                    }) {
                        self.error(node.range, "alternative patterns bind different names");
                    }
                }
                alternatives.into_iter().next().unwrap_or_default()
            }
            Pattern::Invalid(_) => PatternBindings::new(),
        }
    }

    fn add_pattern_binding(
        &mut self,
        bindings: &mut PatternBindings,
        name: &str,
        range: TextRange,
    ) {
        if name == "_" || name.is_empty() {
            return;
        }
        if let Some(previous) = bindings.insert(name.to_owned(), range) {
            self.diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::Validation,
                    range,
                    format!("name capture '{name}' is repeated"),
                )
                .with_label(previous, "name captured here"),
            );
        }
    }

    fn merge_pattern_bindings(
        &mut self,
        bindings: &mut PatternBindings,
        child_bindings: PatternBindings,
    ) {
        for (name, range) in child_bindings {
            self.add_pattern_binding(bindings, &name, range);
        }
    }

    fn record_target_bindings(&mut self, expr: &Expr) {
        let mut names = Vec::new();
        collect_target_names(expr, &mut names);
        for (name, range) in names {
            self.record_binding(&name, range);
        }
    }

    fn error(&mut self, range: TextRange, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::error(DiagnosticCode::Validation, range, message));
    }
}

type PatternBindings = HashMap<String, TextRange>;

fn parameter_bindings(parameters: &Parameters) -> Vec<(String, TextRange)> {
    let mut bindings = Vec::new();
    for parameter in parameters
        .posonlyargs
        .iter()
        .chain(&parameters.args)
        .chain(parameters.vararg.iter())
        .chain(&parameters.kwonlyargs)
        .chain(parameters.kwarg.iter())
    {
        bindings.push((parameter.name.to_string(), parameter.range));
    }
    bindings
}

fn collect_block_bindings(body: &[Stmt]) -> HashSet<String> {
    let mut bindings = HashSet::new();
    for statement in body {
        collect_statement_bindings(statement, &mut bindings);
    }
    bindings
}

fn collect_statement_bindings(statement: &Stmt, bindings: &mut HashSet<String>) {
    match statement {
        Stmt::FunctionDef(node) | Stmt::AsyncFunctionDef(node) => {
            bindings.insert(node.name.to_string());
            for decorator in &node.decorator_list {
                collect_expr_bindings(decorator, bindings);
            }
            collect_parameters_bindings(&node.args, bindings);
            if let Some(returns) = &node.returns {
                collect_expr_bindings(returns, bindings);
            }
        }
        Stmt::ClassDef(node) => {
            bindings.insert(node.name.to_string());
            for decorator in &node.decorator_list {
                collect_expr_bindings(decorator, bindings);
            }
            for base in &node.bases {
                collect_expr_bindings(base, bindings);
            }
            for keyword in &node.keywords {
                collect_expr_bindings(&keyword.value, bindings);
            }
        }
        Stmt::For(node) | Stmt::AsyncFor(node) => {
            collect_target_bindings(&node.target, bindings);
            collect_expr_bindings(&node.iter, bindings);
            collect_block_bindings_into(&node.body, bindings);
            collect_block_bindings_into(&node.orelse, bindings);
        }
        Stmt::While(node) => {
            collect_expr_bindings(&node.test, bindings);
            collect_block_bindings_into(&node.body, bindings);
            collect_block_bindings_into(&node.orelse, bindings);
        }
        Stmt::If(node) => {
            collect_expr_bindings(&node.test, bindings);
            collect_block_bindings_into(&node.body, bindings);
            collect_block_bindings_into(&node.orelse, bindings);
        }
        Stmt::With(node) | Stmt::AsyncWith(node) => {
            for item in &node.items {
                collect_expr_bindings(&item.context_expr, bindings);
                if let Some(target) = &item.optional_vars {
                    collect_target_bindings(target, bindings);
                }
            }
            collect_block_bindings_into(&node.body, bindings);
        }
        Stmt::Try(node) | Stmt::TryStar(node) => {
            collect_block_bindings_into(&node.body, bindings);
            for handler in &node.handlers {
                if let Some(typ) = &handler.typ {
                    collect_expr_bindings(typ, bindings);
                }
                if let Some(name) = &handler.name {
                    bindings.insert(name.to_string());
                }
                collect_block_bindings_into(&handler.body, bindings);
            }
            collect_block_bindings_into(&node.orelse, bindings);
            collect_block_bindings_into(&node.finalbody, bindings);
        }
        Stmt::Match(node) => {
            collect_expr_bindings(&node.subject, bindings);
            for case in &node.cases {
                collect_pattern_names(&case.pattern, bindings);
                if let Some(guard) = &case.guard {
                    collect_expr_bindings(guard, bindings);
                }
                collect_block_bindings_into(&case.body, bindings);
            }
        }
        Stmt::Return(node) => {
            if let Some(value) = &node.value {
                collect_expr_bindings(value, bindings);
            }
        }
        Stmt::Delete(node) => {
            for target in &node.targets {
                collect_target_bindings(target, bindings);
                collect_expr_bindings(target, bindings);
            }
        }
        Stmt::Assign(node) => {
            for target in &node.targets {
                collect_target_bindings(target, bindings);
            }
            collect_expr_bindings(&node.value, bindings);
        }
        Stmt::AnnAssign(node) => {
            collect_target_bindings(&node.target, bindings);
            collect_expr_bindings(&node.annotation, bindings);
            if let Some(value) = &node.value {
                collect_expr_bindings(value, bindings);
            }
        }
        Stmt::AugAssign(node) => {
            collect_target_bindings(&node.target, bindings);
            collect_expr_bindings(&node.target, bindings);
            collect_expr_bindings(&node.value, bindings);
        }
        Stmt::TypeAlias(node) => {
            collect_target_bindings(&node.name, bindings);
            collect_expr_bindings(&node.value, bindings);
        }
        Stmt::Expr(node) => collect_expr_bindings(&node.value, bindings),
        Stmt::Raise(node) => {
            if let Some(value) = &node.exc {
                collect_expr_bindings(value, bindings);
            }
            if let Some(value) = &node.cause {
                collect_expr_bindings(value, bindings);
            }
        }
        Stmt::Assert(node) => {
            collect_expr_bindings(&node.test, bindings);
            if let Some(value) = &node.msg {
                collect_expr_bindings(value, bindings);
            }
        }
        Stmt::Import(node) => {
            for alias in &node.names {
                let name = alias
                    .asname
                    .as_deref()
                    .or_else(|| alias.name.split('.').next())
                    .unwrap_or_default();
                if name != "*" {
                    bindings.insert(name.to_owned());
                }
            }
        }
        Stmt::ImportFrom(node) => {
            for alias in &node.names {
                let name = alias.asname.as_deref().unwrap_or(&alias.name);
                if name != "*" {
                    bindings.insert(name.to_string());
                }
            }
        }
        Stmt::Global(_)
        | Stmt::Nonlocal(_)
        | Stmt::Pass(_)
        | Stmt::Break(_)
        | Stmt::Continue(_)
        | Stmt::Invalid(_) => {}
    }
}

fn collect_block_bindings_into(body: &[Stmt], bindings: &mut HashSet<String>) {
    for statement in body {
        collect_statement_bindings(statement, bindings);
    }
}

fn collect_parameters_bindings(parameters: &Parameters, bindings: &mut HashSet<String>) {
    for parameter in parameters
        .posonlyargs
        .iter()
        .chain(&parameters.args)
        .chain(parameters.vararg.iter())
        .chain(&parameters.kwonlyargs)
        .chain(parameters.kwarg.iter())
    {
        collect_optional_expr_bindings(parameter.annotation.as_deref(), bindings);
        collect_optional_expr_bindings(parameter.default.as_deref(), bindings);
    }
    for default in &parameters.defaults {
        collect_expr_bindings(default, bindings);
    }
    for default in &parameters.kw_defaults {
        collect_optional_expr_bindings(default.as_ref(), bindings);
    }
}

fn collect_optional_expr_bindings(expr: Option<&Expr>, bindings: &mut HashSet<String>) {
    if let Some(expr) = expr {
        collect_expr_bindings(expr, bindings);
    }
}

fn collect_target_names(expr: &Expr, names: &mut Vec<(String, TextRange)>) {
    match expr {
        Expr::Name(node) => names.push((node.id.to_string(), node.range)),
        Expr::Starred(node) => collect_target_names(&node.value, names),
        Expr::List(node) | Expr::Tuple(node) => {
            for element in &node.elts {
                collect_target_names(element, names);
            }
        }
        _ => {}
    }
}

fn collect_target_bindings(expr: &Expr, bindings: &mut HashSet<String>) {
    let mut names = Vec::new();
    collect_target_names(expr, &mut names);
    for (name, _) in names {
        bindings.insert(name);
    }
}

fn collect_expr_bindings(expr: &Expr, bindings: &mut HashSet<String>) {
    struct BindingCollector<'bindings> {
        bindings: &'bindings mut HashSet<String>,
    }

    impl<'tree, 'bindings> Visitor<'tree> for BindingCollector<'bindings> {
        fn visit_expr(&mut self, expr: &'tree Expr) {
            if let Expr::Lambda(_) = expr {
                return;
            }
            if let Expr::NamedExpr(node) = expr {
                let mut targets = Vec::new();
                collect_target_names(&node.target, &mut targets);
                for (name, _) in targets {
                    self.bindings.insert(name);
                }
            }
            walk_expr(self, expr);
        }
    }

    let mut collector = BindingCollector { bindings };
    collector.visit_expr(expr);
}

fn collect_pattern_names(pattern: &Pattern, bindings: &mut HashSet<String>) {
    match pattern {
        Pattern::Star(node) => {
            if let Some(name) = &node.name {
                if name.as_ref() != "_" {
                    bindings.insert(name.to_string());
                }
            }
        }
        Pattern::As(node) => {
            if let Some(name) = &node.name {
                if name.as_ref() != "_" {
                    bindings.insert(name.to_string());
                }
            }
            if let Some(pattern) = &node.pattern {
                collect_pattern_names(pattern, bindings);
            }
        }
        Pattern::Sequence(node) => {
            for pattern in &node.patterns {
                collect_pattern_names(pattern, bindings);
            }
        }
        Pattern::Mapping(node) => {
            if let Some(name) = &node.rest {
                if name.as_ref() != "_" {
                    bindings.insert(name.to_string());
                }
            }
            for pattern in &node.patterns {
                collect_pattern_names(pattern, bindings);
            }
        }
        Pattern::Class(node) => {
            for pattern in &node.patterns {
                collect_pattern_names(pattern, bindings);
            }
            for pattern in &node.kwd_patterns {
                collect_pattern_names(pattern, bindings);
            }
        }
        Pattern::Or(node) => {
            if let Some(pattern) = node.patterns.first() {
                collect_pattern_names(pattern, bindings);
            }
        }
        Pattern::Value(_) | Pattern::Singleton(_) | Pattern::Invalid(_) => {}
    }
}
