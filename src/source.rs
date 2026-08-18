//! Source text and byte-range utilities.

use std::fmt;
use std::ops::{Add, AddAssign, Index, Sub, SubAssign};

/// A UTF-8 byte offset in a source file.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TextSize(u32);

impl TextSize {
    /// Creates an offset from a byte count.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
    /// Returns the represented byte count.
    pub const fn raw(self) -> u32 {
        self.0
    }
    /// Returns the offset as a platform-sized integer.
    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Debug for TextSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for TextSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<u32> for TextSize {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<TextSize> for u32 {
    fn from(value: TextSize) -> Self {
        value.0
    }
}

impl Add<u32> for TextSize {
    type Output = Self;
    fn add(self, rhs: u32) -> Self::Output {
        Self(self.0.saturating_add(rhs))
    }
}

impl Add for TextSize {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        self + rhs.0
    }
}

impl AddAssign<u32> for TextSize {
    fn add_assign(&mut self, rhs: u32) {
        *self = *self + rhs;
    }
}

impl Sub for TextSize {
    type Output = u32;
    fn sub(self, rhs: Self) -> Self::Output {
        self.0.saturating_sub(rhs.0)
    }
}

impl SubAssign<u32> for TextSize {
    fn sub_assign(&mut self, rhs: u32) {
        self.0 = self.0.saturating_sub(rhs);
    }
}

/// A half-open UTF-8 byte range.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TextRange {
    start: TextSize,
    end: TextSize,
}

impl TextRange {
    /// Creates a range and clamps a reversed end to the start.
    pub const fn new(start: TextSize, end: TextSize) -> Self {
        if end.raw() < start.raw() {
            Self { start, end: start }
        } else {
            Self { start, end }
        }
    }
    /// Creates a range from platform-sized byte offsets.
    pub fn from_usize(start: usize, end: usize) -> Self {
        Self::new(TextSize::new(start as u32), TextSize::new(end as u32))
    }
    /// Creates an empty range at an offset.
    pub const fn empty(offset: TextSize) -> Self {
        Self { start: offset, end: offset }
    }
    /// Returns the inclusive start offset.
    pub const fn start(self) -> TextSize {
        self.start
    }
    /// Returns the exclusive end offset.
    pub const fn end(self) -> TextSize {
        self.end
    }
    /// Returns the range length in bytes.
    pub const fn len(self) -> u32 {
        self.end.raw() - self.start.raw()
    }
    /// Tests whether this range has no bytes.
    pub const fn is_empty(self) -> bool {
        self.start.raw() == self.end.raw()
    }
    /// Tests whether an offset is contained by this range.
    pub fn contains(self, offset: TextSize) -> bool {
        self.start <= offset && offset < self.end
    }
    /// Tests whether another range is fully contained by this range.
    pub fn contains_range(self, other: Self) -> bool {
        self.start <= other.start && other.end <= self.end
    }
}

impl fmt::Debug for TextRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("TextRange").field(&self.start).field(&self.end).finish()
    }
}

impl fmt::Display for TextRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

/// A one-based line and zero-based column.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct LineCol {
    /// One-based line number.
    pub line: u32,
    /// Zero-based column.
    pub column: u32,
}

/// Converts source offsets into line and column coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineIndex {
    line_starts: Vec<TextSize>,
    ascii_only: bool,
}

impl LineIndex {
    /// Builds an index recognizing LF, CRLF, and CR line endings.
    pub fn new(src: &str) -> Self {
        let mut starts = vec![TextSize::new(0)];
        let bytes = src.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'\n' => starts.push(TextSize::new((index + 1) as u32)),
                b'\r' => {
                    let next =
                        if bytes.get(index + 1) == Some(&b'\n') { index + 2 } else { index + 1 };
                    starts.push(TextSize::new(next as u32));
                    index = next;
                    continue;
                }
                _ => {}
            }
            index += 1;
        }
        Self { line_starts: starts, ascii_only: src.is_ascii() }
    }

    /// Returns the number of lines represented by the index.
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }
    /// Returns whether the source was ASCII-only.
    pub const fn is_ascii_only(&self) -> bool {
        self.ascii_only
    }

    fn line_start(&self, offset: TextSize) -> (usize, TextSize) {
        let position = self.line_starts.partition_point(|start| *start <= offset);
        let line = position.saturating_sub(1);
        (line, self.line_starts[line])
    }

    /// Converts an offset to CPython-compatible UTF-8 byte columns.
    pub fn line_col_utf8(&self, src: &str, offset: TextSize) -> LineCol {
        let offset = offset.as_usize().min(src.len());
        let (line, start) = self.line_start(TextSize::new(offset as u32));
        LineCol { line: line as u32 + 1, column: (offset - start.as_usize()) as u32 }
    }

    /// Converts an offset to UTF-16 code-unit columns.
    pub fn line_col_utf16(&self, src: &str, offset: TextSize) -> LineCol {
        let offset = offset.as_usize().min(src.len());
        let (line, start) = self.line_start(TextSize::new(offset as u32));
        LineCol {
            line: line as u32 + 1,
            column: src[start.as_usize()..offset].encode_utf16().count() as u32,
        }
    }

    /// Converts an offset to Unicode scalar-value columns.
    pub fn line_col_chars(&self, src: &str, offset: TextSize) -> LineCol {
        let offset = offset.as_usize().min(src.len());
        let (line, start) = self.line_start(TextSize::new(offset as u32));
        LineCol {
            line: line as u32 + 1,
            column: src[start.as_usize()..offset].chars().count() as u32,
        }
    }
}

