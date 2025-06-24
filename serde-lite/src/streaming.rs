//! Streaming deserialization support.
//!
//! This module provides token-based streaming deserialization that avoids
//! creating intermediate representations, directly deserializing from token
//! streams to target types.

use std::{
    fmt::{self, Display, Formatter},
    io,
};

use crate::{Error, Number};

/// A token in the input stream.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// Null value
    Null,
    /// Boolean value
    Bool(bool),
    /// Numeric value
    Number(Number),
    /// String value
    String(String),
    /// Start of an object `{`
    ObjectStart,
    /// End of an object `}`
    ObjectEnd,
    /// Start of an array `[`
    ArrayStart,
    /// End of an array `]`
    ArrayEnd,
    /// Object key
    Key(String),
    /// End of input
    Eof,
}

impl Display for Token {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Token::Null => write!(f, "null"),
            Token::Bool(b) => write!(f, "{}", b),
            Token::Number(n) => write!(f, "{:?}", n),
            Token::String(s) => write!(f, "\"{}\"", s),
            Token::ObjectStart => write!(f, "{{"),
            Token::ObjectEnd => write!(f, "}}"),
            Token::ArrayStart => write!(f, "["),
            Token::ArrayEnd => write!(f, "]"),
            Token::Key(k) => write!(f, "\"{}\":", k),
            Token::Eof => write!(f, "EOF"),
        }
    }
}

/// Trait for tokenizing input streams.
pub trait Tokenizer {
    /// Error type for tokenization failures.
    type Error;

    /// Get the next token from the stream.
    fn next_token(&mut self) -> Result<Token, Self::Error>;

    /// Peek at the next token without consuming it.
    fn peek_token(&mut self) -> Result<&Token, Self::Error>;

    /// Expect a specific token, consuming it if it matches.
    fn expect_token(&mut self, expected: Token) -> Result<(), Self::Error>;

    /// Expect and consume a key token, returning the key string.
    fn expect_key(&mut self) -> Result<String, Self::Error>;
}

/// Trait for types that can be deserialized from a token stream.
pub trait StreamDeserialize {
    /// Deserialize from a token stream.
    fn deserialize_from_tokens<T: Tokenizer>(tokenizer: &mut T) -> Result<Self, Error>
    where
        Self: Sized,
        T::Error: Into<Error>;
}

/// Skip a value in the token stream.
pub fn skip_value<T: Tokenizer>(tokenizer: &mut T) -> Result<(), Error>
where
    T::Error: Into<Error>,
{
    match tokenizer.next_token().map_err(|e| e.into())? {
        Token::Null | Token::Bool(_) | Token::Number(_) | Token::String(_) => Ok(()),
        Token::ObjectStart => {
            loop {
                match tokenizer.next_token().map_err(|e| e.into())? {
                    Token::ObjectEnd => break,
                    Token::Key(_) => skip_value(tokenizer)?,
                    _ => return Err(Error::invalid_value_static("object key or end")),
                }
            }
            Ok(())
        }
        Token::ArrayStart => {
            loop {
                match tokenizer.peek_token().map_err(|e| e.into())? {
                    Token::ArrayEnd => {
                        tokenizer.next_token().map_err(|e| e.into())?;
                        break;
                    }
                    _ => skip_value(tokenizer)?,
                }
            }
            Ok(())
        }
        _ => Err(Error::invalid_value_static("value")),
    }
}

/// Error type for streaming operations.
#[derive(Debug, Clone)]
pub enum StreamError {
    /// IO error
    Io(String),
    /// JSON parsing error
    Parse(String),
    /// Unexpected token
    UnexpectedToken { expected: String, found: Token },
    /// Unexpected end of input
    UnexpectedEof,
    /// Invalid JSON structure
    InvalidJson(String),
}

impl Display for StreamError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            StreamError::Io(msg) => write!(f, "IO error: {}", msg),
            StreamError::Parse(msg) => write!(f, "Parse error: {}", msg),
            StreamError::UnexpectedToken { expected, found } => {
                write!(f, "Expected {}, found {}", expected, found)
            }
            StreamError::UnexpectedEof => write!(f, "Unexpected end of input"),
            StreamError::InvalidJson(msg) => write!(f, "Invalid JSON: {}", msg),
        }
    }
}

impl std::error::Error for StreamError {}

impl From<io::Error> for StreamError {
    fn from(err: io::Error) -> Self {
        StreamError::Io(err.to_string())
    }
}

impl From<StreamError> for Error {
    fn from(err: StreamError) -> Self {
        Error::custom(err.to_string())
    }
}

// Basic implementations for primitive types
impl StreamDeserialize for bool {
    fn deserialize_from_tokens<T: Tokenizer>(tokenizer: &mut T) -> Result<Self, Error>
    where
        T::Error: Into<Error>,
    {
        match tokenizer.next_token().map_err(|e| e.into())? {
            Token::Bool(b) => Ok(b),
            token => Err(Error::invalid_value(format!(
                "expected bool, found {}",
                token
            ))),
        }
    }
}

