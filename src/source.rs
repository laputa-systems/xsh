use std::fmt;
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct SourceId(u32);

impl SourceId {
    pub const fn new(raw: usize) -> Self {
        assert!(raw <= u32::MAX as usize);
        Self(raw as u32)
    }

    pub const fn raw(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Span {
    pub source_id: SourceId,
    start: u32,
    len: u32,
}

impl Span {
    pub const fn new(source_id: SourceId, start: usize, end: usize) -> Self {
        assert!(start <= u32::MAX as usize);
        assert!(end >= start);
        assert!(end - start <= u32::MAX as usize);
        Self {
            source_id,
            start: start as u32,
            len: (end - start) as u32,
        }
    }

    pub const fn at(source_id: SourceId, start: usize) -> Self {
        assert!(start <= u32::MAX as usize);
        Self {
            source_id,
            start: start as u32,
            len: 0,
        }
    }

    pub const fn start(self) -> usize {
        self.start as usize
    }

    pub fn set_start(&mut self, start: usize) {
        let end = self.end();
        assert!(start <= end);
        assert!(start <= u32::MAX as usize);
        self.start = start as u32;
        self.len = (end - start) as u32;
    }

    pub fn set_end(&mut self, end: usize) {
        let start = self.start();
        assert!(end >= start);
        assert!(end - start <= u32::MAX as usize);
        self.len = (end - start) as u32;
    }

    pub fn shift_start(&mut self, offset: usize) {
        let start = self.start().saturating_add(offset);
        let end = self.end().saturating_add(offset);
        assert!(start <= u32::MAX as usize);
        assert!(end - start <= u32::MAX as usize);
        self.start = start as u32;
        self.len = (end - start) as u32;
    }

    pub const fn end(self) -> usize {
        self.start as usize + self.len as usize
    }

    pub fn range(self) -> Range<usize> {
        self.start()..self.end()
    }

    pub fn is_empty(self) -> bool {
        self.len == 0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceLocation {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub offset: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceLoadError {
    pub name: String,
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for SourceLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: {}",
            self.name,
            self.offset.saturating_add(1),
            self.message
        )
    }
}

impl std::error::Error for SourceLoadError {}

#[derive(Clone, Debug)]
pub struct SourceFile {
    id: SourceId,
    name: String,
    text: String,
    line_starts: Vec<usize>,
}

impl SourceFile {
    pub fn from_utf8(
        id: SourceId,
        name: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Result<Self, SourceLoadError> {
        let name = name.into();
        let text = String::from_utf8(bytes).map_err(|err| SourceLoadError {
            name: name.clone(),
            offset: err.utf8_error().valid_up_to(),
            message: "source file is not valid UTF-8".to_string(),
        })?;
        Ok(Self::new(id, name, text))
    }

    pub fn new(id: SourceId, name: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        let mut line_starts = vec![0];
        for (offset, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(offset + 1);
            }
        }

        Self {
            id,
            name: name.into(),
            text,
            line_starts,
        }
    }

    pub fn id(&self) -> SourceId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn retained_bytes(&self) -> usize {
        self.name.capacity()
            + self.text.capacity()
            + self.line_starts.capacity() * std::mem::size_of::<usize>()
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    pub fn line_text(&self, one_based_line: usize) -> Option<&str> {
        let index = one_based_line.checked_sub(1)?;
        let start = *self.line_starts.get(index)?;
        let end = self
            .line_starts
            .get(index + 1)
            .copied()
            .unwrap_or(self.text.len());
        Some(self.text[start..end].trim_end_matches(['\r', '\n']))
    }

    pub fn location(&self, offset: usize) -> Option<SourceLocation> {
        if offset > self.text.len() || !self.text.is_char_boundary(offset) {
            return None;
        }

        let line_index = match self.line_starts.binary_search(&offset) {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        };
        let line_start = self.line_starts[line_index];
        let column = self.text[line_start..offset].chars().count() + 1;

        Some(SourceLocation {
            file: self.name.clone(),
            line: line_index + 1,
            column,
            offset,
        })
    }

    pub fn inferred_span_end(&self, span: Span) -> Option<usize> {
        if span.source_id != self.id {
            return None;
        }
        Some(span.end())
    }

    pub fn span_text(&self, span: Span) -> Option<&str> {
        if span.source_id != self.id {
            return None;
        }
        let end = self.inferred_span_end(span)?;
        let start = span.start();
        if start > end
            || end > self.text.len()
            || !self.text.is_char_boundary(start)
            || !self.text.is_char_boundary(end)
        {
            return None;
        }
        Some(&self.text[start..end])
    }
}

fn resolve_source_path(name: String) -> String {
    if !name.contains(std::path::MAIN_SEPARATOR) {
        return name;
    }
    std::path::absolute(&name)
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or(name)
}

#[derive(Clone, Debug, Default)]
pub struct SourceMap {
    files: Arc<Vec<SourceFile>>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_file(&mut self, name: impl Into<String>, text: impl Into<String>) -> SourceId {
        let id = SourceId::new(self.files.len());
        let name = name.into();
        let resolved = resolve_source_path(name);
        Arc::make_mut(&mut self.files).push(SourceFile::new(id, resolved, text));
        id
    }

    pub fn add_file_from_utf8(
        &mut self,
        name: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Result<SourceId, SourceLoadError> {
        let id = SourceId::new(self.files.len());
        let name = name.into();
        let resolved = resolve_source_path(name);
        let file = SourceFile::from_utf8(id, resolved, bytes)?;
        Arc::make_mut(&mut self.files).push(file);
        Ok(id)
    }

    pub fn get(&self, id: SourceId) -> Option<&SourceFile> {
        self.files.get(id.raw())
    }

    pub fn location(&self, source_id: SourceId, offset: usize) -> Option<SourceLocation> {
        self.get(source_id)?.location(offset)
    }

    pub fn inferred_span_end(&self, span: Span) -> Option<usize> {
        self.get(span.source_id)?.inferred_span_end(span)
    }

    pub fn span_text(&self, span: Span) -> Option<&str> {
        self.get(span.source_id)?.span_text(span)
    }

    pub fn files(&self) -> &[SourceFile] {
        self.files.as_slice()
    }

    pub fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Vec<SourceFile>>()
            + 2 * std::mem::size_of::<usize>()
            + self.files.capacity() * std::mem::size_of::<SourceFile>()
            + self
                .files
                .iter()
                .map(SourceFile::retained_bytes)
                .sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::SourceMap;

    #[test]
    fn reports_one_based_line_and_unicode_column() {
        let mut sources = SourceMap::new();
        let id = sources.add_file("sample.xsh", "one\nåβ = 1\n");
        let file = sources.get(id).unwrap();
        let offset = file.text().find('=').unwrap();

        let location = file.location(offset).unwrap();

        assert_eq!(location.line, 2);
        assert_eq!(location.column, 4);
        assert_eq!(file.line_text(2), Some("åβ = 1"));
    }

    #[test]
    fn keeps_crlf_line_text_without_line_ending() {
        let mut sources = SourceMap::new();
        let id = sources.add_file("sample.xsh", "one\r\ntwo\r\n");
        let file = sources.get(id).unwrap();

        assert_eq!(file.line_count(), 3);
        assert_eq!(file.line_text(1), Some("one"));
        assert_eq!(file.line_text(2), Some("two"));
    }

    #[test]
    fn rejects_invalid_utf8_source() {
        let mut sources = SourceMap::new();
        let err = sources
            .add_file_from_utf8("bad.xsh", vec![b'o', b'k', 0xff])
            .unwrap_err();

        assert_eq!(err.name, "bad.xsh");
        assert_eq!(err.offset, 2);
    }

    #[test]
    fn cloned_source_maps_are_independent_when_mutated() {
        let mut original = SourceMap::new();
        original.add_file("original.xsh", "let value = 1\n");
        let mut clone = original.clone();
        clone.add_file("clone.xsh", "let value = 2\n");

        assert_eq!(original.files().len(), 1);
        assert_eq!(clone.files().len(), 2);
    }
}
