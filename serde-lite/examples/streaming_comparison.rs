//! Example comparing traditional intermediate-based deserialization
//! with streaming deserialization.

use std::io::Cursor;
use std::time::Instant;

use serde_lite::{Deserialize, JsonTokenizer, StreamDeserialize};
use serde_lite_derive::Deserialize;

// Test data structure - now with derive macro that generates both implementations!
#[derive(Debug, PartialEq, Deserialize)]
struct Person {
    name: String,
    age: i32,
    email: String,
    active: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let json_data = r#"[
        {"name": "Alice", "age": 30, "email": "alice@example.com", "active": true},
        {"name": "Bob", "age": 25, "email": "bob@example.com", "active": false},
        {"name": "Charlie", "age": 35, "email": "charlie@example.com", "active": true},
        {"name": "Diana", "age": 28, "email": "diana@example.com", "active": true},
        {"name": "Eve", "age": 32, "email": "eve@example.com", "active": false}
    ]"#;

    println!("Comparing deserialization methods...\n");

    // Traditional method: JSON -> Intermediate -> Struct
    let start = Instant::now();
    let intermediate: serde_lite::Intermediate = serde_json::from_str(json_data)?;
    let people_traditional: Vec<Person> = Vec::deserialize(&intermediate)?;
    let traditional_time = start.elapsed();

    println!("Traditional method (JSON -> Intermediate -> Struct):");
    println!("  Time: {:?}", traditional_time);
    println!("  People count: {}", people_traditional.len());
    println!("  First person: {:?}\n", people_traditional.first());

    // Streaming method: JSON -> Struct directly
    let start = Instant::now();
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json_data));
    let people_streaming: Vec<Person> = Vec::deserialize_from_tokens(&mut tokenizer)?;
    let streaming_time = start.elapsed();

    println!("Streaming method (JSON -> Struct directly):");
    println!("  Time: {:?}", streaming_time);
    println!("  People count: {}", people_streaming.len());
    println!("  First person: {:?}\n", people_streaming.first());

    // Verify results are identical
    assert_eq!(people_traditional, people_streaming);
    println!("✓ Both methods produced identical results!");

    // Performance comparison
    if streaming_time < traditional_time {
        let speedup = traditional_time.as_nanos() as f64 / streaming_time.as_nanos() as f64;
        println!("🚀 Streaming method is {:.2}x faster!", speedup);
    } else {
        let slowdown = streaming_time.as_nanos() as f64 / traditional_time.as_nanos() as f64;
        println!("⚠️  Streaming method is {:.2}x slower", slowdown);
    }

    println!("\nMemory usage benefits:");
    println!("  Traditional: JSON + Intermediate + Struct (3x memory)");
    println!("  Streaming: JSON + Struct (2x memory, ~33% savings)");

    Ok(())
}
