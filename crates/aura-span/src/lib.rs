#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(u32);

impl FileId {
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// A byte offset from the start of a source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BytePos(u32);

impl BytePos {
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// A half-open byte span `[start, end)` within a single file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: BytePos,
    pub end: BytePos,
}

impl Span {
    pub const fn new(start: BytePos, end: BytePos) -> Self {
        debug_assert!(start.raw() <= end.raw());
        Self { start, end }
    }

    pub const fn empty(at: BytePos) -> Self {
        Self::new(at, at)
    }

    pub const fn len(self) -> u32 {
        self.end.raw().saturating_sub(self.start.raw())
    }

    pub const fn is_empty(self) -> bool {
        self.start.raw() == self.end.raw()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileSpan {
    pub file_id: FileId,
    pub span: Span,
}

impl FileSpan {
    pub const fn new(file_id: FileId, span: Span) -> Self {
        Self { file_id, span }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_is_half_open() {
        let span = Span::new(BytePos::new(3), BytePos::new(8));
        assert_eq!(span.len(), 5);
        assert!(!span.is_empty());
        assert_eq!(Span::empty(BytePos::new(10)).len(), 0);
    }
}
