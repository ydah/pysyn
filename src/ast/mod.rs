//! CPython-shaped abstract syntax tree types.

#![allow(missing_docs)]

use crate::source::{TextRange, TextSize};
use crate::token::StringPrefix;
use std::borrow::Cow;
use std::fmt;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ExprContext {
    Load,
    Store,
    Del,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum BoolOperator {
    And,
    Or,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum UnaryOperator {
    Invert,
    Not,
    UAdd,
    USub,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum BinaryOperator {
    Add,
    Sub,
    Mult,
    MatMult,
    Div,
    FloorDiv,
    Mod,
    Pow,
    LShift,
    RShift,
    BitOr,
    BitXor,
    BitAnd,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum CompareOperator {
    Eq,
    NotEq,
    Lt,
    LtE,
    Gt,
    GtE,
    In,
    NotIn,
    Is,
    IsNot,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Number {
    Int(Int),
    Float(f64),
    Complex { real: f64, imag: f64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Int(String);

impl Int {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
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
pub struct StringLiteralValue {
    pub parts: Vec<StringLiteralPart>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringLiteralPart {
    pub range: TextRange,
    pub flags: StringFlags,
    pub value: Box<str>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct StringFlags {
    pub prefix: StringPrefix,
    pub triple: bool,
    pub quote: char,
}

impl StringLiteralValue {
    pub fn new(parts: Vec<StringLiteralPart>) -> Self {
        Self { parts }
    }
    pub fn to_str(&self) -> Cow<'_, str> {
        if self.parts.len() == 1 {
            return Cow::Borrowed(&self.parts[0].value);
        }
        Cow::Owned(self.parts.iter().map(|part| part.value.as_ref()).collect())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModModule {
    pub body: Vec<Stmt>,
    pub range: TextRange,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    FunctionDef(StmtFunctionDef),
    AsyncFunctionDef(StmtFunctionDef),
    ClassDef(StmtClassDef),
    Return(StmtReturn),
    Delete(StmtDelete),
    Assign(StmtAssign),
    TypeAlias(StmtTypeAlias),
    AugAssign(StmtAugAssign),
    AnnAssign(StmtAnnAssign),
    For(StmtFor),
    AsyncFor(StmtFor),
    While(StmtWhile),
    If(StmtIf),
    With(StmtWith),
    AsyncWith(StmtWith),
    Match(StmtMatch),
    Raise(StmtRaise),
    Try(StmtTry),
    TryStar(StmtTry),
    Assert(StmtAssert),
    Import(StmtImport),
    ImportFrom(StmtImportFrom),
    Global(StmtNames),
    Nonlocal(StmtNames),
    Expr(StmtExpr),
    Pass(StmtSimple),
    Break(StmtSimple),
    Continue(StmtSimple),
    Invalid(StmtInvalid),
}

#[derive(Clone, Debug, PartialEq)]
pub struct StmtFunctionDef {
    pub range: TextRange,
    pub name: Box<str>,
    pub decorator_list: Vec<Expr>,
    pub type_params: Vec<TypeParam>,
    pub args: Parameters,
    pub returns: Option<Box<Expr>>,
    pub body: Vec<Stmt>,
    pub type_comment: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StmtClassDef {
    pub range: TextRange,
    pub name: Box<str>,
    pub bases: Vec<Expr>,
    pub keywords: Vec<Keyword>,
    pub decorator_list: Vec<Expr>,
    pub type_params: Vec<TypeParam>,
    pub body: Vec<Stmt>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StmtReturn {
    pub range: TextRange,
    pub value: Option<Box<Expr>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StmtDelete {
    pub range: TextRange,
    pub targets: Vec<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StmtAssign {
    pub range: TextRange,
    pub targets: Vec<Expr>,
    pub value: Box<Expr>,
    pub type_comment: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StmtTypeAlias {
    pub range: TextRange,
    pub name: Box<Expr>,
    pub type_params: Vec<TypeParam>,
    pub value: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StmtAugAssign {
    pub range: TextRange,
    pub target: Box<Expr>,
    pub op: BinaryOperator,
    pub value: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StmtAnnAssign {
    pub range: TextRange,
    pub target: Box<Expr>,
    pub annotation: Box<Expr>,
    pub value: Option<Box<Expr>>,
    pub simple: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StmtFor {
    pub range: TextRange,
    pub target: Box<Expr>,
    pub iter: Box<Expr>,
    pub body: Vec<Stmt>,
    pub orelse: Vec<Stmt>,
    pub type_comment: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StmtWhile {
    pub range: TextRange,
    pub test: Box<Expr>,
    pub body: Vec<Stmt>,
    pub orelse: Vec<Stmt>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StmtIf {
    pub range: TextRange,
    pub test: Box<Expr>,
    pub body: Vec<Stmt>,
    pub orelse: Vec<Stmt>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StmtWith {
    pub range: TextRange,
    pub items: Vec<WithItem>,
    pub body: Vec<Stmt>,
    pub type_comment: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StmtMatch {
    pub range: TextRange,
    pub subject: Box<Expr>,
    pub cases: Vec<MatchCase>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StmtRaise {
    pub range: TextRange,
    pub exc: Option<Box<Expr>>,
    pub cause: Option<Box<Expr>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StmtTry {
    pub range: TextRange,
    pub body: Vec<Stmt>,
    pub handlers: Vec<ExceptHandler>,
    pub orelse: Vec<Stmt>,
    pub finalbody: Vec<Stmt>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StmtAssert {
    pub range: TextRange,
    pub test: Box<Expr>,
    pub msg: Option<Box<Expr>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StmtImport {
    pub range: TextRange,
    pub names: Vec<Alias>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StmtImportFrom {
    pub range: TextRange,
    pub module: Option<Box<str>>,
    pub names: Vec<Alias>,
    pub level: u32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StmtNames {
    pub range: TextRange,
    pub names: Vec<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StmtExpr {
    pub range: TextRange,
    pub value: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StmtSimple {
    pub range: TextRange,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StmtInvalid {
    pub range: TextRange,
    pub message: Box<str>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    BoolOp(ExprBoolOp),
    NamedExpr(ExprNamedExpr),
    BinOp(ExprBinOp),
    UnaryOp(ExprUnaryOp),
    Lambda(ExprLambda),
    IfExp(ExprIfExp),
    Dict(ExprDict),
    Set(ExprSet),
    ListComp(ExprComprehension),
    SetComp(ExprComprehension),
    DictComp(ExprComprehension),
    GeneratorExp(ExprComprehension),
    Await(ExprUnaryValue),
    Yield(ExprUnaryValue),
    YieldFrom(ExprUnaryValue),
    Compare(ExprCompare),
    Call(ExprCall),
    FString(ExprFString),
    FormattedValue(ExprFormattedValue),
    StringLiteral(ExprString),
    BytesLiteral(ExprString),
    NumberLiteral(ExprNumber),
    BooleanLiteral(ExprBoolean),
    NoneLiteral(ExprLiteral),
    EllipsisLiteral(ExprLiteral),
    Attribute(ExprAttribute),
    Subscript(ExprSubscript),
    Starred(ExprStarred),
    Name(ExprName),
    List(ExprSequence),
    Tuple(ExprSequence),
    Slice(ExprSlice),
    Invalid(ExprInvalid),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExprBoolOp {
    pub range: TextRange,
    pub op: BoolOperator,
    pub values: Vec<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprNamedExpr {
    pub range: TextRange,
    pub target: Box<Expr>,
    pub value: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprBinOp {
    pub range: TextRange,
    pub left: Box<Expr>,
    pub op: BinaryOperator,
    pub right: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprUnaryOp {
    pub range: TextRange,
    pub op: UnaryOperator,
    pub operand: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprLambda {
    pub range: TextRange,
    pub args: Parameters,
    pub body: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprIfExp {
    pub range: TextRange,
    pub body: Box<Expr>,
    pub test: Box<Expr>,
    pub orelse: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprDict {
    pub range: TextRange,
    pub keys: Vec<Option<Expr>>,
    pub values: Vec<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprSet {
    pub range: TextRange,
    pub elts: Vec<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprComprehension {
    pub range: TextRange,
    pub elt: Box<Expr>,
    pub generators: Vec<Comprehension>,
    pub key: Option<Box<Expr>>,
    pub value: Option<Box<Expr>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprUnaryValue {
    pub range: TextRange,
    pub value: Option<Box<Expr>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprCompare {
    pub range: TextRange,
    pub left: Box<Expr>,
    pub ops: Vec<CompareOperator>,
    pub comparators: Vec<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprCall {
    pub range: TextRange,
    pub func: Box<Expr>,
    pub args: Vec<Expr>,
    pub keywords: Vec<Keyword>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprFString {
    pub range: TextRange,
    pub values: Vec<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprFormattedValue {
    pub range: TextRange,
    pub value: Box<Expr>,
    pub conversion: Option<char>,
    pub format_spec: Option<Box<Expr>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprString {
    pub range: TextRange,
    pub value: StringLiteralValue,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprNumber {
    pub range: TextRange,
    pub value: Number,
    pub raw: Box<str>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprBoolean {
    pub range: TextRange,
    pub value: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprLiteral {
    pub range: TextRange,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprAttribute {
    pub range: TextRange,
    pub value: Box<Expr>,
    pub attr: Box<str>,
    pub ctx: ExprContext,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprSubscript {
    pub range: TextRange,
    pub value: Box<Expr>,
    pub slice: Box<Expr>,
    pub ctx: ExprContext,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprStarred {
    pub range: TextRange,
    pub value: Box<Expr>,
    pub ctx: ExprContext,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprName {
    pub range: TextRange,
    pub id: Box<str>,
    pub ctx: ExprContext,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprSequence {
    pub range: TextRange,
    pub elts: Vec<Expr>,
    pub ctx: ExprContext,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprSlice {
    pub range: TextRange,
    pub lower: Option<Box<Expr>>,
    pub upper: Option<Box<Expr>>,
    pub step: Option<Box<Expr>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExprInvalid {
    pub range: TextRange,
    pub message: Box<str>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Parameters {
    pub posonlyargs: Vec<Parameter>,
    pub args: Vec<Parameter>,
    pub vararg: Option<Parameter>,
    pub kwonlyargs: Vec<Parameter>,
    pub kw_defaults: Vec<Option<Expr>>,
    pub kwarg: Option<Parameter>,
    pub defaults: Vec<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    pub range: TextRange,
    pub name: Box<str>,
    pub annotation: Option<Box<Expr>>,
    pub default: Option<Box<Expr>>,
    pub type_comment: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Keyword {
    pub range: TextRange,
    pub arg: Option<Box<str>>,
    pub value: Expr,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Alias {
    pub range: TextRange,
    pub name: Box<str>,
    pub asname: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct WithItem {
    pub range: TextRange,
    pub context_expr: Expr,
    pub optional_vars: Option<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExceptHandler {
    pub range: TextRange,
    pub typ: Option<Expr>,
    pub name: Option<Box<str>>,
    pub body: Vec<Stmt>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Comprehension {
    pub range: TextRange,
    pub target: Expr,
    pub iter: Expr,
    pub ifs: Vec<Expr>,
    pub is_async: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct MatchCase {
    pub range: TextRange,
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Vec<Stmt>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    Value(PatternValue),
    Singleton(PatternSingleton),
    Sequence(PatternSequence),
    Mapping(PatternMapping),
    Class(PatternClass),
    Star(PatternStar),
    As(PatternAs),
    Or(PatternOr),
    Invalid(PatternInvalid),
}
#[derive(Clone, Debug, PartialEq)]
pub struct PatternValue {
    pub range: TextRange,
    pub value: Expr,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PatternSingleton {
    pub range: TextRange,
    pub value: Expr,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PatternSequence {
    pub range: TextRange,
    pub patterns: Vec<Pattern>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PatternMapping {
    pub range: TextRange,
    pub keys: Vec<Expr>,
    pub patterns: Vec<Pattern>,
    pub rest: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PatternClass {
    pub range: TextRange,
    pub cls: Expr,
    pub patterns: Vec<Pattern>,
    pub kwd_attrs: Vec<Box<str>>,
    pub kwd_patterns: Vec<Pattern>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PatternStar {
    pub range: TextRange,
    pub name: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PatternAs {
    pub range: TextRange,
    pub pattern: Option<Box<Pattern>>,
    pub name: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PatternOr {
    pub range: TextRange,
    pub patterns: Vec<Pattern>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PatternInvalid {
    pub range: TextRange,
}
#[derive(Clone, Debug, PartialEq)]
pub enum TypeParam {
    TypeVar(TypeParamData),
    ParamSpec(TypeParamData),
    TypeVarTuple(TypeParamData),
}
#[derive(Clone, Debug, PartialEq)]
pub struct TypeParamData {
    pub range: TextRange,
    pub name: Box<str>,
    pub bound: Option<Expr>,
    pub default: Option<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Comment {
    pub range: TextRange,
    pub text: Box<str>,
}

/// Returns the source range of an AST node.
pub trait Ranged {
    fn range(&self) -> TextRange;
}

macro_rules! ranged_structs { ($($ty:ty),+ $(,)?) => { $(impl Ranged for $ty { fn range(&self) -> TextRange { self.range } })+ }; }
ranged_structs!(
    ModModule,
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
pub enum AnyNodeRef<'a> {
    Stmt(&'a Stmt),
    Expr(&'a Expr),
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
