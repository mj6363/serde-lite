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
fn test_stream_deserialize_fixed_array() {
    // Test [u32; 3]
    let json = r#"[1, 2, 3]"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: [u32; 3] = <[u32; 3]>::deserialize_from_tokens(&mut tokenizer).unwrap();
    assert_eq!(result, [1, 2, 3]);

    // Test [String; 2]
    let json = r#"["hello", "world"]"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: [String; 2] = <[String; 2]>::deserialize_from_tokens(&mut tokenizer).unwrap();
    assert_eq!(result, ["hello".to_string(), "world".to_string()]);

    // Test empty array [i32; 0]
    let json = r#"[]"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: [i32; 0] = <[i32; 0]>::deserialize_from_tokens(&mut tokenizer).unwrap();
    assert_eq!(result, []);
}

#[test]
fn test_stream_deserialize_fixed_array_errors() {
    // Test array too short
    let json = r#"[1, 2]"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: Result<[u32; 3], _> = <[u32; 3]>::deserialize_from_tokens(&mut tokenizer);
    assert!(result.is_err());

    // Test array too long
    let json = r#"[1, 2, 3, 4]"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: Result<[u32; 3], _> = <[u32; 3]>::deserialize_from_tokens(&mut tokenizer);
    assert!(result.is_err());

    // Test not an array
    let json = r#"42"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: Result<[u32; 3], _> = <[u32; 3]>::deserialize_from_tokens(&mut tokenizer);
    assert!(result.is_err());
}

#[test]
fn test_stream_deserialize_btreemap() {
    use std::collections::BTreeMap;

    let json = r#"{"key1": "value1", "key2": "value2"}"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: BTreeMap<String, String> =
        BTreeMap::deserialize_from_tokens(&mut tokenizer).unwrap();

    assert_eq!(result.len(), 2);
    assert_eq!(result.get("key1"), Some(&"value1".to_string()));
    assert_eq!(result.get("key2"), Some(&"value2".to_string()));
}

#[test]
fn test_stream_deserialize_hashmap() {
    use std::collections::HashMap;

    let json = r#"{"key1": 42, "key2": 84}"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: HashMap<String, i32> = HashMap::deserialize_from_tokens(&mut tokenizer).unwrap();

    assert_eq!(result.len(), 2);
    assert_eq!(result.get("key1"), Some(&42));
    assert_eq!(result.get("key2"), Some(&84));
}

#[test]
fn test_stream_deserialize_hashset() {
    use std::collections::HashSet;

    let json = r#"[1, 2, 3, 2, 1]"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: HashSet<i32> = HashSet::deserialize_from_tokens(&mut tokenizer).unwrap();

    assert_eq!(result.len(), 3);
    assert!(result.contains(&1));
    assert!(result.contains(&2));
    assert!(result.contains(&3));
}

#[test]
fn test_stream_deserialize_range() {
    use std::ops::Range;

    let json = r#"{"start": 5, "end": 10}"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: Range<i32> = Range::deserialize_from_tokens(&mut tokenizer).unwrap();

    assert_eq!(result.start, 5);
    assert_eq!(result.end, 10);
}

#[test]
fn test_stream_deserialize_unit() {
    let json = r#"null"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: () = <()>::deserialize_from_tokens(&mut tokenizer).unwrap();

    assert_eq!(result, ());
}

#[test]
fn test_stream_deserialize_btreeset() {
    use std::collections::BTreeSet;

    let json = r#"[3, 1, 4, 1, 5, 9, 2, 6]"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: BTreeSet<i32> = BTreeSet::deserialize_from_tokens(&mut tokenizer).unwrap();

    // BTreeSet should be sorted and deduplicated
    let expected: BTreeSet<i32> = [1, 2, 3, 4, 5, 6, 9].iter().cloned().collect();
    assert_eq!(result, expected);
}

#[test]
fn test_stream_deserialize_box() {
    let json = r#"42"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: Box<i32> = Box::deserialize_from_tokens(&mut tokenizer).unwrap();

    assert_eq!(*result, 42);
}

#[test]
fn test_stream_deserialize_tuple() {
    let json = r#"["hello", 42, true]"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: (String, i32, bool) =
        <(String, i32, bool)>::deserialize_from_tokens(&mut tokenizer).unwrap();

    assert_eq!(result.0, "hello");
    assert_eq!(result.1, 42);
    assert_eq!(result.2, true);
}

#[test]
fn test_stream_deserialize_char() {
    let json = r#""A""#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: char = char::deserialize_from_tokens(&mut tokenizer).unwrap();

    assert_eq!(result, 'A');
}

#[test]
fn test_stream_deserialize_i128_u128() {
    // Test i128
    let json = r#"123456789"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: i128 = i128::deserialize_from_tokens(&mut tokenizer).unwrap();
    assert_eq!(result, 123456789i128);

    // Test u128
    let json = r#"987654321"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let result: u128 = u128::deserialize_from_tokens(&mut tokenizer).unwrap();
    assert_eq!(result, 987654321u128);
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
