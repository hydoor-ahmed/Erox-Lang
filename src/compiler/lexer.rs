// EROX Language Lexer — Professional Stateful Tokenizer with Diagnostics

#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    // Keywords
    Let, Const, Fn, If, Else, Return, Async, Await, Import, From, While, For,
    Try, Catch,
    // Literals
    Identifier(String),
    Number(f64),
    String(String),
    True, False, NullLiteral,
    // Operators
    Assign, Plus, Minus, Star, Slash, Modulo,
    Bang, Equal, NotEqual, GT, LT, GTE, LTE, Arrow,
    PlusPlus, MinusMinus,
    // Delimiters
    LParen, RParen, LBrace, RBrace, LBracket, RBracket,
    Semicolon, Comma, Colon, Dot,
    // Special
    Dollar,
    InterpolationStart, // ${
    EOF,
}

impl Token {
    pub fn lookup_identifier(ident: &str) -> Token {
        match ident {
            "fn" => Token::Fn,
            "let" => Token::Let,
            "const" => Token::Const,
            "true" => Token::True,
            "false" => Token::False,
            "null" => Token::NullLiteral,
            "if" => Token::If,
            "else" => Token::Else,
            "return" => Token::Return,
            "async" => Token::Async,
            "await" => Token::Await,
            "while" => Token::While,
            "for" => Token::For,
            "import" => Token::Import,
            "from" => Token::From,
            "try" => Token::Try,
            "catch" => Token::Catch,
            _ => Token::Identifier(ident.to_string()),
        }
    }
}

/// Source position for diagnostic reporting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub line: usize,
    pub col: usize,
}

impl Span {
    pub fn new(line: usize, col: usize) -> Self {
        Span { line, col }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum LexerMode {
    Normal,
    String,
}

#[derive(Clone)]
pub struct Lexer {
    input: Vec<char>,
    pub source: String,
    position: usize,
    read_position: usize,
    ch: char,
    pub mode: LexerMode,
    // Position tracking
    pub line: usize,
    pub col: usize,
    pub last_span: Span,
}

impl Lexer {
    pub fn new(input: String) -> Self {
        let mut l = Lexer {
            input: input.chars().collect(),
            source: input,
            position: 0,
            read_position: 0,
            ch: '\0',
            mode: LexerMode::Normal,
            line: 1,
            col: 0,
            last_span: Span::new(1, 0),
        };
        l.read_char();
        l
    }

    fn read_char(&mut self) {
        if self.read_position >= self.input.len() {
            self.ch = '\0';
        } else {
            self.ch = self.input[self.read_position];
        }
        self.position = self.read_position;
        self.read_position += 1;
        // Track line and column
        if self.ch == '\n' {
            self.line += 1;
            self.col = 0;
        } else {
            self.col += 1;
        }
    }

    fn peek_char(&self) -> char {
        if self.read_position >= self.input.len() { '\0' } else { self.input[self.read_position] }
    }

    fn mark_span(&mut self) {
        self.last_span = Span::new(self.line, self.col);
    }

    pub fn next_token(&mut self) -> Token {
        match self.mode {
            LexerMode::String => self.scan_string_mode(),
            LexerMode::Normal => self.scan_normal_mode(),
        }
    }

    fn scan_normal_mode(&mut self) -> Token {
        // Loop to skip whitespace and comments (handles consecutive comment lines)
        loop {
            self.skip_whitespace();
            if self.ch == '/' && self.peek_char() == '/' {
                while self.ch != '\n' && self.ch != '\0' { self.read_char(); }
                continue;
            }
            break;
        }
        self.mark_span();
        let tok = match self.ch {
            '=' => {
                if self.peek_char() == '>' { self.read_char(); Token::Arrow }
                else if self.peek_char() == '=' { self.read_char(); Token::Equal }
                else { Token::Assign }
            }
            '!' => {
                if self.peek_char() == '=' { self.read_char(); Token::NotEqual }
                else { Token::Bang }
            }
            '+' => {
                if self.peek_char() == '+' { self.read_char(); Token::PlusPlus }
                else { Token::Plus }
            }
            '-' => {
                if self.peek_char() == '-' { self.read_char(); Token::MinusMinus }
                else { Token::Minus }
            }
            '*' => Token::Star,
            '/' => Token::Slash,
            '%' => Token::Modulo,
            '(' => Token::LParen,
            ')' => Token::RParen,
            '{' => Token::LBrace,
            '}' => Token::RBrace,
            '[' => Token::LBracket,
            ']' => Token::RBracket,
            ';' => Token::Semicolon,
            ',' => Token::Comma,
            ':' => Token::Colon,
            '.' => Token::Dot,
            '>' => {
                if self.peek_char() == '=' { self.read_char(); Token::GTE }
                else { Token::GT }
            }
            '<' => {
                if self.peek_char() == '=' { self.read_char(); Token::LTE }
                else { Token::LT }
            }
            '$' => Token::Dollar,
            '"' => {
                self.mode = LexerMode::String;
                self.read_char();
                return self.scan_string_mode();
            }
            '\0' => Token::EOF,
            _ => {
                if self.ch.is_alphabetic() || self.ch == '_' {
                    let ident = self.read_identifier();
                    return Token::lookup_identifier(&ident);
                } else if self.ch.is_numeric() {
                    return Token::Number(self.read_number());
                } else {
                    Token::EOF
                }
            }
        };
        self.read_char();
        tok
    }

    fn scan_string_mode(&mut self) -> Token {
        self.mark_span();
        let mut content = String::new();
        while self.ch != '\0' {
            if self.ch == '"' {
                self.mode = LexerMode::Normal;
                self.read_char();
                return Token::String(content);
            }
            if self.ch == '$' && self.peek_char() == '{' {
                if !content.is_empty() {
                    return Token::String(content);
                }
                self.read_char(); // $
                self.read_char(); // {
                self.mode = LexerMode::Normal;
                return Token::InterpolationStart;
            }
            // Escape sequences
            if self.ch == '\\' {
                self.read_char();
                match self.ch {
                    'n' => content.push('\n'),
                    't' => content.push('\t'),
                    'r' => content.push('\r'),
                    '\\' => content.push('\\'),
                    '"' => content.push('"'),
                    '$' => content.push('$'),
                    _ => {
                        content.push('\\');
                        content.push(self.ch);
                    }
                }
                self.read_char();
                continue;
            }
            content.push(self.ch);
            self.read_char();
        }
        Token::EOF
    }

    fn read_identifier(&mut self) -> String {
        let pos = self.position;
        while self.ch.is_alphanumeric() || self.ch == '_' { self.read_char(); }
        self.input[pos..self.position].iter().collect()
    }

    fn read_number(&mut self) -> f64 {
        let pos = self.position;
        while self.ch.is_numeric() || self.ch == '.' { self.read_char(); }
        let s: String = self.input[pos..self.position].iter().collect();
        s.parse().unwrap_or(0.0)
    }

    fn skip_whitespace(&mut self) {
        while self.ch.is_whitespace() { self.read_char(); }
    }


    pub fn resume_string(&mut self) {
        self.mode = LexerMode::String;
    }

    /// Get source line text by line number (1-indexed).
    pub fn get_source_line(&self, line: usize) -> Option<String> {
        self.source.lines().nth(line.saturating_sub(1)).map(|s| s.to_string())
    }
}
