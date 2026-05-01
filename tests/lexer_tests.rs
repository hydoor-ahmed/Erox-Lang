use erox_lib::compiler::lexer::{Lexer, Token};


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_token_keywords_and_symbols() {
        let input = r#"
            let x = 10
            async fn scan() {
                return 1
            }
        "#;

        let mut lexer = Lexer::new(input.to_string());

        let expected_tokens = vec![
            Token::Let,
            Token::Identifier("x".to_string()),
            Token::Assign,
            Token::Number(10.0),
            Token::Async,
            Token::Fn,
            Token::Identifier("scan".to_string()),
            Token::LParen,
            Token::RParen,
            Token::LBrace,
            Token::Return,
            Token::Number(1.0),
            Token::RBrace,
            Token::EOF,
        ];

        for expected in expected_tokens {
            let tok = lexer.next_token();
            assert_eq!(tok, expected, "Error: Expected {:?}, got {:?}", expected, tok);
        }
        
        println!("Lexer Test Passed! 🛰️✅");
    }
}