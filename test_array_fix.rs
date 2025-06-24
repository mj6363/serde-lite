use serde_lite::{JsonTokenizer, StreamDeserialize};
use std::io::Cursor;

// 원래 에러가 발생했던 것과 유사한 구조체
#[derive(Debug)]
struct Config {
    pub version: [u32; 3],
}

impl StreamDeserialize for Config {
    fn deserialize_from_tokens<T: serde_lite::Tokenizer>(
        tokenizer: &mut T,
    ) -> Result<Self, serde_lite::Error>
    where
        T::Error: Into<serde_lite::Error>,
    {
        use serde_lite::{Error, Token};

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

        let mut version = None;

        // Read object fields
        loop {
            match tokenizer.next_token().map_err(|e| e.into())? {
                Token::ObjectEnd => break,
                Token::String(key) => {
                    match key.as_str() {
                        "version" => {
                            version = Some(<[u32; 3]>::deserialize_from_tokens(tokenizer)?);
                        }
                        _ => {
                            // Skip unknown field
                            serde_lite::skip_value(tokenizer)?;
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

        Ok(Config {
            version: version.ok_or(Error::MissingField)?,
        })
    }
}

fn main() {
    let json = r#"{"version": [1, 2, 3]}"#;
    let mut tokenizer = JsonTokenizer::new(Cursor::new(json));

    match Config::deserialize_from_tokens(&mut tokenizer) {
        Ok(config) => {
            println!("성공! Config {{ version: {:?} }}", config.version);
            assert_eq!(config.version, [1, 2, 3]);
            println!("테스트 통과: [u32; 3] StreamDeserialize 구현이 정상 작동합니다!");
        }
        Err(e) => {
            println!("에러: {}", e);
        }
    }
}
