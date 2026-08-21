use std::{fmt, ops::Deref};

#[derive(thiserror::Error, Debug)]
pub enum CursorError {
    #[error("unterminated quote!")]
    UnterminatedQuote,

    #[error("expected whitespace!")]
    ExpectedWhitespace,
}

pub struct Cursor<'a> {
    input: &'a str,
    pos: usize,
    finished: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Token<'a> {
    inner: &'a str,
    quoted: bool,
}

impl<'a> Token<'a> {
    pub fn as_str(&self) -> &'a str {
        self.inner
    }

    pub fn is_quoted(&self) -> bool {
        self.quoted
    }
}

impl Deref for Token<'_> {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl From<Token<'_>> for String {
    fn from(value: Token<'_>) -> Self {
        value.inner.to_owned()
    }
}

impl PartialEq<&str> for Token<'_> {
    fn eq(&self, other: &&str) -> bool {
        self.inner == *other
    }
}

impl fmt::Display for Token<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.inner)
    }
}

impl<'a> Cursor<'a> {
    pub fn new(str: &'a str) -> Self {
        Self {
            input: str,
            pos: 0,
            finished: false,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn consume(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(ch) if ch.is_ascii_whitespace()) {
            self.consume();
        }
    }

    pub fn next_token(&mut self) -> Result<Option<Token<'a>>, CursorError> {
        self.skip_whitespace();
        match self.peek() {
            None => Ok(None),
            Some('\'') => self.read_quoted_token().map(Some),
            Some(_) => Ok(Some(self.read_unquoted_token())),
        }
    }

    fn read_quoted_token(&mut self) -> Result<Token<'a>, CursorError> {
        // skip the opening quote
        self.consume();

        let start = self.pos;
        loop {
            match self.peek() {
                Some('\'') => {
                    let end = self.pos;
                    self.consume();

                    if matches!(
                        self.peek(),
                        Some(ch) if !ch.is_ascii_whitespace()
                    ) {
                        return Err(CursorError::ExpectedWhitespace);
                    }

                    return Ok(Token {
                        inner: &self.input[start..end],
                        quoted: true,
                    });
                }

                Some(_) => {
                    self.consume();
                }

                None => {
                    return Err(CursorError::UnterminatedQuote);
                }
            }
        }
    }

    fn read_unquoted_token(&mut self) -> Token<'a> {
        let start = self.pos;
        while matches!(self.peek(), Some(ch) if !ch.is_ascii_whitespace()) {
            self.consume();
        }
        Token {
            inner: &self.input[start..self.pos],
            quoted: false,
        }
    }
}

impl<'a> Iterator for Cursor<'a> {
    type Item = Result<Token<'a>, CursorError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        match self.next_token() {
            Ok(Some(token)) => Some(Ok(token)),
            Ok(None) => {
                self.finished = true;
                None
            }
            Err(err) => {
                self.finished = true;
                Some(Err(err))
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::assert_eq;

    use crate::parser::cursor::Cursor;

    #[test]
    fn cursor_next_token_works() {
        let raw = "\t\n Hello World 'Hello World'";
        let cursor = Cursor::new(raw);
        assert_eq!(
            cursor.map(|x| x.unwrap()).collect::<Vec<_>>(),
            vec!["Hello", "World", "Hello World"]
        );
    }
}
