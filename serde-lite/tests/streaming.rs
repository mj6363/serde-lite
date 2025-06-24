use std::io::Cursor;

use serde_lite::{skip_value, Error, JsonTokenizer, StreamDeserialize, Token, Tokenizer};

#[test]
fn test_json_tokenizer_basic() {
    let json = r#"{"name": "John", "age": 30, "active": true}"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));

    assert_eq!(tokenizer.next_token().unwrap(), Token::ObjectStart);
    assert_eq!(
        tokenizer.next_token().unwrap(),
        Token::String("name".to_string())
    );
    assert_eq!(
        tokenizer.next_token().unwrap(),
        Token::String("John".to_string())
    );
    assert_eq!(
        tokenizer.next_token().unwrap(),
        Token::String("age".to_string())
    );
    // Note: JSON numbers are parsed as tokens, so 30 will be a Number token
    assert_eq!(
        tokenizer.next_token().unwrap(),
        Token::Number(serde_lite::Number::UnsignedInt(30))
    );
    assert_eq!(
        tokenizer.next_token().unwrap(),
        Token::String("active".to_string())
    );
    assert_eq!(tokenizer.next_token().unwrap(), Token::Bool(true));
    assert_eq!(tokenizer.next_token().unwrap(), Token::ObjectEnd);
}

#[test]
fn test_stream_deserialize_primitives() {
    // Test string
    let json = r#""hello world""#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: String = String::deserialize_from_tokens(&mut tokenizer).unwrap();
    assert_eq!(result, "hello world");

    // Test boolean
    let json = r#"true"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: bool = bool::deserialize_from_tokens(&mut tokenizer).unwrap();
    assert_eq!(result, true);

    // Test integer
    let json = r#"42"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: i32 = i32::deserialize_from_tokens(&mut tokenizer).unwrap();
    assert_eq!(result, 42);
}

#[test]
fn test_stream_deserialize_array() {
    let json = r#"[1, 2, 3, 4, 5]"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: Vec<i32> = Vec::deserialize_from_tokens(&mut tokenizer).unwrap();
    assert_eq!(result, vec![1, 2, 3, 4, 5]);
}

#[test]
fn test_stream_deserialize_option() {
    // Test Some
    let json = r#"42"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: Option<i32> = Option::deserialize_from_tokens(&mut tokenizer).unwrap();
    assert_eq!(result, Some(42));

    // Test None
    let json = r#"null"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: Option<i32> = Option::deserialize_from_tokens(&mut tokenizer).unwrap();
    assert_eq!(result, None);
}

// Simple struct for testing manual deserialization
struct Person {
    name: String,
    age: i32,
    active: bool,
}

impl StreamDeserialize for Person {
    fn deserialize_from_tokens<T: Tokenizer>(tokenizer: &mut T) -> Result<Self, Error>
    where
        T::Error: Into<Error>,
    {
        // Expect object start
        match tokenizer.next_token().map_err(|e| e.into())? {
            Token::ObjectStart => {}
            token => {
                return Err(Error::invalid_value(format!(
                    "expected object, found {}",
                    token
                )))
            }
        }

        let mut name = None;
        let mut age = None;
        let mut active = None;

        // Read object fields
        loop {
            match tokenizer.next_token().map_err(|e| e.into())? {
                Token::ObjectEnd => break,
                Token::String(key) => {
                    match key.as_str() {
                        "name" => {
                            name = Some(String::deserialize_from_tokens(tokenizer)?);
                        }
                        "age" => {
                            age = Some(i32::deserialize_from_tokens(tokenizer)?);
                        }
                        "active" => {
                            active = Some(bool::deserialize_from_tokens(tokenizer)?);
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
                    )))
                }
            }
        }

        Ok(Person {
            name: name.ok_or(Error::MissingField)?,
            age: age.ok_or(Error::MissingField)?,
            active: active.ok_or(Error::MissingField)?,
        })
    }
}

#[test]
fn test_stream_deserialize_struct() {
    let json = r#"{"name": "John", "age": 30, "active": true}"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: Person = Person::deserialize_from_tokens(&mut tokenizer).unwrap();

    assert_eq!(result.name, "John");
    assert_eq!(result.age, 30);
    assert_eq!(result.active, true);
}
