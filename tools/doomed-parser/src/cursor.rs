//! Shared fscanf-like text cursor for the DoomEd ASCII parsers.
//!
//! Every primitive skips leading whitespace, then matches strictly, tracking the
//! 1-based text line for error reporting. The cursor is parameterised by the
//! format's error type `E` (via [`CursorError`]), so each parser fixes `E` once
//! at `new` and every method builds that error directly — no per-call `map_err`.

use std::marker::PhantomData;

use geom_kernel::Name8;

/// Longest prefix of remaining input included in error context.
const ERROR_CONTEXT_CHARS: usize = 24;

/// How a parser's error type is built from a cursor failure. The cursor stays
/// format-neutral; each format maps these to its own variants.
pub trait CursorError {
    /// Input ended where a token was required.
    fn unexpected_eof(line: usize) -> Self;
    /// A token did not match: `expected` names what was wanted, `found` is a
    /// short prefix of the offending input (a format may ignore it).
    fn bad_token(line: usize, expected: &'static str, found: String) -> Self;
    /// A name token was not a valid 8-char name.
    fn bad_name(line: usize, name: String) -> Self;
}

/// fscanf-like cursor over text; the 1-based `line` is public for error context
/// the parsers build directly (header lines, trailing-data line).
pub struct Cursor<'a, E> {
    rest: &'a str,
    pub line: usize,
    _error: PhantomData<E>,
}

impl<'a, E: CursorError> Cursor<'a, E> {
    pub fn new(text: &'a str) -> Self {
        Self {
            rest: text,
            line: 1,
            _error: PhantomData,
        }
    }

    pub fn skip_ws(&mut self) {
        let bytes = self.rest.as_bytes();
        let mut i = 0;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            if bytes[i] == b'\n' {
                self.line += 1;
            }
            i += 1;
        }
        self.rest = &self.rest[i..];
    }

    /// A short prefix of the remaining input, for error context.
    pub fn found(&self) -> String {
        self.rest.chars().take(ERROR_CONTEXT_CHARS).collect()
    }

    /// Bytes of input still unparsed; caps pre-allocations against a hostile
    /// header count.
    pub fn remaining(&self) -> usize {
        self.rest.len()
    }

    pub fn at_eof(&mut self) -> bool {
        self.skip_ws();
        self.rest.is_empty()
    }

    pub fn lit(&mut self, tok: &'static str) -> Result<(), E> {
        if self.at_eof() {
            return Err(E::unexpected_eof(self.line));
        }
        match self.rest.strip_prefix(tok) {
            Some(r) => {
                self.rest = r;
                Ok(())
            }
            None => Err(E::bad_token(self.line, tok, self.found())),
        }
    }

    /// A non-whitespace token (fscanf `%s` semantics).
    pub fn token(&mut self) -> Result<&'a str, E> {
        if self.at_eof() {
            return Err(E::unexpected_eof(self.line));
        }
        let end = self
            .rest
            .find(|c: char| c.is_ascii_whitespace())
            .unwrap_or(self.rest.len());
        let tok = &self.rest[..end];
        self.rest = &self.rest[end..];
        Ok(tok)
    }

    pub fn int(&mut self) -> Result<i32, E> {
        if self.at_eof() {
            return Err(E::unexpected_eof(self.line));
        }
        let bytes = self.rest.as_bytes();
        let mut end = usize::from(bytes[0] == b'-');
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        let tok = &self.rest[..end];
        let value = tok
            .parse::<i32>()
            .map_err(|_| E::bad_token(self.line, "integer", self.found()))?;
        self.rest = &self.rest[end..];
        Ok(value)
    }

    pub fn float(&mut self) -> Result<f32, E> {
        if self.at_eof() {
            return Err(E::unexpected_eof(self.line));
        }
        let end = self
            .rest
            .find(|c: char| !matches!(c, '0'..='9' | '-' | '+' | '.' | 'e' | 'E'))
            .unwrap_or(self.rest.len());
        let tok = &self.rest[..end];
        let value = tok
            .parse::<f32>()
            .map_err(|_| E::bad_token(self.line, "number", self.found()))?;
        self.rest = &self.rest[end..];
        Ok(value)
    }

    /// A texture/flat name token (fscanf `%8s`, strict on length).
    pub fn name8(&mut self) -> Result<Name8, E> {
        self.skip_ws();
        let line = self.line;
        let tok = self.token()?;
        Name8::from_dwd_field(tok).map_err(|_| E::bad_name(line, tok.to_owned()))
    }

    /// A `%31s`-style description token, capped at `max` bytes.
    pub fn desc(&mut self, max: usize) -> Result<String, E> {
        self.skip_ws();
        let line = self.line;
        let tok = self.token()?;
        if tok.len() > max {
            return Err(E::bad_token(
                line,
                "name within the length limit",
                self.found(),
            ));
        }
        Ok(tok.to_owned())
    }

    /// Assert the input is exhausted (trailing whitespace allowed).
    pub fn end(&mut self) -> Result<(), E> {
        if self.at_eof() {
            Ok(())
        } else {
            Err(E::bad_token(self.line, "end of file", self.found()))
        }
    }
}
