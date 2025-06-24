//! Streaming deserialization support.
//!
//! This module provides token-based streaming deserialization that avoids
//! creating intermediate representations, directly deserializing from token
//! streams to target types.

use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt::{self, Display, Formatter},
    hash::{BuildHasher, Hash},
    io,
    ops::Range,
    rc::Rc,
    sync::{Arc, Mutex},
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

/// Helper function to deserialize a key from a string token for maps
fn deserialize_key_from_token<K>(key_str: &str) -> Result<K, Error>
where
    K: StreamDeserialize,
{
    use crate::JsonTokenizer;
    use std::io::Cursor;

    // Try to deserialize as string first
    let string_json = format!("\"{}\"", key_str.replace("\"", "\\\""));
    let mut tokenizer = JsonTokenizer::new(Cursor::new(string_json));
    if let Ok(key) = K::deserialize_from_tokens(&mut tokenizer) {
        return Ok(key);
    }

    // Try to parse as number
    if let Ok(unsigned) = key_str.parse::<u64>() {
        let mut tokenizer = JsonTokenizer::new(Cursor::new(unsigned.to_string()));
        if let Ok(key) = K::deserialize_from_tokens(&mut tokenizer) {
            return Ok(key);
        }
    }

    if let Ok(signed) = key_str.parse::<i64>() {
        let mut tokenizer = JsonTokenizer::new(Cursor::new(signed.to_string()));
        if let Ok(key) = K::deserialize_from_tokens(&mut tokenizer) {
            return Ok(key);
        }
    }

    if let Ok(float) = key_str.parse::<f64>() {
        let mut tokenizer = JsonTokenizer::new(Cursor::new(float.to_string()));
        if let Ok(key) = K::deserialize_from_tokens(&mut tokenizer) {
            return Ok(key);
        }
    }

    Err(Error::invalid_value_static("key"))
}

impl<K, V, S> StreamDeserialize for HashMap<K, V, S>
where
    K: StreamDeserialize + Eq + Hash,
    V: StreamDeserialize,
    S: BuildHasher + Default,
{
    fn deserialize_from_tokens<Tok: Tokenizer>(tokenizer: &mut Tok) -> Result<Self, Error>
    where
        Tok::Error: Into<Error>,
    {
        match tokenizer.next_token().map_err(|e| e.into())? {
            Token::ObjectStart => {
                let mut map = HashMap::with_hasher(Default::default());

                loop {
                    match tokenizer.next_token().map_err(|e| e.into())? {
                        Token::ObjectEnd => break,
                        Token::String(key_str) => {
                            let key = deserialize_key_from_token(&key_str)?;
                            let value = V::deserialize_from_tokens(tokenizer)?;
                            map.insert(key, value);
                        }
                        token => {
                            return Err(Error::invalid_value(format!(
                                "expected object key, found {}",
                                token
                            )));
                        }
                    }
                }

                Ok(map)
            }
            token => Err(Error::invalid_value(format!(
                "expected object, found {}",
                token
            ))),
        }
    }
}

impl<K, V> StreamDeserialize for BTreeMap<K, V>
where
    K: StreamDeserialize + Ord,
    V: StreamDeserialize,
{
    fn deserialize_from_tokens<Tok: Tokenizer>(tokenizer: &mut Tok) -> Result<Self, Error>
    where
        Tok::Error: Into<Error>,
    {
        match tokenizer.next_token().map_err(|e| e.into())? {
            Token::ObjectStart => {
                let mut map = BTreeMap::new();

                loop {
                    match tokenizer.next_token().map_err(|e| e.into())? {
                        Token::ObjectEnd => break,
                        Token::String(key_str) => {
                            let key = deserialize_key_from_token(&key_str)?;
                            let value = V::deserialize_from_tokens(tokenizer)?;
                            map.insert(key, value);
                        }
                        token => {
                            return Err(Error::invalid_value(format!(
                                "expected object key, found {}",
                                token
                            )));
                        }
                    }
                }

                Ok(map)
            }
            token => Err(Error::invalid_value(format!(
                "expected object, found {}",
                token
            ))),
        }
    }
}

