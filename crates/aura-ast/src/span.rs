//! Source positions and spans.

/// Byte offset into the source file (UTF-8).
pub type BytePos = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: BytePos,
    pub end: BytePos,
}

impl Span {
    pub fn new(start: BytePos, end: BytePos) -> Self {
        Self { start, end }
    }

    /// Shift both endpoints by `delta` (used when concatenating multi-file packages).
    pub fn shift(self, delta: BytePos) -> Self {
        Self {
            start: self.start.saturating_add(delta),
            end: self.end.saturating_add(delta),
        }
    }
}

