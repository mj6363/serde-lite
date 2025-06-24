use serde_lite::{Deserialize, JsonTokenizer, StreamDeserialize};
use serde_lite_derive::Deserialize;
use std::io::Cursor;

#[derive(Debug, PartialEq, Deserialize)]
struct SimpleStruct {
    name: String,
    age: i32,
}

#[test]
fn test_derive_generates_both_traits() {
    let json = r#"{"name": "John", "age": 30}"#;

    // Test traditional deserialize
    let intermediate: serde_lite::Intermediate = serde_json::from_str(json).unwrap();
    let person1: SimpleStruct = SimpleStruct::deserialize(&intermediate).unwrap();

    // Test streaming deserialize
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));
    let person2: SimpleStruct = SimpleStruct::deserialize_from_tokens(&mut tokenizer).unwrap();

    assert_eq!(person1, person2);
    assert_eq!(person1.name, "John");
    assert_eq!(person1.age, 30);
}
