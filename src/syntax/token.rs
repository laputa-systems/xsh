use crate::source::{SourceId, Span};
use crate::symbol::{Name, Symbol};
use crate::syntax::literal::{self, QuotedScan};
use std::num::NonZeroU32;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct TokenId(NonZeroU32);

impl TokenId {
    pub fn new(index: usize) -> Self {
        let raw = u32::try_from(index + 1).expect("token table exceeded u32 token ids");
        Self(NonZeroU32::new(raw).expect("token ids are one-based"))
    }

    pub fn index(self) -> usize {
        self.0.get() as usize - 1
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum TokenTag {
    Ident,
    ProcIdent,
    Keyword,
    Int,
    Float,
    Duration,
    String,
    PathString,
    GlobString,
    FmtString,
    PathFmtString,
    Bytes,
    Comment,
    Newline,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Semicolon,
    Dot,
    At,
    Question,
    QuestionQuestion,
    LastStatus,
    DollarLBrace,
    DollarIdent,
    Arrow,
    FatArrow,
    Equals,
    EqEq,
    Bang,
    BangEq,
    Lt,
    Le,
    Gt,
    Ge,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Pipe,
    PipeGt,
    Amp,
    GtGt,
    ErrorGt,
    ErrorGtGt,
    Eof,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct TokenPayload(pub u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TokenPayloadEntry {
    index: u32,
    payload: TokenPayload,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TokenStarts {
    U16(Vec<u16>),
    U32(Vec<u32>),
}

impl Default for TokenStarts {
    fn default() -> Self {
        Self::U16(Vec::new())
    }
}

impl TokenStarts {
    fn with_capacity(capacity: usize) -> Self {
        Self::U16(Vec::with_capacity(capacity))
    }

    fn push(&mut self, start: usize) {
        let start = u32::try_from(start).expect("token start exceeded u32 byte offset");
        match self {
            Self::U16(starts) if u16::try_from(start).is_ok() => {
                starts.push(start as u16);
            }
            Self::U16(starts) => {
                let mut promoted = Vec::with_capacity(starts.capacity().max(starts.len() + 1));
                promoted.extend(starts.iter().map(|start| *start as u32));
                promoted.push(start);
                *self = Self::U32(promoted);
            }
            Self::U32(starts) => starts.push(start),
        }
    }

    fn get(&self, index: usize) -> Option<usize> {
        match self {
            Self::U16(starts) => starts.get(index).map(|start| *start as usize),
            Self::U32(starts) => starts.get(index).map(|start| *start as usize),
        }
    }

    fn shrink_to_fit(&mut self) {
        match self {
            Self::U16(starts) => starts.shrink_to_fit(),
            Self::U32(starts) => starts.shrink_to_fit(),
        }
    }

    fn retained_bytes(&self) -> usize {
        match self {
            Self::U16(starts) => starts.capacity() * std::mem::size_of::<u16>(),
            Self::U32(starts) => starts.capacity() * std::mem::size_of::<u32>(),
        }
    }

    fn row_bytes(&self) -> usize {
        match self {
            Self::U16(starts) => starts.len() * std::mem::size_of::<u16>(),
            Self::U32(starts) => starts.len() * std::mem::size_of::<u32>(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TokenTable {
    data: Arc<TokenTableData>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TokenTableData {
    tags: Vec<TokenTag>,
    starts: TokenStarts,
    payloads: Vec<TokenPayloadEntry>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TokenTableBuilder {
    tags: Vec<TokenTag>,
    starts: TokenStarts,
    payloads: Vec<TokenPayloadEntry>,
}

impl TokenTableBuilder {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            tags: Vec::with_capacity(capacity),
            starts: TokenStarts::with_capacity(capacity),
            // Idents and keywords dominate payload-bearing tokens; half the token
            // capacity avoids most of the growth reallocations without
            // over-committing for punctuation-heavy sources.
            payloads: Vec::with_capacity(capacity / 2),
        }
    }

    pub fn push_kind(&mut self, kind: &TokenKind, start: usize) -> TokenId {
        let index = self.tags.len();
        let id = TokenId::new(index);
        self.tags.push(kind.tag());
        self.starts.push(start);
        let payload = kind.compact_payload();
        if payload.0 != 0 {
            self.payloads.push(TokenPayloadEntry {
                index: u32::try_from(index).expect("token table exceeded u32 token ids"),
                payload,
            });
        }
        id
    }

    pub fn finish(mut self) -> TokenTable {
        self.tags.shrink_to_fit();
        self.starts.shrink_to_fit();
        self.payloads.shrink_to_fit();
        TokenTable {
            data: Arc::new(TokenTableData {
                tags: self.tags,
                starts: self.starts,
                payloads: self.payloads,
            }),
        }
    }
}

impl TokenTable {
    pub fn len(&self) -> usize {
        self.data.tags.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.tags.is_empty()
    }

    pub fn tag(&self, id: TokenId) -> TokenTag {
        self.data.tags[id.index()]
    }

    pub fn tag_at(&self, index: usize) -> Option<TokenTag> {
        self.data.tags.get(index).copied()
    }

    pub fn start(&self, id: TokenId) -> usize {
        self.start_at(id.index())
            .expect("token id belongs to this token table")
    }

    pub fn start_at(&self, index: usize) -> Option<usize> {
        self.data.starts.get(index)
    }

    pub fn end(&self, id: TokenId, source: &str) -> usize {
        self.end_at(id.index(), source)
            .expect("token id belongs to this token table")
    }

    pub fn end_at(&self, index: usize, source: &str) -> Option<usize> {
        let tag = self.tag_at(index)?;
        let start = self.start_at(index)?;
        Some(token_end(source, start, tag))
    }

    pub fn span(&self, id: TokenId, source_id: SourceId, source: &str) -> Span {
        let start = self.start(id);
        Span::new(source_id, start, self.end(id, source))
    }

    pub fn span_at(&self, index: usize, source_id: SourceId, source: &str) -> Option<Span> {
        let start = self.start_at(index)?;
        let end = self.end_at(index, source)?;
        Some(Span::new(source_id, start, end))
    }

    pub fn payload(&self, id: TokenId) -> TokenPayload {
        self.payload_at(id.index())
            .expect("token id belongs to this token table")
    }

    pub fn payload_at(&self, index: usize) -> Option<TokenPayload> {
        if index >= self.len() {
            return None;
        }
        let index = u32::try_from(index).expect("token table exceeded u32 token ids");
        match self
            .data
            .payloads
            .binary_search_by_key(&index, |entry| entry.index)
        {
            Ok(payload_index) => Some(self.data.payloads[payload_index].payload),
            Err(_) => Some(TokenPayload::default()),
        }
    }

    pub fn name(&self, id: TokenId) -> Option<Name> {
        self.name_at(id.index())
    }

    pub fn name_at(&self, index: usize) -> Option<Name> {
        match self.tag_at(index)? {
            TokenTag::Ident | TokenTag::ProcIdent | TokenTag::DollarIdent => self
                .payload_at(index)
                .map(|payload| Name::from_symbol(Symbol::from_raw(payload.0))),
            _ => None,
        }
    }

    pub fn keyword(&self, id: TokenId) -> Option<Keyword> {
        self.keyword_at(id.index())
    }

    pub fn keyword_at(&self, index: usize) -> Option<Keyword> {
        if self.tag_at(index)? != TokenTag::Keyword {
            return None;
        }
        Keyword::from_payload(self.payload_at(index)?.0)
    }

    pub fn string_flags(&self, id: TokenId) -> Option<StringTokenFlags> {
        self.string_flags_at(id.index())
    }

    pub fn string_flags_at(&self, index: usize) -> Option<StringTokenFlags> {
        match self.tag_at(index)? {
            TokenTag::String | TokenTag::FmtString => {
                Some(StringTokenFlags::from_payload(self.payload_at(index)?.0))
            }
            _ => None,
        }
    }

    pub fn retained_bytes(&self) -> usize {
        std::mem::size_of::<TokenTableData>()
            + 2 * std::mem::size_of::<usize>()
            + self.data.tags.capacity() * std::mem::size_of::<TokenTag>()
            + self.data.starts.retained_bytes()
            + self.data.payloads.capacity() * std::mem::size_of::<TokenPayloadEntry>()
    }

    pub fn row_bytes(&self) -> usize {
        self.data.tags.len() * std::mem::size_of::<TokenTag>()
            + self.data.starts.row_bytes()
            + self.data.payloads.len() * std::mem::size_of::<TokenPayloadEntry>()
    }
}

fn token_end(source: &str, start: usize, tag: TokenTag) -> usize {
    let bytes = source.as_bytes();
    match tag {
        TokenTag::Eof => start,
        TokenTag::Newline => {
            if bytes.get(start..start + 2) == Some(b"\r\n") {
                start + 2
            } else {
                start + 1
            }
        }
        TokenTag::Comment => scan_until(source, start + 1, |byte| !matches!(byte, b'\r' | b'\n')),
        TokenTag::Ident | TokenTag::ProcIdent | TokenTag::Keyword => {
            let offset = start.saturating_add(1);
            scan_until(source, offset, |byte| {
                is_ident_continue(byte) || byte == b'-'
            })
        }
        TokenTag::DollarIdent => {
            let offset = start.saturating_add(2);
            scan_until(source, offset, is_ident_continue)
        }
        TokenTag::Int | TokenTag::Float | TokenTag::Duration => scan_number_end(source, start),
        TokenTag::String
        | TokenTag::PathString
        | TokenTag::GlobString
        | TokenTag::FmtString
        | TokenTag::PathFmtString
        | TokenTag::Bytes => match literal::scan_quoted_literal(source, start, true) {
            Some(QuotedScan::Terminated(literal)) => literal.end,
            Some(QuotedScan::Unterminated { end }) => end,
            None => start,
        },
        _ => start + fixed_width(tag),
    }
}

fn fixed_width(tag: TokenTag) -> usize {
    match tag {
        TokenTag::QuestionQuestion
        | TokenTag::LastStatus
        | TokenTag::DollarLBrace
        | TokenTag::Arrow
        | TokenTag::FatArrow
        | TokenTag::EqEq
        | TokenTag::BangEq
        | TokenTag::Le
        | TokenTag::Ge
        | TokenTag::PipeGt
        | TokenTag::GtGt
        | TokenTag::ErrorGt => 2,
        TokenTag::ErrorGtGt => 3,
        _ => 1,
    }
}

fn scan_number_end(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut offset = start;
    if bytes.get(offset) == Some(&b'0') && bytes.get(offset + 1) == Some(&b'o') {
        offset += 2;
        while matches!(bytes.get(offset), Some(b'0'..=b'7')) {
            offset += 1;
        }
        if matches!(bytes.get(offset), Some(byte) if byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            offset += 1;
            while matches!(bytes.get(offset), Some(byte) if byte.is_ascii_alphanumeric() || *byte == b'_')
            {
                offset += 1;
            }
        }
        return offset;
    }

    while matches!(bytes.get(offset), Some(byte) if byte.is_ascii_digit()) {
        offset += 1;
    }
    if bytes.get(offset) == Some(&b'.')
        && matches!(bytes.get(offset + 1), Some(byte) if byte.is_ascii_digit())
    {
        offset += 1;
        while matches!(bytes.get(offset), Some(byte) if byte.is_ascii_digit()) {
            offset += 1;
        }
    }
    if matches!(bytes.get(offset), Some(b'e' | b'E')) {
        offset += 1;
        if matches!(bytes.get(offset), Some(b'+' | b'-')) {
            offset += 1;
        }
        while matches!(bytes.get(offset), Some(byte) if byte.is_ascii_digit()) {
            offset += 1;
        }
    }
    if bytes.get(offset..offset + 2) == Some(b"ms") {
        offset + 2
    } else if matches!(bytes.get(offset), Some(b's' | b'm' | b'h')) {
        offset + 1
    } else {
        offset
    }
}

fn scan_until(source: &str, start: usize, keep_going: impl Fn(u8) -> bool) -> usize {
    let mut offset = start;
    let bytes = source.as_bytes();
    while bytes.get(offset).is_some_and(|byte| keep_going(*byte)) {
        offset += 1;
    }
    offset
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StringTokenFlags {
    pub has_interpolation: bool,
    pub raw_literal: bool,
}

impl StringTokenFlags {
    const HAS_INTERPOLATION: u32 = 1 << 0;
    const RAW_LITERAL: u32 = 1 << 1;

    pub const fn from_payload(payload: u32) -> Self {
        Self {
            has_interpolation: payload & Self::HAS_INTERPOLATION != 0,
            raw_literal: payload & Self::RAW_LITERAL != 0,
        }
    }

    const fn to_payload(self) -> u32 {
        (if self.has_interpolation {
            Self::HAS_INTERPOLATION
        } else {
            0
        }) | (if self.raw_literal {
            Self::RAW_LITERAL
        } else {
            0
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TokenKind {
    Ident(Name),
    ProcIdent(Name),
    Keyword(Keyword),
    Int,
    Float,
    Duration,
    String {
        has_interpolation: bool,
        raw_literal: bool,
    },
    PathString,
    GlobString,
    FmtString {
        raw_literal: bool,
    },
    PathFmtString,
    Bytes,
    Comment,
    Newline,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Semicolon,
    Dot,
    At,
    Question,
    QuestionQuestion,
    LastStatus,
    DollarLBrace,
    DollarIdent(Name),
    Arrow,
    FatArrow,
    Equals,
    EqEq,
    Bang,
    BangEq,
    Lt,
    Le,
    Gt,
    Ge,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Pipe,
    PipeGt,
    Amp,
    GtGt,
    ErrorGt,
    ErrorGtGt,
    Eof,
}

impl TokenKind {
    pub const fn tag(&self) -> TokenTag {
        match self {
            Self::Ident(_) => TokenTag::Ident,
            Self::ProcIdent(_) => TokenTag::ProcIdent,
            Self::Keyword(_) => TokenTag::Keyword,
            Self::Int => TokenTag::Int,
            Self::Float => TokenTag::Float,
            Self::Duration => TokenTag::Duration,
            Self::String { .. } => TokenTag::String,
            Self::PathString => TokenTag::PathString,
            Self::GlobString => TokenTag::GlobString,
            Self::FmtString { .. } => TokenTag::FmtString,
            Self::PathFmtString => TokenTag::PathFmtString,
            Self::Bytes => TokenTag::Bytes,
            Self::Comment => TokenTag::Comment,
            Self::Newline => TokenTag::Newline,
            Self::LParen => TokenTag::LParen,
            Self::RParen => TokenTag::RParen,
            Self::LBrace => TokenTag::LBrace,
            Self::RBrace => TokenTag::RBrace,
            Self::LBracket => TokenTag::LBracket,
            Self::RBracket => TokenTag::RBracket,
            Self::Comma => TokenTag::Comma,
            Self::Colon => TokenTag::Colon,
            Self::Semicolon => TokenTag::Semicolon,
            Self::Dot => TokenTag::Dot,
            Self::At => TokenTag::At,
            Self::Question => TokenTag::Question,
            Self::QuestionQuestion => TokenTag::QuestionQuestion,
            Self::LastStatus => TokenTag::LastStatus,
            Self::DollarLBrace => TokenTag::DollarLBrace,
            Self::DollarIdent(_) => TokenTag::DollarIdent,
            Self::Arrow => TokenTag::Arrow,
            Self::FatArrow => TokenTag::FatArrow,
            Self::Equals => TokenTag::Equals,
            Self::EqEq => TokenTag::EqEq,
            Self::Bang => TokenTag::Bang,
            Self::BangEq => TokenTag::BangEq,
            Self::Lt => TokenTag::Lt,
            Self::Le => TokenTag::Le,
            Self::Gt => TokenTag::Gt,
            Self::Ge => TokenTag::Ge,
            Self::Plus => TokenTag::Plus,
            Self::Minus => TokenTag::Minus,
            Self::Star => TokenTag::Star,
            Self::Slash => TokenTag::Slash,
            Self::Percent => TokenTag::Percent,
            Self::Pipe => TokenTag::Pipe,
            Self::PipeGt => TokenTag::PipeGt,
            Self::Amp => TokenTag::Amp,
            Self::GtGt => TokenTag::GtGt,
            Self::ErrorGt => TokenTag::ErrorGt,
            Self::ErrorGtGt => TokenTag::ErrorGtGt,
            Self::Eof => TokenTag::Eof,
        }
    }

    pub fn compact_payload(&self) -> TokenPayload {
        let payload = match self {
            Self::Ident(name) | Self::ProcIdent(name) | Self::DollarIdent(name) => {
                name.symbol().raw()
            }
            Self::Keyword(keyword) => *keyword as u32,
            Self::String {
                has_interpolation,
                raw_literal,
            } => StringTokenFlags {
                has_interpolation: *has_interpolation,
                raw_literal: *raw_literal,
            }
            .to_payload(),
            Self::FmtString { raw_literal } => StringTokenFlags {
                has_interpolation: true,
                raw_literal: *raw_literal,
            }
            .to_payload(),
            _ => 0,
        };
        TokenPayload(payload)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum Keyword {
    And,
    Break,
    Continue,
    Defer,
    Else,
    Export,
    False,
    For,
    Guard,
    If,
    In,
    Let,
    Loop,
    Match,
    Not,
    Null,
    Or,
    Proc,
    Pure,
    Retry,
    Return,
    Run,
    Spawn,
    Stream,
    True,
    Type,
    Unless,
    Use,
    Var,
    Wait,
    When,
    While,
    With,
    Yield,
}

impl Keyword {
    pub const fn from_payload(payload: u32) -> Option<Self> {
        Some(match payload {
            value if value == Self::And as u32 => Self::And,
            value if value == Self::Break as u32 => Self::Break,
            value if value == Self::Continue as u32 => Self::Continue,
            value if value == Self::Defer as u32 => Self::Defer,
            value if value == Self::Else as u32 => Self::Else,
            value if value == Self::Export as u32 => Self::Export,
            value if value == Self::False as u32 => Self::False,
            value if value == Self::For as u32 => Self::For,
            value if value == Self::Guard as u32 => Self::Guard,
            value if value == Self::If as u32 => Self::If,
            value if value == Self::In as u32 => Self::In,
            value if value == Self::Let as u32 => Self::Let,
            value if value == Self::Loop as u32 => Self::Loop,
            value if value == Self::Match as u32 => Self::Match,
            value if value == Self::Not as u32 => Self::Not,
            value if value == Self::Null as u32 => Self::Null,
            value if value == Self::Or as u32 => Self::Or,
            value if value == Self::Proc as u32 => Self::Proc,
            value if value == Self::Pure as u32 => Self::Pure,
            value if value == Self::Retry as u32 => Self::Retry,
            value if value == Self::Return as u32 => Self::Return,
            value if value == Self::Run as u32 => Self::Run,
            value if value == Self::Spawn as u32 => Self::Spawn,
            value if value == Self::Stream as u32 => Self::Stream,
            value if value == Self::True as u32 => Self::True,
            value if value == Self::Type as u32 => Self::Type,
            value if value == Self::Unless as u32 => Self::Unless,
            value if value == Self::Use as u32 => Self::Use,
            value if value == Self::Var as u32 => Self::Var,
            value if value == Self::Wait as u32 => Self::Wait,
            value if value == Self::When as u32 => Self::When,
            value if value == Self::While as u32 => Self::While,
            value if value == Self::With as u32 => Self::With,
            value if value == Self::Yield as u32 => Self::Yield,
            _ => return None,
        })
    }

    pub fn from_ident(ident: &str) -> Option<Self> {
        Some(match ident {
            "and" => Self::And,
            "break" => Self::Break,
            "continue" => Self::Continue,
            "defer" => Self::Defer,
            "else" => Self::Else,
            "export" => Self::Export,
            "false" => Self::False,
            "for" => Self::For,
            "guard" => Self::Guard,
            "if" => Self::If,
            "in" => Self::In,
            "let" => Self::Let,
            "loop" => Self::Loop,
            "match" => Self::Match,
            "not" => Self::Not,
            "null" => Self::Null,
            "or" => Self::Or,
            "proc" => Self::Proc,
            "pure" => Self::Pure,
            "retry" => Self::Retry,
            "return" => Self::Return,
            "run" => Self::Run,
            "spawn" => Self::Spawn,
            "stream" => Self::Stream,
            "true" => Self::True,
            "type" => Self::Type,
            "unless" => Self::Unless,
            "use" => Self::Use,
            "var" => Self::Var,
            "wait" => Self::Wait,
            "when" => Self::When,
            "while" => Self::While,
            "with" => Self::With,
            "yield" => Self::Yield,
            _ => return None,
        })
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::And => "and",
            Self::Break => "break",
            Self::Continue => "continue",
            Self::Defer => "defer",
            Self::Else => "else",
            Self::Export => "export",
            Self::False => "false",
            Self::For => "for",
            Self::Guard => "guard",
            Self::If => "if",
            Self::In => "in",
            Self::Let => "let",
            Self::Loop => "loop",
            Self::Match => "match",
            Self::Not => "not",
            Self::Null => "null",
            Self::Or => "or",
            Self::Proc => "proc",
            Self::Pure => "pure",
            Self::Retry => "retry",
            Self::Return => "return",
            Self::Run => "run",
            Self::Spawn => "spawn",
            Self::Stream => "stream",
            Self::True => "true",
            Self::Type => "type",
            Self::Unless => "unless",
            Self::Use => "use",
            Self::Var => "var",
            Self::Wait => "wait",
            Self::When => "when",
            Self::While => "while",
            Self::With => "with",
            Self::Yield => "yield",
        }
    }
}
