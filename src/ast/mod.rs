//! CPython-shaped abstract syntax tree types.

//! The public enums and records mirror Python's `ast` module. Every node
//! carries a byte range, while parser-owned source text remains private.
use crate::source::{TextRange, TextSize};
use crate::token::StringPrefix;
use std::borrow::Cow;
use std::fmt;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// Public API item.
pub enum ExprContext {
    /// AST variant.
    Load,
    /// AST variant.
    Store,
    /// AST variant.
    Del,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// Public API item.
pub enum BoolOperator {
    /// AST variant.
    And,
    /// AST variant.
    Or,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// Public API item.
pub enum UnaryOperator {
    /// AST variant.
    Invert,
    /// AST variant.
    Not,
    /// AST variant.
    UAdd,
    /// AST variant.
    USub,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// Public API item.
pub enum BinaryOperator {
    /// AST variant.
    Add,
    /// AST variant.
    Sub,
    /// AST variant.
    Mult,
    /// AST variant.
    MatMult,
    /// AST variant.
    Div,
    /// AST variant.
    FloorDiv,
    /// AST variant.
    Mod,
    /// AST variant.
    Pow,
    /// AST variant.
    LShift,
    /// AST variant.
    RShift,
    /// AST variant.
    BitOr,
    /// AST variant.
    BitXor,
    /// AST variant.
    BitAnd,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// Public API item.
pub enum CompareOperator {
    /// AST variant.
    Eq,
    /// AST variant.
    NotEq,
    /// AST variant.
    Lt,
    /// AST variant.
    LtE,
    /// AST variant.
    Gt,
    /// AST variant.
    GtE,
    /// AST variant.
    In,
    /// AST variant.
    NotIn,
    /// AST variant.
    Is,
    /// AST variant.
    IsNot,
}

#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub enum Number {
    /// AST variant.
    Int(Int),
    /// AST variant.
    Float(f64),
    /// AST variant.
    Complex {
        /// Real component.
        real: f64,
        /// Imaginary component.
        imag: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Public API item.
pub struct Int(String);

impl Int {
    /// Performs this public operation.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    /// Performs this public operation.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Int {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Public API item.
pub struct StringLiteralValue {
    /// Value stored by this public node.
    pub parts: Vec<StringLiteralPart>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Public API item.
pub struct StringLiteralPart {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub flags: StringFlags,
    /// Value stored by this public node.
    pub value: Box<str>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// Public API item.
pub struct StringFlags {
    /// Value stored by this public node.
    pub prefix: StringPrefix,
    /// Value stored by this public node.
    pub triple: bool,
    /// Value stored by this public node.
    pub quote: char,
}

impl StringLiteralValue {
    /// Performs this public operation.
    pub fn new(parts: Vec<StringLiteralPart>) -> Self {
        Self { parts }
    }
    /// Performs this public operation.
    pub fn to_str(&self) -> Cow<'_, str> {
        if self.parts.len() == 1 {
            return Cow::Borrowed(&self.parts[0].value);
        }
        Cow::Owned(self.parts.iter().map(|part| part.value.as_ref()).collect())
    }
}

#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ModModule {
    /// Value stored by this public node.
    pub body: Vec<Stmt>,
    /// Value stored by this public node.
    pub type_ignores: Vec<TypeIgnore>,
    /// Value stored by this public node.
    pub range: TextRange,
    pub(crate) source: Option<Box<str>>,
}

#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct TypeIgnore {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub lineno: u32,
    /// Value stored by this public node.
    pub tag: Box<str>,
}

#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub enum Stmt {
    /// AST variant.
    FunctionDef(Box<StmtFunctionDef>),
    /// AST variant.
    AsyncFunctionDef(Box<StmtFunctionDef>),
    /// AST variant.
    ClassDef(Box<StmtClassDef>),
    /// AST variant.
    Return(StmtReturn),
    /// AST variant.
    Delete(StmtDelete),
    /// AST variant.
    Assign(StmtAssign),
    /// AST variant.
    TypeAlias(StmtTypeAlias),
    /// AST variant.
    AugAssign(StmtAugAssign),
    /// AST variant.
    AnnAssign(StmtAnnAssign),
    /// AST variant.
    For(StmtFor),
    /// AST variant.
    AsyncFor(StmtFor),
    /// AST variant.
    While(StmtWhile),
    /// AST variant.
    If(StmtIf),
    /// AST variant.
    With(StmtWith),
    /// AST variant.
    AsyncWith(StmtWith),
    /// AST variant.
    Match(StmtMatch),
    /// AST variant.
    Raise(StmtRaise),
    /// AST variant.
    Try(Box<StmtTry>),
    /// AST variant.
    TryStar(Box<StmtTry>),
    /// AST variant.
    Assert(StmtAssert),
    /// AST variant.
    Import(StmtImport),
    /// AST variant.
    ImportFrom(StmtImportFrom),
    /// AST variant.
    Global(StmtNames),
    /// AST variant.
    Nonlocal(StmtNames),
    /// AST variant.
    Expr(StmtExpr),
    /// AST variant.
    Pass(StmtSimple),
    /// AST variant.
    Break(StmtSimple),
    /// AST variant.
    Continue(StmtSimple),
    /// AST variant.
    Invalid(StmtInvalid),
}

#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtFunctionDef {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub name: Box<str>,
    /// Value stored by this public node.
    pub decorator_list: Vec<Expr>,
    /// Value stored by this public node.
    pub type_params: Vec<TypeParam>,
    /// Value stored by this public node.
    pub args: Parameters,
    /// Value stored by this public node.
    pub returns: Option<Box<Expr>>,
    /// Value stored by this public node.
    pub body: Vec<Stmt>,
    /// Value stored by this public node.
    pub type_comment: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtClassDef {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub name: Box<str>,
    /// Value stored by this public node.
    pub bases: Vec<Expr>,
    /// Value stored by this public node.
    pub keywords: Vec<Keyword>,
    /// Value stored by this public node.
    pub decorator_list: Vec<Expr>,
    /// Value stored by this public node.
    pub type_params: Vec<TypeParam>,
    /// Value stored by this public node.
    pub body: Vec<Stmt>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtReturn {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub value: Option<Box<Expr>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtDelete {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub targets: Vec<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtAssign {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub targets: Vec<Expr>,
    /// Value stored by this public node.
    pub value: Box<Expr>,
    /// Value stored by this public node.
    pub type_comment: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtTypeAlias {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub name: Box<Expr>,
    /// Value stored by this public node.
    pub type_params: Vec<TypeParam>,
    /// Value stored by this public node.
    pub value: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtAugAssign {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub target: Box<Expr>,
    /// Value stored by this public node.
    pub op: BinaryOperator,
    /// Value stored by this public node.
    pub value: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtAnnAssign {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub target: Box<Expr>,
    /// Value stored by this public node.
    pub annotation: Box<Expr>,
    /// Value stored by this public node.
    pub value: Option<Box<Expr>>,
    /// Value stored by this public node.
    pub simple: bool,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtFor {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub target: Box<Expr>,
    /// Value stored by this public node.
    pub iter: Box<Expr>,
    /// Value stored by this public node.
    pub body: Vec<Stmt>,
    /// Value stored by this public node.
    pub orelse: Vec<Stmt>,
    /// Value stored by this public node.
    pub type_comment: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtWhile {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub test: Box<Expr>,
    /// Value stored by this public node.
    pub body: Vec<Stmt>,
    /// Value stored by this public node.
    pub orelse: Vec<Stmt>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtIf {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub test: Box<Expr>,
    /// Value stored by this public node.
    pub body: Vec<Stmt>,
    /// Value stored by this public node.
    pub orelse: Vec<Stmt>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtWith {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub items: Vec<WithItem>,
    /// Value stored by this public node.
    pub body: Vec<Stmt>,
    /// Value stored by this public node.
    pub type_comment: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtMatch {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub subject: Box<Expr>,
    /// Value stored by this public node.
    pub cases: Vec<MatchCase>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtRaise {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub exc: Option<Box<Expr>>,
    /// Value stored by this public node.
    pub cause: Option<Box<Expr>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtTry {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub body: Vec<Stmt>,
    /// Value stored by this public node.
    pub handlers: Vec<ExceptHandler>,
    /// Value stored by this public node.
    pub orelse: Vec<Stmt>,
    /// Value stored by this public node.
    pub finalbody: Vec<Stmt>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtAssert {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub test: Box<Expr>,
    /// Value stored by this public node.
    pub msg: Option<Box<Expr>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtImport {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub names: Vec<Alias>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtImportFrom {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub module: Option<Box<str>>,
    /// Value stored by this public node.
    pub names: Vec<Alias>,
    /// Value stored by this public node.
    pub level: u32,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtNames {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub names: Vec<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtExpr {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub value: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtSimple {
    /// Value stored by this public node.
    pub range: TextRange,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct StmtInvalid {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub message: Box<str>,
}

#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub enum Expr {
    /// AST variant.
    BoolOp(ExprBoolOp),
    /// AST variant.
    NamedExpr(ExprNamedExpr),
    /// AST variant.
    BinOp(ExprBinOp),
    /// AST variant.
    UnaryOp(ExprUnaryOp),
    /// AST variant.
    Lambda(Box<ExprLambda>),
    /// AST variant.
    IfExp(ExprIfExp),
    /// AST variant.
    Dict(ExprDict),
    /// AST variant.
    Set(ExprSet),
    /// AST variant.
    ListComp(ExprComprehension),
    /// AST variant.
    SetComp(ExprComprehension),
    /// AST variant.
    DictComp(ExprComprehension),
    /// AST variant.
    GeneratorExp(ExprComprehension),
    /// AST variant.
    Await(ExprUnaryValue),
    /// AST variant.
    Yield(ExprUnaryValue),
    /// AST variant.
    YieldFrom(ExprUnaryValue),
    /// AST variant.
    Compare(Box<ExprCompare>),
    /// AST variant.
    Call(Box<ExprCall>),
    /// AST variant.
    FString(ExprFString),
    /// AST variant.
    FormattedValue(ExprFormattedValue),
    /// AST variant.
    StringLiteral(ExprString),
    /// AST variant.
    BytesLiteral(ExprString),
    /// AST variant.
    NumberLiteral(ExprNumber),
    /// AST variant.
    BooleanLiteral(ExprBoolean),
    /// AST variant.
    NoneLiteral(ExprLiteral),
    /// AST variant.
    EllipsisLiteral(ExprLiteral),
    /// AST variant.
    Attribute(ExprAttribute),
    /// AST variant.
    Subscript(ExprSubscript),
    /// AST variant.
    Starred(ExprStarred),
    /// AST variant.
    Name(ExprName),
    /// AST variant.
    List(ExprSequence),
    /// AST variant.
    Tuple(ExprSequence),
    /// AST variant.
    Slice(ExprSlice),
    /// AST variant.
    Invalid(ExprInvalid),
}

#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprBoolOp {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub op: BoolOperator,
    /// Value stored by this public node.
    pub values: Vec<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprNamedExpr {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub target: Box<Expr>,
    /// Value stored by this public node.
    pub value: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprBinOp {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub left: Box<Expr>,
    /// Value stored by this public node.
    pub op: BinaryOperator,
    /// Value stored by this public node.
    pub right: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprUnaryOp {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub op: UnaryOperator,
    /// Value stored by this public node.
    pub operand: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprLambda {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub args: Parameters,
    /// Value stored by this public node.
    pub body: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprIfExp {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub body: Box<Expr>,
    /// Value stored by this public node.
    pub test: Box<Expr>,
    /// Value stored by this public node.
    pub orelse: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprDict {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub keys: Vec<Option<Expr>>,
    /// Value stored by this public node.
    pub values: Vec<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprSet {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub elts: Vec<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprComprehension {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub elt: Box<Expr>,
    /// Value stored by this public node.
    pub generators: Vec<Comprehension>,
    /// Value stored by this public node.
    pub key: Option<Box<Expr>>,
    /// Value stored by this public node.
    pub value: Option<Box<Expr>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprUnaryValue {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub value: Option<Box<Expr>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprCompare {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub left: Box<Expr>,
    /// Value stored by this public node.
    pub ops: Vec<CompareOperator>,
    /// Value stored by this public node.
    pub comparators: Vec<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprCall {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub func: Box<Expr>,
    /// Value stored by this public node.
    pub args: Vec<Expr>,
    /// Value stored by this public node.
    pub keywords: Vec<Keyword>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprFString {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub values: Vec<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprFormattedValue {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub value: Box<Expr>,
    /// Value stored by this public node.
    pub conversion: Option<char>,
    /// Value stored by this public node.
    pub format_spec: Option<Box<Expr>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprString {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub value: StringLiteralValue,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprNumber {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub value: Number,
    /// Value stored by this public node.
    pub raw: Box<str>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprBoolean {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub value: bool,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprLiteral {
    /// Value stored by this public node.
    pub range: TextRange,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprAttribute {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub value: Box<Expr>,
    /// Value stored by this public node.
    pub attr: Box<str>,
    /// Value stored by this public node.
    pub ctx: ExprContext,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprSubscript {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub value: Box<Expr>,
    /// Value stored by this public node.
    pub slice: Box<Expr>,
    /// Value stored by this public node.
    pub ctx: ExprContext,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprStarred {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub value: Box<Expr>,
    /// Value stored by this public node.
    pub ctx: ExprContext,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprName {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub id: Box<str>,
    /// Value stored by this public node.
    pub ctx: ExprContext,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprSequence {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub elts: Vec<Expr>,
    /// Value stored by this public node.
    pub ctx: ExprContext,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprSlice {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub lower: Option<Box<Expr>>,
    /// Value stored by this public node.
    pub upper: Option<Box<Expr>>,
    /// Value stored by this public node.
    pub step: Option<Box<Expr>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExprInvalid {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub message: Box<str>,
}

#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct Parameters {
    /// Value stored by this public node.
    pub posonlyargs: Vec<Parameter>,
    /// Value stored by this public node.
    pub args: Vec<Parameter>,
    /// Value stored by this public node.
    pub vararg: Option<Parameter>,
    /// Value stored by this public node.
    pub kwonlyargs: Vec<Parameter>,
    /// Value stored by this public node.
    pub kw_defaults: Vec<Option<Expr>>,
    /// Value stored by this public node.
    pub kwarg: Option<Parameter>,
    /// Value stored by this public node.
    pub defaults: Vec<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct Parameter {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub name: Box<str>,
    /// Value stored by this public node.
    pub annotation: Option<Box<Expr>>,
    /// Value stored by this public node.
    pub type_comment: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct Keyword {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub arg: Option<Box<str>>,
    /// Value stored by this public node.
    pub value: Expr,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct Alias {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub name: Box<str>,
    /// Value stored by this public node.
    pub asname: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct WithItem {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub context_expr: Expr,
    /// Value stored by this public node.
    pub optional_vars: Option<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct ExceptHandler {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub typ: Option<Expr>,
    /// Value stored by this public node.
    pub name: Option<Box<str>>,
    /// Value stored by this public node.
    pub body: Vec<Stmt>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct Comprehension {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub target: Expr,
    /// Value stored by this public node.
    pub iter: Expr,
    /// Value stored by this public node.
    pub ifs: Vec<Expr>,
    /// Value stored by this public node.
    pub is_async: bool,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct MatchCase {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub pattern: Pattern,
    /// Value stored by this public node.
    pub guard: Option<Expr>,
    /// Value stored by this public node.
    pub body: Vec<Stmt>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub enum Pattern {
    /// AST variant.
    Value(PatternValue),
    /// AST variant.
    Singleton(PatternSingleton),
    /// AST variant.
    Sequence(PatternSequence),
    /// AST variant.
    Mapping(PatternMapping),
    /// AST variant.
    Class(PatternClass),
    /// AST variant.
    Star(PatternStar),
    /// AST variant.
    As(PatternAs),
    /// AST variant.
    Or(PatternOr),
    /// AST variant.
    Invalid(PatternInvalid),
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct PatternValue {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub value: Expr,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct PatternSingleton {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub value: Expr,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct PatternSequence {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub patterns: Vec<Pattern>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct PatternMapping {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub keys: Vec<Expr>,
    /// Value stored by this public node.
    pub patterns: Vec<Pattern>,
    /// Value stored by this public node.
    pub rest: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct PatternClass {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub cls: Expr,
    /// Value stored by this public node.
    pub patterns: Vec<Pattern>,
    /// Value stored by this public node.
    pub kwd_attrs: Vec<Box<str>>,
    /// Value stored by this public node.
    pub kwd_patterns: Vec<Pattern>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct PatternStar {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub name: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct PatternAs {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub pattern: Option<Box<Pattern>>,
    /// Value stored by this public node.
    pub name: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct PatternOr {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub patterns: Vec<Pattern>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct PatternInvalid {
    /// Value stored by this public node.
    pub range: TextRange,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub enum TypeParam {
    /// AST variant.
    TypeVar(TypeParamData),
    /// AST variant.
    ParamSpec(TypeParamData),
    /// AST variant.
    TypeVarTuple(TypeParamData),
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct TypeParamData {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub name: Box<str>,
    /// Value stored by this public node.
    pub bound: Option<Expr>,
    /// Value stored by this public node.
    pub default: Option<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Public API item.
pub struct Comment {
    /// Value stored by this public node.
    pub range: TextRange,
    /// Value stored by this public node.
    pub text: Box<str>,
}

/// Returns the source range of an AST node.
pub trait Ranged {
    /// Visits or transforms the node.
    fn range(&self) -> TextRange;
}

macro_rules! ranged_structs { ($($ty:ty),+ $(,)?) => { $(impl Ranged for $ty { fn range(&self) -> TextRange { self.range } })+ }; }
ranged_structs!(
    ModModule,
    TypeIgnore,
    StmtFunctionDef,
    StmtClassDef,
    StmtReturn,
    StmtDelete,
    StmtAssign,
    StmtTypeAlias,
    StmtAugAssign,
    StmtAnnAssign,
    StmtFor,
    StmtWhile,
    StmtIf,
    StmtWith,
    StmtMatch,
    StmtRaise,
    StmtTry,
    StmtAssert,
    StmtImport,
    StmtImportFrom,
    StmtNames,
    StmtExpr,
    StmtSimple,
    StmtInvalid,
    ExprBoolOp,
    ExprNamedExpr,
    ExprBinOp,
    ExprUnaryOp,
    ExprLambda,
    ExprIfExp,
    ExprDict,
    ExprSet,
    ExprComprehension,
    ExprUnaryValue,
    ExprCompare,
    ExprCall,
    ExprFString,
    ExprFormattedValue,
    ExprString,
    ExprNumber,
    ExprBoolean,
    ExprLiteral,
    ExprAttribute,
    ExprSubscript,
    ExprStarred,
    ExprName,
    ExprSequence,
    ExprSlice,
    ExprInvalid,
    Parameter,
    Keyword,
    Alias,
    WithItem,
    ExceptHandler,
    Comprehension,
    MatchCase,
    PatternValue,
    PatternSingleton,
    PatternSequence,
    PatternMapping,
    PatternClass,
    PatternStar,
    PatternAs,
    PatternOr,
    PatternInvalid,
    TypeParamData,
    Comment
);

impl Ranged for Stmt {
    fn range(&self) -> TextRange {
        match self {
            Self::FunctionDef(n) | Self::AsyncFunctionDef(n) => n.range,
            Self::ClassDef(n) => n.range,
            Self::Return(n) => n.range,
            Self::Delete(n) => n.range,
            Self::Assign(n) => n.range,
            Self::TypeAlias(n) => n.range,
            Self::AugAssign(n) => n.range,
            Self::AnnAssign(n) => n.range,
            Self::For(n) | Self::AsyncFor(n) => n.range,
            Self::While(n) => n.range,
            Self::If(n) => n.range,
            Self::With(n) | Self::AsyncWith(n) => n.range,
            Self::Match(n) => n.range,
            Self::Raise(n) => n.range,
            Self::Try(n) | Self::TryStar(n) => n.range,
            Self::Assert(n) => n.range,
            Self::Import(n) => n.range,
            Self::ImportFrom(n) => n.range,
            Self::Global(n) | Self::Nonlocal(n) => n.range,
            Self::Expr(n) => n.range,
            Self::Pass(n) | Self::Break(n) | Self::Continue(n) => n.range,
            Self::Invalid(n) => n.range,
        }
    }
}

impl Ranged for Expr {
    fn range(&self) -> TextRange {
        match self {
            Self::BoolOp(n) => n.range,
            Self::NamedExpr(n) => n.range,
            Self::BinOp(n) => n.range,
            Self::UnaryOp(n) => n.range,
            Self::Lambda(n) => n.range,
            Self::IfExp(n) => n.range,
            Self::Dict(n) => n.range,
            Self::Set(n) => n.range,
            Self::ListComp(n) | Self::SetComp(n) | Self::DictComp(n) | Self::GeneratorExp(n) => {
                n.range
            }
            Self::Await(n) | Self::Yield(n) | Self::YieldFrom(n) => n.range,
            Self::Compare(n) => n.range,
            Self::Call(n) => n.range,
            Self::FString(n) => n.range,
            Self::FormattedValue(n) => n.range,
            Self::StringLiteral(n) | Self::BytesLiteral(n) => n.range,
            Self::NumberLiteral(n) => n.range,
            Self::BooleanLiteral(n) => n.range,
            Self::NoneLiteral(n) | Self::EllipsisLiteral(n) => n.range,
            Self::Attribute(n) => n.range,
            Self::Subscript(n) => n.range,
            Self::Starred(n) => n.range,
            Self::Name(n) => n.range,
            Self::List(n) | Self::Tuple(n) => n.range,
            Self::Slice(n) => n.range,
            Self::Invalid(n) => n.range,
        }
    }
}

impl Ranged for Pattern {
    fn range(&self) -> TextRange {
        match self {
            Self::Value(n) => n.range,
            Self::Singleton(n) => n.range,
            Self::Sequence(n) => n.range,
            Self::Mapping(n) => n.range,
            Self::Class(n) => n.range,
            Self::Star(n) => n.range,
            Self::As(n) => n.range,
            Self::Or(n) => n.range,
            Self::Invalid(n) => n.range,
        }
    }
}

impl Ranged for TypeParam {
    fn range(&self) -> TextRange {
        match self {
            Self::TypeVar(n) | Self::ParamSpec(n) | Self::TypeVarTuple(n) => n.range,
        }
    }
}

/// A type-erased AST node reference useful for generic traversals.
#[derive(Copy, Clone, Debug)]
/// Public API item.
pub enum AnyNodeRef<'a> {
    /// AST variant.
    Stmt(&'a Stmt),
    /// AST variant.
    Expr(&'a Expr),
    /// AST variant.
    Pattern(&'a Pattern),
}

/// A small helper for creating an empty parameter list.
impl Default for Parameters {
    fn default() -> Self {
        Self {
            posonlyargs: Vec::new(),
            args: Vec::new(),
            vararg: None,
            kwonlyargs: Vec::new(),
            kw_defaults: Vec::new(),
            kwarg: None,
            defaults: Vec::new(),
        }
    }
}

/// Constructs a zero-length source range.
pub const fn empty_range() -> TextRange {
    TextRange::empty(TextSize::new(0))
}
