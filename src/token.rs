//! Python token kinds and language-version switches.

use crate::source::TextRange;

/// Python language versions understood by the parser.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum PythonVersion {
    /// Python 3.8.
    Py38,
    /// Python 3.9.
    Py39,
    /// Python 3.10.
    Py310,
    /// Python 3.11.
    Py311,
    /// Python 3.12.
    Py312,
    /// Python 3.13.
    #[default]
    Py313,
}

impl PythonVersion {
    /// Returns the numeric major/minor pair.
    pub const fn number(self) -> (u8, u8) {
        match self {
            Self::Py38 => (3, 8),
            Self::Py39 => (3, 9),
            Self::Py310 => (3, 10),
            Self::Py311 => (3, 11),
            Self::Py312 => (3, 12),
            Self::Py313 => (3, 13),
        }
    }
    /// Tests whether this version is at least the requested version.
    pub const fn supports(self, minimum: Self) -> bool {
        self.number().1 >= minimum.number().1 || self.number().0 > minimum.number().0
    }
}

/// String prefix flags parsed from a Python literal.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct StringPrefix(u8);

impl StringPrefix {
    /// Raw string flag.
    pub const RAW: Self = Self(1);
    /// Bytes flag.
    pub const BYTES: Self = Self(2);
    /// Unicode flag.
    pub const UNICODE: Self = Self(4);
    /// Formatted-string flag.
    pub const FORMAT: Self = Self(8);
    /// Creates flags from a prefix spelling.
    pub fn parse(prefix: &str) -> Option<Self> {
        let mut result = Self(0);
        for character in prefix.bytes() {
            let flag = match character.to_ascii_lowercase() {
                b'r' => Self::RAW,
                b'b' => Self::BYTES,
                b'u' => Self::UNICODE,
                b'f' => Self::FORMAT,
                _ => return None,
            };
            if result.0 & flag.0 != 0 {
                return None;
            }
            result.0 |= flag.0;
        }
        if result.0 & Self::BYTES.0 != 0 && result.0 & (Self::FORMAT.0 | Self::UNICODE.0) != 0 {
            return None;
        }
        Some(result)
    }
    /// Tests the raw-string flag.
    pub const fn is_raw(self) -> bool {
        self.0 & Self::RAW.0 != 0
    }
    /// Tests the bytes flag.
    pub const fn is_bytes(self) -> bool {
        self.0 & Self::BYTES.0 != 0
    }
    /// Tests the formatted-string flag.
    pub const fn is_format(self) -> bool {
        self.0 & Self::FORMAT.0 != 0
    }
    /// Tests whether no prefix was present.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// A Python token without an owned text payload.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Token {
    /// Token category.
    pub kind: TokenKind,
    /// Source range.
    pub range: TextRange,
}

impl Token {
    /// Creates a token.
    pub const fn new(kind: TokenKind, range: TextRange) -> Self {
        Self { kind, range }
    }
}

/// Token categories emitted by the lexer.
#[allow(missing_docs)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum TokenKind {
    Newline,
    NonLogicalNewline,
    Indent,
    Dedent,
    EndMarker,
    Comment,
    Name,
    Int,
    Float,
    Complex,
    String { prefix: StringPrefix, triple: bool },
    FStringStart { prefix: StringPrefix, triple: bool },
    FStringMiddle,
    FStringEnd { prefix: StringPrefix, triple: bool },
    False,
    None,
    True,
    And,
    As,
    Assert,
    Async,
    Await,
    Break,
    Class,
    Continue,
    Def,
    Del,
    Elif,
    Else,
    Except,
    Finally,
    For,
    From,
    Global,
    If,
    Import,
    In,
    Is,
    Lambda,
    Nonlocal,
    Not,
    Or,
    Pass,
    Raise,
    Return,
    Try,
    While,
    With,
    Yield,
    Plus,
    Minus,
    Star,
    DoubleStar,
    Slash,
    DoubleSlash,
    Percent,
    At,
    LeftShift,
    RightShift,
    Ampersand,
    Vbar,
    CircumFlex,
    Tilde,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    EqEqual,
    NotEqual,
    LPar,
    RPar,
    LSqb,
    RSqb,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Dot,
    Semi,
    Equal,
    Arrow,
    ColonEqual,
    Ellipsis,
    PlusEqual,
    MinusEqual,
    StarEqual,
    SlashEqual,
    DoubleSlashEqual,
    PercentEqual,
    AtEqual,
    AmperEqual,
    VbarEqual,
    CircumflexEqual,
    LeftShiftEqual,
    RightShiftEqual,
    DoubleStarEqual,
    Exclamation,
    Unknown,
}

impl TokenKind {
    /// Classifies a word as a hard keyword; soft keywords remain `Name`.
    pub fn keyword(word: &str) -> Self {
        match word {
            "False" => Self::False,
            "None" => Self::None,
            "True" => Self::True,
            "and" => Self::And,
            "as" => Self::As,
            "assert" => Self::Assert,
            "async" => Self::Async,
            "await" => Self::Await,
            "break" => Self::Break,
            "class" => Self::Class,
            "continue" => Self::Continue,
            "def" => Self::Def,
            "del" => Self::Del,
            "elif" => Self::Elif,
            "else" => Self::Else,
            "except" => Self::Except,
            "finally" => Self::Finally,
            "for" => Self::For,
            "from" => Self::From,
            "global" => Self::Global,
            "if" => Self::If,
            "import" => Self::Import,
            "in" => Self::In,
            "is" => Self::Is,
            "lambda" => Self::Lambda,
            "nonlocal" => Self::Nonlocal,
            "not" => Self::Not,
            "or" => Self::Or,
            "pass" => Self::Pass,
            "raise" => Self::Raise,
            "return" => Self::Return,
            "try" => Self::Try,
            "while" => Self::While,
            "with" => Self::With,
            "yield" => Self::Yield,
            _ => Self::Name,
        }
    }
    /// Returns whether this token is a hard keyword.
    pub const fn is_keyword(self) -> bool {
        matches!(
            self,
            Self::False
                | Self::None
                | Self::True
                | Self::And
                | Self::As
                | Self::Assert
                | Self::Async
                | Self::Await
                | Self::Break
                | Self::Class
                | Self::Continue
                | Self::Def
                | Self::Del
                | Self::Elif
                | Self::Else
                | Self::Except
                | Self::Finally
                | Self::For
                | Self::From
                | Self::Global
                | Self::If
                | Self::Import
                | Self::In
                | Self::Is
                | Self::Lambda
                | Self::Nonlocal
                | Self::Not
                | Self::Or
                | Self::Pass
                | Self::Raise
                | Self::Return
                | Self::Try
                | Self::While
                | Self::With
                | Self::Yield
        )
    }
    /// Returns whether the token has no syntactic expression meaning.
    pub const fn is_trivia(self) -> bool {
        matches!(self, Self::Comment | Self::NonLogicalNewline)
    }
    /// Returns whether the token can begin a statement block.
    pub const fn starts_statement(self) -> bool {
        matches!(
            self,
            Self::Def
                | Self::Class
                | Self::If
                | Self::For
                | Self::While
                | Self::Try
                | Self::With
                | Self::Return
                | Self::Raise
                | Self::Import
                | Self::From
                | Self::Pass
                | Self::Break
                | Self::Continue
                | Self::Global
                | Self::Nonlocal
                | Self::Assert
                | Self::Async
                | Self::Name
        )
    }
}
