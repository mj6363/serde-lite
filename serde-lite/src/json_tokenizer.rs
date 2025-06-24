//! JSON tokenizer for streaming deserialization.

use std::{
    io::{BufReader, Read},
    str::FromStr,
};

use crate::{
    streaming::{StreamError, Token, Tokenizer},
    Number,
};

/// A JSON tokenizer that reads from a stream and produces tokens.
pub struct JsonTokenizer<R: Read> {
    reader: BufReader<R>,
    peeked: Option<Token>,
    putback_char: Option<u8>,
}

impl<R: Read> JsonTokenizer<R> {
    /// Create a new JSON tokenizer from a reader.
    pub fn new(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader),
            peeked: None,
            putback_char: None,
        }
    }

    /// Skip whitespace and return the next non-whitespace character.
    fn skip_whitespace(&mut self) -> Result<Option<u8>, StreamError> {
        // Check if we have a putback character first
        if let Some(ch) = self.putback_char.take() {
            if !ch.is_ascii_whitespace() {
                return Ok(Some(ch));
            }
            // If putback char is whitespace, continue to read more
        }

        let mut buf = [0; 1];
        loop {
            match self.reader.read(&mut buf)? {
                0 => return Ok(None),
                _ => {
                    let ch = buf[0];
                    if !ch.is_ascii_whitespace() {
                        return Ok(Some(ch));
                    }
                }
            }
        }
    }

    /// Read a string token.
    fn read_string(&mut self) -> Result<String, StreamError> {
        let mut result = String::new();
        let mut buf = [0; 1];
        let mut escaped = false;

        loop {
            match self.reader.read(&mut buf)? {
                0 => return Err(StreamError::UnexpectedEof),
                _ => {
                    let ch = buf[0] as char;
                    if escaped {
                        match ch {
                            '"' => result.push('"'),
                            '\\' => result.push('\\'),
                            '/' => result.push('/'),
                            'b' => result.push('\u{0008}'),
                            'f' => result.push('\u{000C}'),
                            'n' => result.push('\n'),
                            'r' => result.push('\r'),
                            't' => result.push('\t'),
                            _ => {
                                return Err(StreamError::InvalidJson(format!(
                                    "Invalid escape sequence: \\{}",
                                    ch
                                )))
                            }
                        }
                        escaped = false;
                    } else if ch == '"' {
                        break;
                    } else if ch == '\\' {
                        escaped = true;
                    } else {
                        result.push(ch);
                    }
                }
            }
        }

        Ok(result)
    }

    /// Read a number token.
    fn read_number(&mut self, first_char: u8) -> Result<Number, StreamError> {
        let mut number_str = String::new();
        number_str.push(first_char as char);

        let mut buf = [0; 1];
        let mut has_dot = false;
        let mut has_exp = false;
        let mut putback_char: Option<u8> = None;

        loop {
            match self.reader.read(&mut buf)? {
                0 => break,
                _ => {
                    let ch = buf[0] as char;
                    match ch {
                        '0'..='9' => number_str.push(ch),
                        '.' if !has_dot && !has_exp => {
                            has_dot = true;
                            number_str.push('.');
                        }
                        'e' | 'E' if !has_exp => {
                            has_exp = true;
                            number_str.push(ch);
                        }
                        '+' | '-'
                            if has_exp
                                && (number_str.ends_with('e') || number_str.ends_with('E')) =>
                        {
                            number_str.push(ch);
                        }
                        _ => {
                            // We need to put this character back for the next token
                            putback_char = Some(buf[0]);
                            break;
                        }
                    }
                }
            }
        }

        // Store the putback character for the next read
        if let Some(ch) = putback_char {
            // For simplicity, we'll create a simple putback mechanism
            // In a real implementation, we'd use a proper buffer
            self.putback_char = Some(ch);
        }

        // Parse the number
        if has_dot || has_exp {
            match f64::from_str(&number_str) {
                Ok(f) => Ok(Number::Float(f)),
                Err(_) => Err(StreamError::InvalidJson(format!(
                    "Invalid float: {}",
                    number_str
                ))),
            }
        } else if number_str.starts_with('-') {
            match i64::from_str(&number_str) {
                Ok(i) => Ok(Number::SignedInt(i)),
                Err(_) => Err(StreamError::InvalidJson(format!(
                    "Invalid integer: {}",
                    number_str
                ))),
            }
        } else {
            match u64::from_str(&number_str) {
                Ok(u) => Ok(Number::UnsignedInt(u)),
                Err(_) => match i64::from_str(&number_str) {
                    Ok(i) => Ok(Number::SignedInt(i)),
                    Err(_) => Err(StreamError::InvalidJson(format!(
                        "Invalid number: {}",
                        number_str
                    ))),
                },
            }
        }
    }

    /// Read a literal (null, true, false).
    fn read_literal(&mut self, first_char: u8, expected: &str) -> Result<(), StreamError> {
        let mut buf = [0; 1];
        for expected_char in expected.chars().skip(1) {
            match self.reader.read(&mut buf)? {
                0 => return Err(StreamError::UnexpectedEof),
                _ => {
                    let ch = buf[0] as char;
                    if ch != expected_char {
                        return Err(StreamError::InvalidJson(format!(
                            "Expected '{}', found '{}{}'",
                            expected, first_char as char, ch
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// Read the next token from the stream.
    fn read_next_token(&mut self) -> Result<Token, StreamError> {
        match self.skip_whitespace()? {
            None => Ok(Token::Eof),
            Some(ch) => match ch {
                b'{' => Ok(Token::ObjectStart),
                b'}' => Ok(Token::ObjectEnd),
                b'[' => Ok(Token::ArrayStart),
                b']' => Ok(Token::ArrayEnd),
                b'"' => {
                    let string = self.read_string()?;
                    Ok(Token::String(string))
                }
                b'n' => {
                    self.read_literal(ch, "null")?;
                    Ok(Token::Null)
                }
                b't' => {
                    self.read_literal(ch, "true")?;
                    Ok(Token::Bool(true))
                }
                b'f' => {
                    self.read_literal(ch, "false")?;
                    Ok(Token::Bool(false))
                }
                b'0'..=b'9' | b'-' => {
                    let number = self.read_number(ch)?;
                    Ok(Token::Number(number))
                }
                b',' | b':' => {
                    // Skip separators and read the next token
                    self.read_next_token()
                }
                _ => Err(StreamError::InvalidJson(format!(
                    "Unexpected character: '{}'",
                    ch as char
                ))),
            },
        }
    }
}

impl<R: Read> Tokenizer for JsonTokenizer<R> {
    type Error = StreamError;

    fn next_token(&mut self) -> Result<Token, Self::Error> {
        if let Some(token) = self.peeked.take() {
            Ok(token)
        } else {
            self.read_next_token()
        }
    }

    fn peek_token(&mut self) -> Result<&Token, Self::Error> {
        if self.peeked.is_none() {
            self.peeked = Some(self.read_next_token()?);
        }
        Ok(self.peeked.as_ref().unwrap())
    }

    fn expect_token(&mut self, expected: Token) -> Result<(), Self::Error> {
        let token = self.next_token()?;
        if std::mem::discriminant(&token) == std::mem::discriminant(&expected) {
            Ok(())
        } else {
            Err(StreamError::UnexpectedToken {
                expected: format!("{}", expected),
                found: token,
            })
        }
    }

    fn expect_key(&mut self) -> Result<String, Self::Error> {
        match self.next_token()? {
            Token::String(key) => Ok(key),
            token => Err(StreamError::UnexpectedToken {
                expected: "object key".to_string(),
                found: token,
            }),
        }
    }
}