impl<T, S> StreamDeserialize for HashSet<T, S>
where
    T: StreamDeserialize + Eq + Hash,
    S: BuildHasher + Default,
{
    fn deserialize_from_tokens<Tok: Tokenizer>(tokenizer: &mut Tok) -> Result<Self, Error>
    where
        Tok::Error: Into<Error>,
    {
        match tokenizer.next_token().map_err(|e| e.into())? {
            Token::ArrayStart => {
                let mut set = HashSet::with_hasher(Default::default());

                loop {
                    match tokenizer.peek_token().map_err(|e| e.into())? {
                        Token::ArrayEnd => {
                            tokenizer.next_token().map_err(|e| e.into())?;
                            break;
                        }
                        _ => {
                            let item = T::deserialize_from_tokens(tokenizer)?;
                            set.insert(item);
                        }
                    }
                }

                Ok(set)
            }
            token => Err(Error::invalid_value(format!(
                "expected array, found {}",
                token
            ))),
        }
    }
}

impl<T> StreamDeserialize for Range<T>
where
    T: StreamDeserialize,
{
    fn deserialize_from_tokens<Tok: Tokenizer>(tokenizer: &mut Tok) -> Result<Self, Error>
    where
        Tok::Error: Into<Error>,
    {
        match tokenizer.next_token().map_err(|e| e.into())? {
            Token::ObjectStart => {
                let mut start = None;
                let mut end = None;

                loop {
                    match tokenizer.next_token().map_err(|e| e.into())? {
                        Token::ObjectEnd => break,
                        Token::String(key) => {
                            match key.as_str() {
                                "start" => {
                                    start = Some(T::deserialize_from_tokens(tokenizer)?);
                                }
                                "end" => {
                                    end = Some(T::deserialize_from_tokens(tokenizer)?);
                                }
                                _ => {
                                    // Skip unknown field
                                    skip_value(tokenizer)?;
                                }
                            }
                        }
                        token => {
                            return Err(Error::invalid_value(format!(
                                "expected object key, found {}",
                                token
                            )));
                        }
                    }
                }

                let start = start.ok_or(Error::MissingField)?;
                let end = end.ok_or(Error::MissingField)?;

                Ok(start..end)
            }
            token => Err(Error::invalid_value(format!(
                "expected object, found {}",
                token
            ))),
        }
    }
}

impl StreamDeserialize for () {
    fn deserialize_from_tokens<Tok: Tokenizer>(tokenizer: &mut Tok) -> Result<Self, Error>
    where
        Tok::Error: Into<Error>,
    {
        match tokenizer.next_token().map_err(|e| e.into())? {
            Token::Null => Ok(()),
            token => Err(Error::invalid_value(format!(
                "expected null, found {}",
                token
            ))),
        }
    }
}

impl<T> StreamDeserialize for BTreeSet<T>
where
    T: StreamDeserialize + Ord,
{
    fn deserialize_from_tokens<Tok: Tokenizer>(tokenizer: &mut Tok) -> Result<Self, Error>
    where
        Tok::Error: Into<Error>,
    {
        match tokenizer.next_token().map_err(|e| e.into())? {
            Token::ArrayStart => {
                let mut set = BTreeSet::new();

                loop {
                    match tokenizer.peek_token().map_err(|e| e.into())? {
                        Token::ArrayEnd => {
                            tokenizer.next_token().map_err(|e| e.into())?;
                            break;
                        }
                        _ => {
                            let item = T::deserialize_from_tokens(tokenizer)?;
                            set.insert(item);
                        }
                    }
                }

                Ok(set)
            }
            token => Err(Error::invalid_value(format!(
                "expected array, found {}",
                token
            ))),
        }
    }
}

// Wrapper types
macro_rules! impl_stream_deserialize_wrapper {
    ($wrapper:ident) => {
        impl<T> StreamDeserialize for $wrapper<T>
        where
            T: StreamDeserialize,
        {
            fn deserialize_from_tokens<Tok: Tokenizer>(tokenizer: &mut Tok) -> Result<Self, Error>
            where
                Tok::Error: Into<Error>,
            {
                let inner = T::deserialize_from_tokens(tokenizer)?;
                Ok($wrapper::new(inner))
            }
        }
    };
}