/// A named source file and its line index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFile {
    name: String,
    text: String,
    index: LineIndex,
}

impl SourceFile {
    /// Creates a source file, rejecting files larger than `u32::MAX` bytes.
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Result<Self, SourceError> {
        let name = name.into();
        let text = text.into();
        if text.len() > u32::MAX as usize {
            return Err(SourceError::FileTooLarge { size: text.len() });
        }
        let index = LineIndex::new(&text);
        Ok(Self { name, text, index })
    }
    /// Returns the source name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Returns the source text.
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Returns the line index.
    pub const fn line_index(&self) -> &LineIndex {
        &self.index
    }

    /// Decodes UTF-8 source bytes, honoring an optional UTF-8 BOM.
    pub fn from_bytes(name: impl Into<String>, bytes: &[u8]) -> Result<Self, SourceError> {
        let encoding = detect_encoding(bytes)?;
        let payload = if matches!(encoding, SourceEncoding::Utf8Bom) { &bytes[3..] } else { bytes };
        let text = std::str::from_utf8(payload).map_err(|_| SourceError::InvalidUtf8)?.to_owned();
        Self::new(name, text)
    }
}

/// Encoding marker detected at a Python source boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceEncoding {
    /// Plain UTF-8.
    Utf8,
    /// UTF-8 preceded by a BOM.
    Utf8Bom,
    /// A declared non-UTF-8 codec name.
    Declared(Box<str>),
}

/// Detects a UTF-8 BOM or PEP 263 cookie in the first two lines.
pub fn detect_encoding(bytes: &[u8]) -> Result<SourceEncoding, SourceError> {
    let has_bom = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
    let scan = if has_bom { &bytes[3..] } else { bytes };
    for line in scan.split(|byte| *byte == b'\n' || *byte == b'\r').take(2) {
        if let Some(position) =
            line.windows(6).position(|window| window.eq_ignore_ascii_case(b"coding"))
        {
            let tail = &line[position + 6..];
            let tail = tail.strip_prefix(b":").or_else(|| tail.strip_prefix(b"="));
            if let Some(tail) = tail {
                let name = tail
                    .iter()
                    .skip_while(|byte| byte.is_ascii_whitespace())
                    .take_while(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                    })
                    .copied()
                    .collect::<Vec<_>>();
                if !name.is_empty() {
                    let name = String::from_utf8_lossy(&name).to_ascii_lowercase();
                    if has_bom && name != "utf-8" && name != "utf8" {
                        return Err(SourceError::EncodingProblem);
                    }
                    return Ok(if name == "utf-8" || name == "utf8" {
                        if has_bom {
                            SourceEncoding::Utf8Bom
                        } else {
                            SourceEncoding::Utf8
                        }
                    } else {
                        SourceEncoding::Declared(name.into())
                    });
                }
            }
        }
    }
    Ok(if has_bom { SourceEncoding::Utf8Bom } else { SourceEncoding::Utf8 })
}

/// Errors raised while constructing a source file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceError {
    /// The source cannot be represented by `TextSize`.
    FileTooLarge {
        /// Source length in bytes.
        size: usize,
    },
    /// The bytes are not valid UTF-8.
    InvalidUtf8,
    /// A BOM conflicts with a declared encoding.
    EncodingProblem,
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileTooLarge { size } => write!(f, "source file is too large: {size} bytes"),
            Self::InvalidUtf8 => f.write_str("source is not valid UTF-8"),
            Self::EncodingProblem => {
                f.write_str("source encoding declaration conflicts with UTF-8 BOM")
            }
        }
    }
}

impl std::error::Error for SourceError {}

impl Index<TextRange> for str {
    type Output = str;
    fn index(&self, range: TextRange) -> &Self::Output {
        &self[range.start.as_usize()..range.end.as_usize()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_index_handles_mixed_line_endings() {
        let src = "a\r\nb\rc\n";
        let index = LineIndex::new(src);
        assert_eq!(index.line_count(), 4);
        assert_eq!(index.line_col_utf8(src, TextSize::new(3)), LineCol { line: 2, column: 0 });
        let unicode = "あx\n";
        let unicode_index = LineIndex::new(unicode);
        assert_eq!(
            unicode_index.line_col_chars(unicode, TextSize::new(3)),
            LineCol { line: 1, column: 1 }
        );
    }

    #[test]
    fn ranges_are_half_open() {
        let range = TextRange::from_usize(2, 5);
        assert!(range.contains(TextSize::new(2)));
        assert!(!range.contains(TextSize::new(5)));
    }
}
