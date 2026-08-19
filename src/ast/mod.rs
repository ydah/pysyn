//! CPython-shaped abstract syntax tree types.

//! The public enums and records mirror Python's `ast` module. Every node
//! carries a byte range, while parser-owned source text remains private.
use crate::source::{TextRange, TextSize};
use crate::token::StringPrefix;
use std::borrow::Cow;
use std::fmt;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// Specifies whether an expression is being loaded, stored, or deleted.
pub enum ExprContext {
    /// AST variant for `Load` syntax.
    Load,
    /// AST variant for `Store` syntax.
    Store,
    /// AST variant for `Del` syntax.
    Del,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// Represents a boolean conjunction or disjunction.
pub enum BoolOperator {
    /// AST variant for `And` syntax.
    And,
    /// AST variant for `Or` syntax.
    Or,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// Represents a unary Python operator.
pub enum UnaryOperator {
    /// AST variant for `Invert` syntax.
    Invert,
    /// AST variant for `Not` syntax.
    Not,
    /// AST variant for `UAdd` syntax.
    UAdd,
    /// AST variant for `USub` syntax.
    USub,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// Represents a binary Python operator.
pub enum BinaryOperator {
    /// AST variant for `Add` syntax.
    Add,
    /// AST variant for `Sub` syntax.
    Sub,
    /// AST variant for `Mult` syntax.
    Mult,
    /// AST variant for `MatMult` syntax.
    MatMult,
    /// AST variant for `Div` syntax.
    Div,
    /// AST variant for `FloorDiv` syntax.
    FloorDiv,
    /// AST variant for `Mod` syntax.
    Mod,
    /// AST variant for `Pow` syntax.
    Pow,
    /// AST variant for `LShift` syntax.
    LShift,
    /// AST variant for `RShift` syntax.
    RShift,
    /// AST variant for `BitOr` syntax.
    BitOr,
    /// AST variant for `BitXor` syntax.
    BitXor,
    /// AST variant for `BitAnd` syntax.
    BitAnd,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// Represents a Python comparison operator.
pub enum CompareOperator {
    /// AST variant for `Eq` syntax.
    Eq,
    /// AST variant for `NotEq` syntax.
    NotEq,
    /// AST variant for `Lt` syntax.
    Lt,
    /// AST variant for `LtE` syntax.
    LtE,
    /// AST variant for `Gt` syntax.
    Gt,
    /// AST variant for `GtE` syntax.
    GtE,
    /// AST variant for `In` syntax.
    In,
    /// AST variant for `NotIn` syntax.
    NotIn,
    /// AST variant for `Is` syntax.
    Is,
    /// AST variant for `IsNot` syntax.
    IsNot,
}

#[derive(Clone, Debug, PartialEq)]
/// Represents the numeric literal categories preserved by the AST.
pub enum Number {
    /// AST variant for `Int` syntax.
    Int(Int),
    /// AST variant for `Float` syntax.
    Float(f64),
    /// AST variant for `Complex` syntax.
    Complex {
        /// Real component.
        real: f64,
        /// Imaginary component.
        imag: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Preserves the source spelling of an integer literal.
pub struct Int(String);

impl Int {
    /// Creates a literal value from its source spelling.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    /// Returns the preserved source spelling of the integer literal.
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
/// Stores one or more adjacent Python string literal parts.
pub struct StringLiteralValue {
    /// Adjacent literal parts making up the string value.
    pub parts: Vec<StringLiteralPart>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Stores one source-level string literal and its decoded value.
pub struct StringLiteralPart {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Lexical prefix, quote, and delimiter information.
    pub flags: StringFlags,
    /// Expression value carried by this node.
    pub value: Box<str>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// Stores the lexical flags of a string literal.
pub struct StringFlags {
    /// String prefix flags such as raw, bytes, or format.
    pub prefix: StringPrefix,
    /// Whether the literal uses a triple-quoted delimiter.
    pub triple: bool,
    /// Quote character used by the literal.
    pub quote: char,
}

impl StringLiteralValue {
    /// Creates a literal value from its source spelling.
    pub fn new(parts: Vec<StringLiteralPart>) -> Self {
        Self { parts }
    }
    /// Returns the concatenated decoded string value.
    pub fn to_str(&self) -> Cow<'_, str> {
        if self.parts.len() == 1 {
            return Cow::Borrowed(&self.parts[0].value);
        }
        Cow::Owned(self.parts.iter().map(|part| part.value.as_ref()).collect())
    }
}

#[derive(Clone, Debug, PartialEq)]
/// Represents a parsed Python module.
pub struct ModModule {
    /// Statements contained by this construct.
    pub body: Vec<Stmt>,
    /// Type-ignore directives recorded in this module.
    pub type_ignores: Vec<TypeIgnore>,
    /// Source range occupied by this node.
    pub range: TextRange,
    pub(crate) source: Option<Box<str>>,
}

#[derive(Clone, Debug, PartialEq)]
/// Stores a `# type: ignore` directive from the source.
pub struct TypeIgnore {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// One-based source line recorded for this directive.
    pub lineno: u32,
    /// Text following the `type: ignore` marker.
    pub tag: Box<str>,
}

#[derive(Clone, Debug, PartialEq)]
/// Represents a Python statement and its associated source range.
pub enum Stmt {
    /// AST variant for `FunctionDef` syntax.
    FunctionDef(Box<StmtFunctionDef>),
    /// AST variant for `AsyncFunctionDef` syntax.
    AsyncFunctionDef(Box<StmtFunctionDef>),
    /// AST variant for `ClassDef` syntax.
    ClassDef(Box<StmtClassDef>),
    /// AST variant for `Return` syntax.
    Return(StmtReturn),
    /// AST variant for `Delete` syntax.
    Delete(StmtDelete),
    /// AST variant for `Assign` syntax.
    Assign(StmtAssign),
    /// AST variant for `TypeAlias` syntax.
    TypeAlias(StmtTypeAlias),
    /// AST variant for `AugAssign` syntax.
    AugAssign(StmtAugAssign),
    /// AST variant for `AnnAssign` syntax.
    AnnAssign(StmtAnnAssign),
    /// AST variant for `For` syntax.
    For(StmtFor),
    /// AST variant for `AsyncFor` syntax.
    AsyncFor(StmtFor),
    /// AST variant for `While` syntax.
    While(StmtWhile),
    /// AST variant for `If` syntax.
    If(StmtIf),
    /// AST variant for `With` syntax.
    With(StmtWith),
    /// AST variant for `AsyncWith` syntax.
    AsyncWith(StmtWith),
    /// AST variant for `Match` syntax.
    Match(StmtMatch),
    /// AST variant for `Raise` syntax.
    Raise(StmtRaise),
    /// AST variant for `Try` syntax.
    Try(Box<StmtTry>),
    /// AST variant for `TryStar` syntax.
    TryStar(Box<StmtTry>),
    /// AST variant for `Assert` syntax.
    Assert(StmtAssert),
    /// AST variant for `Import` syntax.
    Import(StmtImport),
    /// AST variant for `ImportFrom` syntax.
    ImportFrom(StmtImportFrom),
    /// AST variant for `Global` syntax.
    Global(StmtNames),
    /// AST variant for `Nonlocal` syntax.
    Nonlocal(StmtNames),
    /// AST variant for `Expr` syntax.
    Expr(StmtExpr),
    /// AST variant for `Pass` syntax.
    Pass(StmtSimple),
    /// AST variant for `Break` syntax.
    Break(StmtSimple),
    /// AST variant for `Continue` syntax.
    Continue(StmtSimple),
    /// AST variant for `Invalid` syntax.
    Invalid(StmtInvalid),
}

#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `FunctionDef` statement node.
pub struct StmtFunctionDef {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Identifier or binding name represented by this node.
    pub name: Box<str>,
    /// Decorators applied to this definition.
    pub decorator_list: Vec<Expr>,
    /// Type parameters declared by this construct.
    pub type_params: Vec<TypeParam>,
    /// Positional and variadic parameters of this callable.
    pub args: Parameters,
    /// Optional return annotation expression.
    pub returns: Option<Box<Expr>>,
    /// Statements contained by this construct.
    pub body: Vec<Stmt>,
    /// Optional PEP 484 type comment attached to this construct.
    pub type_comment: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `ClassDef` statement node.
pub struct StmtClassDef {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Identifier or binding name represented by this node.
    pub name: Box<str>,
    /// Base-class expressions used by this class definition.
    pub bases: Vec<Expr>,
    /// Keyword arguments associated with this construct.
    pub keywords: Vec<Keyword>,
    /// Decorators applied to this definition.
    pub decorator_list: Vec<Expr>,
    /// Type parameters declared by this construct.
    pub type_params: Vec<TypeParam>,
    /// Statements contained by this construct.
    pub body: Vec<Stmt>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Return` statement node.
pub struct StmtReturn {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Expression value carried by this node.
    pub value: Option<Box<Expr>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Delete` statement node.
pub struct StmtDelete {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Assignment or deletion targets, in source order.
    pub targets: Vec<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Assign` statement node.
pub struct StmtAssign {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Assignment or deletion targets, in source order.
    pub targets: Vec<Expr>,
    /// Expression value carried by this node.
    pub value: Box<Expr>,
    /// Optional PEP 484 type comment attached to this construct.
    pub type_comment: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `TypeAlias` statement node.
pub struct StmtTypeAlias {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Identifier or binding name represented by this node.
    pub name: Box<Expr>,
    /// Type parameters declared by this construct.
    pub type_params: Vec<TypeParam>,
    /// Expression value carried by this node.
    pub value: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `AugAssign` statement node.
pub struct StmtAugAssign {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Expression receiving the assignment or iteration value.
    pub target: Box<Expr>,
    /// Operator applied by this expression.
    pub op: BinaryOperator,
    /// Expression value carried by this node.
    pub value: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `AnnAssign` statement node.
pub struct StmtAnnAssign {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Expression receiving the assignment or iteration value.
    pub target: Box<Expr>,
    /// Optional or required type annotation expression.
    pub annotation: Box<Expr>,
    /// Expression value carried by this node.
    pub value: Option<Box<Expr>>,
    /// Whether the annotation target is a simple name.
    pub simple: bool,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `For` statement node.
pub struct StmtFor {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Expression receiving the assignment or iteration value.
    pub target: Box<Expr>,
    /// Iterable expression consumed by this loop or comprehension clause.
    pub iter: Box<Expr>,
    /// Statements contained by this construct.
    pub body: Vec<Stmt>,
    /// Statements in the construct's `else` branch.
    pub orelse: Vec<Stmt>,
    /// Optional PEP 484 type comment attached to this construct.
    pub type_comment: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `While` statement node.
pub struct StmtWhile {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Condition expression evaluated by this construct.
    pub test: Box<Expr>,
    /// Statements contained by this construct.
    pub body: Vec<Stmt>,
    /// Statements in the construct's `else` branch.
    pub orelse: Vec<Stmt>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `If` statement node.
pub struct StmtIf {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Condition expression evaluated by this construct.
    pub test: Box<Expr>,
    /// Statements contained by this construct.
    pub body: Vec<Stmt>,
    /// Statements in the construct's `else` branch.
    pub orelse: Vec<Stmt>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `With` statement node.
pub struct StmtWith {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Context-manager items contained by this `with` statement.
    pub items: Vec<WithItem>,
    /// Statements contained by this construct.
    pub body: Vec<Stmt>,
    /// Optional PEP 484 type comment attached to this construct.
    pub type_comment: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Match` statement node.
pub struct StmtMatch {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Expression examined by this `match` statement.
    pub subject: Box<Expr>,
    /// Pattern-matching cases contained by this `match` statement.
    pub cases: Vec<MatchCase>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Raise` statement node.
pub struct StmtRaise {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Optional exception expression being raised.
    pub exc: Option<Box<Expr>>,
    /// Optional explicit exception cause expression.
    pub cause: Option<Box<Expr>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Try` statement node.
pub struct StmtTry {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Statements contained by this construct.
    pub body: Vec<Stmt>,
    /// Exception handlers contained by this `try` statement.
    pub handlers: Vec<ExceptHandler>,
    /// Statements in the construct's `else` branch.
    pub orelse: Vec<Stmt>,
    /// Statements in the `finally` branch.
    pub finalbody: Vec<Stmt>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Assert` statement node.
pub struct StmtAssert {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Condition expression evaluated by this construct.
    pub test: Box<Expr>,
    /// Optional assertion message expression.
    pub msg: Option<Box<Expr>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Import` statement node.
pub struct StmtImport {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Names declared or imported by this construct.
    pub names: Vec<Alias>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `ImportFrom` statement node.
pub struct StmtImportFrom {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Optional module path used by a relative import.
    pub module: Option<Box<str>>,
    /// Names declared or imported by this construct.
    pub names: Vec<Alias>,
    /// Number of leading dots in a relative import.
    pub level: u32,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Names` statement node.
pub struct StmtNames {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Names declared or imported by this construct.
    pub names: Vec<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Expr` statement node.
pub struct StmtExpr {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Expression value carried by this node.
    pub value: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Simple` statement node.
pub struct StmtSimple {
    /// Source range occupied by this node.
    pub range: TextRange,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Invalid` statement node.
pub struct StmtInvalid {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Parser-provided description of the invalid construct.
    pub message: Box<str>,
}

#[derive(Clone, Debug, PartialEq)]
/// Represents a Python expression and its associated source range.
pub enum Expr {
    /// AST variant for `BoolOp` syntax.
    BoolOp(ExprBoolOp),
    /// AST variant for `NamedExpr` syntax.
    NamedExpr(ExprNamedExpr),
    /// AST variant for `BinOp` syntax.
    BinOp(ExprBinOp),
    /// AST variant for `UnaryOp` syntax.
    UnaryOp(ExprUnaryOp),
    /// AST variant for `Lambda` syntax.
    Lambda(Box<ExprLambda>),
    /// AST variant for `IfExp` syntax.
    IfExp(ExprIfExp),
    /// AST variant for `Dict` syntax.
    Dict(ExprDict),
    /// AST variant for `Set` syntax.
    Set(ExprSet),
    /// AST variant for `ListComp` syntax.
    ListComp(ExprComprehension),
    /// AST variant for `SetComp` syntax.
    SetComp(ExprComprehension),
    /// AST variant for `DictComp` syntax.
    DictComp(ExprComprehension),
    /// AST variant for `GeneratorExp` syntax.
    GeneratorExp(ExprComprehension),
    /// AST variant for `Await` syntax.
    Await(ExprUnaryValue),
    /// AST variant for `Yield` syntax.
    Yield(ExprUnaryValue),
    /// AST variant for `YieldFrom` syntax.
    YieldFrom(ExprUnaryValue),
    /// AST variant for `Compare` syntax.
    Compare(Box<ExprCompare>),
    /// AST variant for `Call` syntax.
    Call(Box<ExprCall>),
    /// AST variant for `FString` syntax.
    FString(ExprFString),
    /// AST variant for `FormattedValue` syntax.
    FormattedValue(ExprFormattedValue),
    /// AST variant for `StringLiteral` syntax.
    StringLiteral(ExprString),
    /// AST variant for `BytesLiteral` syntax.
    BytesLiteral(ExprString),
    /// AST variant for `NumberLiteral` syntax.
    NumberLiteral(ExprNumber),
    /// AST variant for `BooleanLiteral` syntax.
    BooleanLiteral(ExprBoolean),
    /// AST variant for `NoneLiteral` syntax.
    NoneLiteral(ExprLiteral),
    /// AST variant for `EllipsisLiteral` syntax.
    EllipsisLiteral(ExprLiteral),
    /// AST variant for `Attribute` syntax.
    Attribute(ExprAttribute),
    /// AST variant for `Subscript` syntax.
    Subscript(ExprSubscript),
    /// AST variant for `Starred` syntax.
    Starred(ExprStarred),
    /// AST variant for `Name` syntax.
    Name(ExprName),
    /// AST variant for `List` syntax.
    List(ExprSequence),
    /// AST variant for `Tuple` syntax.
    Tuple(ExprSequence),
    /// AST variant for `Slice` syntax.
    Slice(ExprSlice),
    /// AST variant for `Invalid` syntax.
    Invalid(ExprInvalid),
}

#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `BoolOp` expression node.
pub struct ExprBoolOp {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Operator applied by this expression.
    pub op: BoolOperator,
    /// Values produced or combined by this expression.
    pub values: Vec<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `NamedExpr` expression node.
pub struct ExprNamedExpr {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Expression receiving the assignment or iteration value.
    pub target: Box<Expr>,
    /// Expression value carried by this node.
    pub value: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `BinOp` expression node.
pub struct ExprBinOp {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Left-hand operand of this binary expression.
    pub left: Box<Expr>,
    /// Operator applied by this expression.
    pub op: BinaryOperator,
    /// Right-hand operand of this binary expression.
    pub right: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `UnaryOp` expression node.
pub struct ExprUnaryOp {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Operator applied by this expression.
    pub op: UnaryOperator,
    /// Operand of this unary expression.
    pub operand: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Lambda` expression node.
pub struct ExprLambda {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Positional and variadic parameters of this callable.
    pub args: Parameters,
    /// Statements contained by this construct.
    pub body: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `IfExp` expression node.
pub struct ExprIfExp {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Statements contained by this construct.
    pub body: Box<Expr>,
    /// Condition expression evaluated by this construct.
    pub test: Box<Expr>,
    /// Statements in the construct's `else` branch.
    pub orelse: Box<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Dict` expression node.
pub struct ExprDict {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Key expressions associated with this mapping or dictionary.
    pub keys: Vec<Option<Expr>>,
    /// Values produced or combined by this expression.
    pub values: Vec<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Set` expression node.
pub struct ExprSet {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Elements contained by this list, tuple, or set expression.
    pub elts: Vec<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Comprehension` expression node.
pub struct ExprComprehension {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Element expression produced by this comprehension.
    pub elt: Box<Expr>,
    /// Generator clauses used by this comprehension.
    pub generators: Vec<Comprehension>,
    /// Key expression produced by this dictionary comprehension.
    pub key: Option<Box<Expr>>,
    /// Expression value carried by this node.
    pub value: Option<Box<Expr>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `UnaryValue` expression node.
pub struct ExprUnaryValue {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Expression value carried by this node.
    pub value: Option<Box<Expr>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Compare` expression node.
pub struct ExprCompare {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Left-hand operand of this binary expression.
    pub left: Box<Expr>,
    /// Comparison operators applied between the operands.
    pub ops: Vec<CompareOperator>,
    /// Expressions compared with the left-hand operand.
    pub comparators: Vec<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Call` expression node.
pub struct ExprCall {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Callable expression invoked by this call.
    pub func: Box<Expr>,
    /// Positional and variadic parameters of this callable.
    pub args: Vec<Expr>,
    /// Keyword arguments associated with this construct.
    pub keywords: Vec<Keyword>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `FString` expression node.
pub struct ExprFString {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Values produced or combined by this expression.
    pub values: Vec<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `FormattedValue` expression node.
pub struct ExprFormattedValue {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Expression value carried by this node.
    pub value: Box<Expr>,
    /// Optional f-string conversion character.
    pub conversion: Option<char>,
    /// Optional f-string format-specification expression.
    pub format_spec: Option<Box<Expr>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `String` expression node.
pub struct ExprString {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Expression value carried by this node.
    pub value: StringLiteralValue,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Number` expression node.
pub struct ExprNumber {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Expression value carried by this node.
    pub value: Number,
    /// Original source spelling of this numeric literal.
    pub raw: Box<str>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Boolean` expression node.
pub struct ExprBoolean {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Expression value carried by this node.
    pub value: bool,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Literal` expression node.
pub struct ExprLiteral {
    /// Source range occupied by this node.
    pub range: TextRange,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Attribute` expression node.
pub struct ExprAttribute {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Expression value carried by this node.
    pub value: Box<Expr>,
    /// Attribute name selected by this attribute expression.
    pub attr: Box<str>,
    /// Expression context indicating load, store, or delete.
    pub ctx: ExprContext,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Subscript` expression node.
pub struct ExprSubscript {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Expression value carried by this node.
    pub value: Box<Expr>,
    /// Subscript or slice expression applied to the value.
    pub slice: Box<Expr>,
    /// Expression context indicating load, store, or delete.
    pub ctx: ExprContext,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Starred` expression node.
pub struct ExprStarred {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Expression value carried by this node.
    pub value: Box<Expr>,
    /// Expression context indicating load, store, or delete.
    pub ctx: ExprContext,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Name` expression node.
pub struct ExprName {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Identifier text represented by this name expression.
    pub id: Box<str>,
    /// Expression context indicating load, store, or delete.
    pub ctx: ExprContext,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Sequence` expression node.
pub struct ExprSequence {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Elements contained by this list, tuple, or set expression.
    pub elts: Vec<Expr>,
    /// Expression context indicating load, store, or delete.
    pub ctx: ExprContext,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Slice` expression node.
pub struct ExprSlice {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Optional lower bound of this slice.
    pub lower: Option<Box<Expr>>,
    /// Optional upper bound of this slice.
    pub upper: Option<Box<Expr>>,
    /// Optional step expression of this slice.
    pub step: Option<Box<Expr>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the Python `Invalid` expression node.
pub struct ExprInvalid {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Parser-provided description of the invalid construct.
    pub message: Box<str>,
}

#[derive(Clone, Debug, PartialEq)]
/// Stores the positional, variadic, keyword-only, and default parameters of a callable.
pub struct Parameters {
    /// Positional-only parameters, in declaration order.
    pub posonlyargs: Vec<Parameter>,
    /// Positional and variadic parameters of this callable.
    pub args: Vec<Parameter>,
    /// Optional variadic positional parameter.
    pub vararg: Option<Parameter>,
    /// Keyword-only parameters, in declaration order.
    pub kwonlyargs: Vec<Parameter>,
    /// Defaults corresponding to the keyword-only parameters.
    pub kw_defaults: Vec<Option<Expr>>,
    /// Optional variadic keyword parameter.
    pub kwarg: Option<Parameter>,
    /// Defaults corresponding to the trailing positional parameters.
    pub defaults: Vec<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Stores one callable parameter and its optional annotation.
pub struct Parameter {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Identifier or binding name represented by this node.
    pub name: Box<str>,
    /// Optional or required type annotation expression.
    pub annotation: Option<Box<Expr>>,
    /// Optional PEP 484 type comment attached to this construct.
    pub type_comment: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Stores one keyword argument in a call or class definition.
pub struct Keyword {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Optional keyword name; `None` denotes a positional argument.
    pub arg: Option<Box<str>>,
    /// Expression value carried by this node.
    pub value: Expr,
}
#[derive(Clone, Debug, PartialEq)]
/// Stores one import name and its optional alias.
pub struct Alias {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Identifier or binding name represented by this node.
    pub name: Box<str>,
    /// Optional import alias.
    pub asname: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Stores one context-manager item in a `with` statement.
pub struct WithItem {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Context-manager expression evaluated by this item.
    pub context_expr: Expr,
    /// Optional target receiving the context-manager value.
    pub optional_vars: Option<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Stores one `except` or `except*` handler.
pub struct ExceptHandler {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Optional exception type expression.
    pub typ: Option<Expr>,
    /// Identifier or binding name represented by this node.
    pub name: Option<Box<str>>,
    /// Statements contained by this construct.
    pub body: Vec<Stmt>,
}
#[derive(Clone, Debug, PartialEq)]
/// Stores one generator clause in a comprehension.
pub struct Comprehension {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Expression receiving the assignment or iteration value.
    pub target: Expr,
    /// Iterable expression consumed by this loop or comprehension clause.
    pub iter: Expr,
    /// Conditions that filter this comprehension clause.
    pub ifs: Vec<Expr>,
    /// Whether this comprehension clause uses `async for`.
    pub is_async: bool,
}
#[derive(Clone, Debug, PartialEq)]
/// Stores one `case` arm in a `match` statement.
pub struct MatchCase {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Pattern matched by this case or pattern node.
    pub pattern: Pattern,
    /// Optional condition guarding this match case.
    pub guard: Option<Expr>,
    /// Statements contained by this construct.
    pub body: Vec<Stmt>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents a structural-pattern matching node.
pub enum Pattern {
    /// AST variant for `Value` syntax.
    Value(PatternValue),
    /// AST variant for `Singleton` syntax.
    Singleton(PatternSingleton),
    /// AST variant for `Sequence` syntax.
    Sequence(PatternSequence),
    /// AST variant for `Mapping` syntax.
    Mapping(PatternMapping),
    /// AST variant for `Class` syntax.
    Class(PatternClass),
    /// AST variant for `Star` syntax.
    Star(PatternStar),
    /// AST variant for `As` syntax.
    As(PatternAs),
    /// AST variant for `Or` syntax.
    Or(PatternOr),
    /// AST variant for `Invalid` syntax.
    Invalid(PatternInvalid),
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the `Value` structural pattern node.
pub struct PatternValue {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Expression value carried by this node.
    pub value: Expr,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the `Singleton` structural pattern node.
pub struct PatternSingleton {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Expression value carried by this node.
    pub value: Expr,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the `Sequence` structural pattern node.
pub struct PatternSequence {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Nested patterns contained by this pattern node.
    pub patterns: Vec<Pattern>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the `Mapping` structural pattern node.
pub struct PatternMapping {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Key expressions associated with this mapping or dictionary.
    pub keys: Vec<Expr>,
    /// Nested patterns contained by this pattern node.
    pub patterns: Vec<Pattern>,
    /// Optional name capturing unmatched mapping keys.
    pub rest: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the `Class` structural pattern node.
pub struct PatternClass {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Class expression used by this class pattern.
    pub cls: Expr,
    /// Nested patterns contained by this pattern node.
    pub patterns: Vec<Pattern>,
    /// Keyword attribute names used by this class pattern.
    pub kwd_attrs: Vec<Box<str>>,
    /// Patterns matched against the keyword attributes.
    pub kwd_patterns: Vec<Pattern>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the `Star` structural pattern node.
pub struct PatternStar {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Identifier or binding name represented by this node.
    pub name: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the `As` structural pattern node.
pub struct PatternAs {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Pattern matched by this case or pattern node.
    pub pattern: Option<Box<Pattern>>,
    /// Identifier or binding name represented by this node.
    pub name: Option<Box<str>>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the `Or` structural pattern node.
pub struct PatternOr {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Nested patterns contained by this pattern node.
    pub patterns: Vec<Pattern>,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents the `Invalid` structural pattern node.
pub struct PatternInvalid {
    /// Source range occupied by this node.
    pub range: TextRange,
}
#[derive(Clone, Debug, PartialEq)]
/// Represents a Python 3.12+ type parameter declaration.
pub enum TypeParam {
    /// AST variant for `TypeVar` syntax.
    TypeVar(TypeParamData),
    /// AST variant for `ParamSpec` syntax.
    ParamSpec(TypeParamData),
    /// AST variant for `TypeVarTuple` syntax.
    TypeVarTuple(TypeParamData),
}
#[derive(Clone, Debug, PartialEq)]
/// Stores the name, bound, and default of a type parameter.
pub struct TypeParamData {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Identifier or binding name represented by this node.
    pub name: Box<str>,
    /// Optional type-variable bound expression.
    pub bound: Option<Expr>,
    /// Optional type-parameter default expression.
    pub default: Option<Expr>,
}
#[derive(Clone, Debug, PartialEq)]
/// Stores a source comment retained by the parser.
pub struct Comment {
    /// Source range occupied by this node.
    pub range: TextRange,
    /// Comment text without its leading marker.
    pub text: Box<str>,
}

/// Returns the source range of an AST node.
pub trait Ranged {
    /// Visits this node and then traverses its descendants.
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
/// References one of the AST categories exposed to generic traversals.
pub enum AnyNodeRef<'a> {
    /// AST variant for `Stmt` syntax.
    Stmt(&'a Stmt),
    /// AST variant for `Expr` syntax.
    Expr(&'a Expr),
    /// AST variant for `Pattern` syntax.
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