impl StreamDeserialize for String {
    fn deserialize_from_tokens<T: Tokenizer>(tokenizer: &mut T) -> Result<Self, Error>
    where
        T::Error: Into<Error>,
    {
        match tokenizer.next_token().map_err(|e| e.into())? {
            Token::String(s) => Ok(s),
            token => Err(Error::invalid_value(format!(
                "expected string, found {}",
                token
            ))),
        }
    }
}

macro_rules! impl_stream_deserialize_for_int {
    ($ty:ty, $type_name:expr) => {
        impl StreamDeserialize for $ty {
            fn deserialize_from_tokens<T: Tokenizer>(tokenizer: &mut T) -> Result<Self, Error>
            where
                T::Error: Into<Error>,
            {
                match tokenizer.next_token().map_err(|e| e.into())? {
                    Token::Number(n) => n.try_into(),
                    token => Err(Error::invalid_value(format!(
                        "expected {}, found {}",
                        $type_name, token
                    ))),
                }
            }
        }
    };
}

impl_stream_deserialize_for_int!(i8, "i8");
impl_stream_deserialize_for_int!(i16, "i16");
impl_stream_deserialize_for_int!(i32, "i32");
impl_stream_deserialize_for_int!(i64, "i64");
impl_stream_deserialize_for_int!(isize, "isize");
impl_stream_deserialize_for_int!(u8, "u8");
impl_stream_deserialize_for_int!(u16, "u16");
impl_stream_deserialize_for_int!(u32, "u32");
impl_stream_deserialize_for_int!(u64, "u64");
impl_stream_deserialize_for_int!(usize, "usize");

impl StreamDeserialize for f32 {
    fn deserialize_from_tokens<T: Tokenizer>(tokenizer: &mut T) -> Result<Self, Error>
    where
        T::Error: Into<Error>,
    {
        match tokenizer.next_token().map_err(|e| e.into())? {
            Token::Number(n) => Ok(f64::from(n) as f32),
            token => Err(Error::invalid_value(format!(
                "expected f32, found {}",
                token
            ))),
        }
    }
}

impl StreamDeserialize for f64 {
    fn deserialize_from_tokens<T: Tokenizer>(tokenizer: &mut T) -> Result<Self, Error>
    where
        T::Error: Into<Error>,
    {
        match tokenizer.next_token().map_err(|e| e.into())? {
            Token::Number(n) => Ok(n.into()),
            token => Err(Error::invalid_value(format!(
                "expected f64, found {}",
                token
            ))),
        }
    }
}

impl<T> StreamDeserialize for Option<T>
where
    T: StreamDeserialize,
{
    fn deserialize_from_tokens<Tok: Tokenizer>(tokenizer: &mut Tok) -> Result<Self, Error>
    where
        Tok::Error: Into<Error>,
    {
        match tokenizer.peek_token().map_err(|e| e.into())? {
            Token::Null => {
                tokenizer.next_token().map_err(|e| e.into())?;
                Ok(None)
            }
            _ => Ok(Some(T::deserialize_from_tokens(tokenizer)?)),
        }
    }
}

impl<T> StreamDeserialize for Vec<T>
where
    T: StreamDeserialize,
{
    fn deserialize_from_tokens<Tok: Tokenizer>(tokenizer: &mut Tok) -> Result<Self, Error>
    where
        Tok::Error: Into<Error>,
    {
        match tokenizer.next_token().map_err(|e| e.into())? {
            Token::ArrayStart => {
                let mut vec = Vec::new();
                loop {
                    match tokenizer.peek_token().map_err(|e| e.into())? {
                        Token::ArrayEnd => {
                            tokenizer.next_token().map_err(|e| e.into())?;
                            break;
                        }
                        _ => {
                            vec.push(T::deserialize_from_tokens(tokenizer)?);
                        }
                    }
                }
                Ok(vec)
            }
            token => Err(Error::invalid_value(format!(
                "expected array, found {}",
                token
            ))),
        }
    }
}

impl<T, const N: usize> StreamDeserialize for [T; N]
where
    T: StreamDeserialize,
{
    fn deserialize_from_tokens<Tok: Tokenizer>(tokenizer: &mut Tok) -> Result<Self, Error>
    where
        Tok::Error: Into<Error>,
    {
        match tokenizer.next_token().map_err(|e| e.into())? {
            Token::ArrayStart => {
                let mut result = Vec::with_capacity(N);

                // Read exactly N elements
                for i in 0..N {
                    match tokenizer.peek_token().map_err(|e| e.into())? {
                        Token::ArrayEnd => {
                            return Err(Error::invalid_value(format!(
                                "expected array of length {}, found array of length {}",
                                N, i
                            )));
                        }
                        _ => {
                            result.push(T::deserialize_from_tokens(tokenizer)?);
                        }
                    }
                }

                // Expect ArrayEnd token
                match tokenizer.next_token().map_err(|e| e.into())? {
                    Token::ArrayEnd => {
                        // Convert Vec to fixed-size array
                        result.try_into().map_err(|_| {
                            Error::invalid_value(format!(
                                "failed to convert vector to array of length {}",
                                N
                            ))
                        })
                    }
                    _ => Err(Error::invalid_value(format!(
                        "expected array of length {}, found longer array",
                        N
                    ))),
                }
            }
            token => Err(Error::invalid_value(format!(
                "expected array, found {}",
                token
            ))),
        }
    }
}