impl_stream_deserialize_wrapper!(Box);
impl_stream_deserialize_wrapper!(Rc);
impl_stream_deserialize_wrapper!(Arc);
impl_stream_deserialize_wrapper!(Cell);
impl_stream_deserialize_wrapper!(RefCell);
impl_stream_deserialize_wrapper!(Mutex);

// Tuple implementations
macro_rules! impl_stream_deserialize_tuple {
    ($len:expr => ($($n:tt $ty:ident)+)) => {
        impl<$($ty),+> StreamDeserialize for ($($ty,)+)
        where
            $($ty: StreamDeserialize,)+
        {
            fn deserialize_from_tokens<Tok: Tokenizer>(tokenizer: &mut Tok) -> Result<Self, Error>
            where
                Tok::Error: Into<Error>,
            {
                match tokenizer.next_token().map_err(|e| e.into())? {
                    Token::ArrayStart => {
                        let result = (
                            $(
                                $ty::deserialize_from_tokens(tokenizer)?,
                            )+
                        );

                        // Expect ArrayEnd token
                        match tokenizer.next_token().map_err(|e| e.into())? {
                            Token::ArrayEnd => Ok(result),
                            token => Err(Error::invalid_value(format!(
                                "expected end of tuple array, found {}",
                                token
                            ))),
                        }
                    }
                    token => Err(Error::invalid_value(format!(
                        "expected array for tuple, found {}",
                        token
                    ))),
                }
            }
        }
    };
}

impl_stream_deserialize_tuple!(1 => (0 T0));
impl_stream_deserialize_tuple!(2 => (0 T0 1 T1));
impl_stream_deserialize_tuple!(3 => (0 T0 1 T1 2 T2));
impl_stream_deserialize_tuple!(4 => (0 T0 1 T1 2 T2 3 T3));
impl_stream_deserialize_tuple!(5 => (0 T0 1 T1 2 T2 3 T3 4 T4));
impl_stream_deserialize_tuple!(6 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5));
impl_stream_deserialize_tuple!(7 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6));
impl_stream_deserialize_tuple!(8 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7));
impl_stream_deserialize_tuple!(9 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8));
impl_stream_deserialize_tuple!(10 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8 9 T9));
impl_stream_deserialize_tuple!(11 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8 9 T9 10 T10));
impl_stream_deserialize_tuple!(12 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8 9 T9 10 T10 11 T11));
impl_stream_deserialize_tuple!(13 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8 9 T9 10 T10 11 T11 12 T12));
impl_stream_deserialize_tuple!(14 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8 9 T9 10 T10 11 T11 12 T12 13 T13));
impl_stream_deserialize_tuple!(15 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8 9 T9 10 T10 11 T11 12 T12 13 T13 14 T14));
impl_stream_deserialize_tuple!(16 => (0 T0 1 T1 2 T2 3 T3 4 T4 5 T5 6 T6 7 T7 8 T8 9 T9 10 T10 11 T11 12 T12 13 T13 14 T14 15 T15));

// Additional integer types
impl StreamDeserialize for i128 {
    fn deserialize_from_tokens<T: Tokenizer>(tokenizer: &mut T) -> Result<Self, Error>
    where
        T::Error: Into<Error>,
    {
        // i128 is deserialized as i64 and then converted
        let val = i64::deserialize_from_tokens(tokenizer)?;
        Ok(val as i128)
    }
}

impl StreamDeserialize for u128 {
    fn deserialize_from_tokens<T: Tokenizer>(tokenizer: &mut T) -> Result<Self, Error>
    where
        T::Error: Into<Error>,
    {
        // u128 is deserialized as u64 and then converted
        let val = u64::deserialize_from_tokens(tokenizer)?;
        Ok(val as u128)
    }
}

impl StreamDeserialize for char {
    fn deserialize_from_tokens<T: Tokenizer>(tokenizer: &mut T) -> Result<Self, Error>
    where
        T::Error: Into<Error>,
    {
        match tokenizer.next_token().map_err(|e| e.into())? {
            Token::String(s) => {
                let mut chars = s.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => Ok(c),
                    _ => Err(Error::invalid_value_static("single character string")),
                }
            }
            token => Err(Error::invalid_value(format!(
                "expected string for char, found {}",
                token
            ))),
        }
    }
}
