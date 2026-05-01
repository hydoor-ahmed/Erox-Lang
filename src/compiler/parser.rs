use crate::compiler::lexer::{Lexer, Token, Span};

#[derive(Debug, PartialEq, Clone)]
pub enum Statement {
    Let { name: String, value: Expression },
    Const { name: String, value: Expression },
    Return(Expression),
    Block(Vec<Statement>),
    Expression(Expression),
    Function { name: String, params: Vec<String>, body: Vec<Statement>, is_async: bool },
    While { condition: Expression, body: Vec<Statement> },
    For { init: Box<Statement>, condition: Expression, update: Box<Statement>, body: Vec<Statement> },
    If { condition: Expression, consequence: Vec<Statement>, alternative: Option<Vec<Statement>> },
    Assign { name: String, value: Expression },
    Import { path: String, item: Option<String> },
}

#[derive(Debug, PartialEq, Clone)]
pub enum Expression {
    Identifier(String),
    Number(f64),
    String(String),
    Boolean(bool),
    Null,
    Shell(Box<Expression>),
    Binary { left: Box<Expression>, operator: Token, right: Box<Expression> },
    FunctionLiteral { params: Vec<String>, body: Vec<Statement> },
    Call { function: Box<Expression>, arguments: Vec<Expression> },
    Prefix { operator: Token, right: Box<Expression> },
    Postfix { operator: Token, operand: String },
    InterpolatedString(Vec<Expression>),
    Array(Vec<Expression>),
    Object(Vec<(String, Expression)>),
    Index { object: Box<Expression>, index: Box<Expression> },
    Member { object: Box<Expression>, field: String },
    Await(Box<Expression>),
}

#[derive(PartialOrd, PartialEq, Clone, Copy)]
pub enum Precedence {
    LOWEST,
    EQUALS,
    LESSGREATER,
    SUM,
    PRODUCT,
    PREFIX,
    CALL,
    INDEX,
}

pub struct Parser {
    lexer: Lexer,
    pub cur_token: Token,
    pub peek_token: Token,
    pub errors: Vec<String>,
    pub spans: Vec<Span>,
    pub cur_span: Span,
}

impl Parser {
    pub fn new(mut lexer: Lexer) -> Self {
        let cur_token = lexer.next_token();
        let cur_span = lexer.last_span;
        let peek_token = lexer.next_token();
        Parser { lexer, cur_token, peek_token, errors: vec![], spans: vec![], cur_span }
    }

    pub fn next_token(&mut self) {
        self.cur_token = self.peek_token.clone();
        self.cur_span = self.lexer.last_span;
        self.peek_token = self.lexer.next_token();
    }

    fn error_at_current(&mut self, msg: &str) {
        let span = self.cur_span;
        self.spans.push(span);
        self.errors.push(format!("[{}:{}] {}", span.line, span.col, msg));
    }

    pub fn parse_program(&mut self) -> Vec<Statement> {
        let mut program = vec![];
        while self.cur_token != Token::EOF {
            if self.cur_token == Token::Semicolon {
                self.next_token();
                continue;
            }
            if let Some(stmt) = self.parse_statement() {
                program.push(stmt);
            } else {
                // If parse_statement returned None, advance to avoid getting stuck
                self.next_token();
            }
        }
        program
    }

    fn parse_statement(&mut self) -> Option<Statement> {
        match self.cur_token.clone() {
            Token::Import => self.parse_import_statement(),
            Token::Let => self.parse_let_statement(),
            Token::Const => self.parse_const_statement(),
            Token::Return => self.parse_return_statement(),
            Token::While => self.parse_while_statement(),
            Token::For => self.parse_for_statement(),
            Token::If => self.parse_if_statement(),
            Token::LBrace => {
                let stmts = self.parse_block_statements();
                if self.cur_token == Token::RBrace { self.next_token(); }
                Some(Statement::Block(stmts))
            }
            Token::Fn => self.parse_function_statement(false),
            Token::Async => {
                // async fn name(...) { ... }
                if self.peek_token == Token::Fn {
                    self.next_token(); // consume async
                    self.parse_function_statement(true)
                } else {
                    Some(self.parse_expression_statement())
                }
            }
            // CRITICAL FIX: Detect `identifier = expr` as assignment
            Token::Identifier(ref name) if self.peek_token == Token::Assign => {
                let name = name.clone();
                self.parse_assign_statement(name)
            }
            _ => Some(self.parse_expression_statement()),
        }
    }

    fn parse_assign_statement(&mut self, name: String) -> Option<Statement> {
        self.next_token(); // skip identifier
        self.next_token(); // skip =
        let value = self.parse_expression(Precedence::LOWEST);
        if self.peek_token == Token::Semicolon {
            self.next_token();
            self.next_token();
        } else {
            self.next_token();
        }
        Some(Statement::Assign { name, value })
    }

    fn parse_expression_statement(&mut self) -> Statement {
        let expr = self.parse_expression(Precedence::LOWEST);
        // Handle postfix ++ / -- as statements
        if self.peek_token == Token::Semicolon {
            self.next_token(); // consume the semicolon into cur_token
            self.next_token(); // advance past semicolon so cur_token is ready for next statement
        } else {
            self.next_token(); // advance past the expression's last token
        }
        Statement::Expression(expr)
    }

    fn parse_expression(&mut self, precedence: Precedence) -> Expression {
        let mut left_exp = match self.cur_token.clone() {
            Token::Identifier(s) => {
                // Check for postfix ++ or --
                if self.peek_token == Token::PlusPlus {
                    self.next_token(); // consume ++
                    Expression::Postfix { operator: Token::PlusPlus, operand: s }
                } else if self.peek_token == Token::MinusMinus {
                    self.next_token(); // consume --
                    Expression::Postfix { operator: Token::MinusMinus, operand: s }
                } else {
                    Expression::Identifier(s)
                }
            }
            Token::Number(n) => Expression::Number(n),
            Token::String(s) => self.parse_string_with_interpolation(s),
            Token::InterpolationStart => self.parse_string_with_interpolation("".to_string()),
            Token::Dollar => self.parse_shell_expression(),
            Token::True => Expression::Boolean(true),
            Token::False => Expression::Boolean(false),
            Token::Minus | Token::Bang => self.parse_prefix_expression(),
            Token::LParen => self.parse_paren_or_arrow_expression(),
            Token::LBracket => self.parse_array_literal(),
            Token::LBrace => self.parse_object_literal(),
            Token::Await => self.parse_await_expression(),
            _ => {
                self.error_at_current(&format!("No prefix parse function for {:?}", self.cur_token));
                Expression::Null
            }
        };

        while self.peek_token != Token::EOF && precedence < self.peek_precedence() {
            match self.peek_token {
                Token::Plus | Token::Minus | Token::Star | Token::Slash | Token::Modulo |
                Token::Equal | Token::NotEqual | Token::LT | Token::GT |
                Token::LTE | Token::GTE => {
                    self.next_token();
                    left_exp = self.parse_infix_expression(left_exp);
                }
                Token::LParen => {
                    self.next_token();
                    left_exp = self.parse_call_expression(left_exp);
                }
                Token::LBracket => {
                    self.next_token();
                    left_exp = self.parse_index_expression(left_exp);
                }
                Token::Dot => {
                    self.next_token();
                    left_exp = self.parse_member_expression(left_exp);
                }
                _ => return left_exp,
            }
        }
        left_exp
    }

    // ── Array Literal ──────────────────────────────────────────
    fn parse_array_literal(&mut self) -> Expression {
        // cur_token is `[`
        let mut elements = vec![];
        if self.peek_token == Token::RBracket {
            self.next_token(); // consume ]
            return Expression::Array(elements);
        }
        self.next_token(); // move past [
        elements.push(self.parse_expression(Precedence::LOWEST));
        while self.peek_token == Token::Comma {
            self.next_token(); // consume comma
            self.next_token(); // move to next expression
            elements.push(self.parse_expression(Precedence::LOWEST));
        }
        if !self.expect_peek(Token::RBracket) {
            self.error_at_current("Expected `]` to close array literal");
        }
        Expression::Array(elements)
    }

    // ── Object Literal ─────────────────────────────────────────
    fn parse_object_literal(&mut self) -> Expression {
        // cur_token is `{`
        // Distinguish from block: if next token is a string followed by :, it's an object
        let mut pairs = vec![];
        if self.peek_token == Token::RBrace {
            self.next_token(); // consume }
            return Expression::Object(pairs);
        }
        self.next_token(); // move past {
        loop {
            // key must be a string or identifier
            let key = match self.cur_token.clone() {
                Token::String(s) => s,
                Token::Identifier(s) => s,
                _ => {
                    self.error_at_current(&format!("Expected string or identifier as object key, got {:?}", self.cur_token));
                    break;
                }
            };
            if !self.expect_peek(Token::Colon) {
                self.error_at_current("Expected `:` after object key");
                break;
            }
            self.next_token(); // move past :
            let value = self.parse_expression(Precedence::LOWEST);
            pairs.push((key, value));

            if self.peek_token == Token::Comma {
                self.next_token(); // consume comma
                self.next_token(); // move to next key
            } else {
                break;
            }
        }
        if !self.expect_peek(Token::RBrace) {
            self.error_at_current("Expected `}` to close object literal");
        }
        Expression::Object(pairs)
    }

    // ── Index Expression ────────────────────────────────────────
    fn parse_index_expression(&mut self, left: Expression) -> Expression {
        // cur_token is `[`
        self.next_token(); // move past [
        let index = self.parse_expression(Precedence::LOWEST);
        if !self.expect_peek(Token::RBracket) {
            self.error_at_current("Expected `]` to close index expression");
        }
        Expression::Index { object: Box::new(left), index: Box::new(index) }
    }

    // ── Member Expression ───────────────────────────────────────
    fn parse_member_expression(&mut self, left: Expression) -> Expression {
        // cur_token is `.`
        self.next_token(); // move to field name
        if let Token::Identifier(field) = self.cur_token.clone() {
            Expression::Member { object: Box::new(left), field }
        } else {
            self.error_at_current(&format!("Expected field name after `.`, got {:?}", self.cur_token));
            Expression::Null
        }
    }

    // ── Await Expression ────────────────────────────────────────
    fn parse_await_expression(&mut self) -> Expression {
        // cur_token is `await`
        self.next_token(); // move to the expression
        let expr = self.parse_expression(Precedence::PREFIX);
        Expression::Await(Box::new(expr))
    }

    // ── String Interpolation ────────────────────────────────────
    fn parse_string_with_interpolation(&mut self, head: String) -> Expression {
        let mut parts = vec![Expression::String(head.clone())];
        
        while self.cur_token == Token::InterpolationStart || self.peek_token == Token::InterpolationStart {
            if self.cur_token != Token::InterpolationStart {
                self.next_token();
            }
            
            self.next_token();
            parts.push(self.parse_expression(Precedence::LOWEST));
            
            if self.peek_token == Token::RBrace {
                self.lexer.resume_string();
                self.next_token();
            } else {
                while self.cur_token != Token::RBrace && self.cur_token != Token::EOF {
                    self.next_token();
                }
                self.lexer.resume_string();
            }
            
            self.next_token(); 
            
            if let Token::String(ref s) = self.cur_token {
                parts.push(Expression::String(s.clone()));
            }
            
            if self.lexer.mode == crate::compiler::lexer::LexerMode::Normal && self.cur_token != Token::InterpolationStart {
                break;
            }
        }
        
        if parts.len() == 1 {
            return Expression::String(head);
        }
        Expression::InterpolatedString(parts)
    }

    // ── If Statement ────────────────────────────────────────────
    fn parse_if_statement(&mut self) -> Option<Statement> {
        self.next_token();
        let condition = self.parse_expression(Precedence::LOWEST);

        if !self.expect_peek(Token::LBrace) { return None; }
        let consequence = self.parse_block_statements();
        if self.cur_token == Token::RBrace { self.next_token(); }

        let mut alternative = None;
        if self.cur_token == Token::Else {
            if self.peek_token == Token::If {
                self.next_token();
                alternative = Some(vec![self.parse_if_statement().unwrap()]);
            } else if self.peek_token == Token::LBrace {
                self.next_token();
                alternative = Some(self.parse_block_statements());
                if self.cur_token == Token::RBrace { self.next_token(); }
            }
        }

        Some(Statement::If { condition, consequence, alternative })
    }

    // ── Block Statements ────────────────────────────────────────
    fn parse_block_statements(&mut self) -> Vec<Statement> {
        let mut stmts = vec![];
        self.next_token();

        while self.cur_token != Token::RBrace && self.cur_token != Token::EOF {
            if self.cur_token == Token::Semicolon {
                self.next_token();
                continue;
            }
            if let Some(stmt) = self.parse_statement() {
                stmts.push(stmt);
            } else {
                self.next_token();
            }
        }
        stmts
    }

    // ── Let Statement ───────────────────────────────────────────
    fn parse_let_statement(&mut self) -> Option<Statement> {
        self.next_token();
        if let Token::Identifier(name) = self.cur_token.clone() {
            if !self.expect_peek(Token::Assign) { return None; }
            self.next_token();
            let value = self.parse_expression(Precedence::LOWEST);
            if self.peek_token == Token::Semicolon { self.next_token(); }
            self.next_token();
            return Some(Statement::Let { name, value });
        }
        None
    }

    // ── Const Statement ─────────────────────────────────────────
    fn parse_const_statement(&mut self) -> Option<Statement> {
        self.next_token();
        if let Token::Identifier(name) = self.cur_token.clone() {
            if !self.expect_peek(Token::Assign) { return None; }
            self.next_token();
            let value = self.parse_expression(Precedence::LOWEST);
            if self.peek_token == Token::Semicolon { self.next_token(); }
            self.next_token();
            return Some(Statement::Const { name, value });
        }
        None
    }

    // ── Return Statement ────────────────────────────────────────
    fn parse_return_statement(&mut self) -> Option<Statement> {
        self.next_token();
        let expr = self.parse_expression(Precedence::LOWEST);
        if self.peek_token == Token::Semicolon {
            self.next_token();
            self.next_token();
        } else {
            self.next_token();
        }
        Some(Statement::Return(expr))
    }

    // ── While Statement ─────────────────────────────────────────
    fn parse_while_statement(&mut self) -> Option<Statement> {
        self.next_token();
        let condition = self.parse_expression(Precedence::LOWEST);
        if !self.expect_peek(Token::LBrace) { return None; }
        let body = self.parse_block_statements();
        if self.cur_token == Token::RBrace { self.next_token(); }
        Some(Statement::While { condition, body })
    }

    // ── For Statement ───────────────────────────────────────────
    // for (init; condition; update) { body }
    fn parse_for_statement(&mut self) -> Option<Statement> {
        // cur_token is `for`
        if !self.expect_peek(Token::LParen) {
            self.error_at_current("Expected `(` after `for`");
            return None;
        }
        self.next_token(); // move past (

        // Parse init statement
        let init = if self.cur_token == Token::Let {
            self.parse_let_statement()?
        } else if self.cur_token == Token::Semicolon {
            self.next_token(); // skip empty init
            Statement::Expression(Expression::Null)
        } else if let Token::Identifier(ref name) = self.cur_token.clone() {
            if self.peek_token == Token::Assign {
                let name = name.clone();
                // Parse as assignment but don't auto-advance past semicolon
                self.next_token(); // skip identifier
                self.next_token(); // skip =
                let value = self.parse_expression(Precedence::LOWEST);
                if self.cur_token == Token::Semicolon || self.peek_token == Token::Semicolon {
                    if self.peek_token == Token::Semicolon { self.next_token(); }
                    self.next_token();
                }
                Statement::Assign { name, value }
            } else {
                let expr = self.parse_expression(Precedence::LOWEST);
                if self.peek_token == Token::Semicolon { self.next_token(); }
                self.next_token();
                Statement::Expression(expr)
            }
        } else {
            let expr = self.parse_expression(Precedence::LOWEST);
            if self.peek_token == Token::Semicolon { self.next_token(); }
            self.next_token();
            Statement::Expression(expr)
        };

        // cur_token should be past the first semicolon now
        // Parse condition
        let condition = if self.cur_token == Token::Semicolon {
            self.next_token();
            Expression::Boolean(true) // empty condition = infinite loop
        } else {
            let cond = self.parse_expression(Precedence::LOWEST);
            if self.peek_token == Token::Semicolon { self.next_token(); }
            self.next_token(); // advance past semicolon
            cond
        };

        // Parse update — leave cur_token so that peek is `)` after update
        let update = if self.cur_token == Token::RParen {
            Statement::Expression(Expression::Null)
        } else if let Token::Identifier(ref name) = self.cur_token.clone() {
            if self.peek_token == Token::Assign {
                let name = name.clone();
                self.next_token(); // cur = `=`
                self.next_token(); // cur = start of value expr
                let value = self.parse_expression(Precedence::LOWEST);
                // After parse_expression, cur is at last token of expr, peek should be `)`
                Statement::Assign { name, value }
            } else if self.peek_token == Token::PlusPlus {
                let name = name.clone();
                self.next_token(); // cur = `++`, peek = `)`
                Statement::Expression(Expression::Postfix { operator: Token::PlusPlus, operand: name })
            } else if self.peek_token == Token::MinusMinus {
                let name = name.clone();
                self.next_token(); // cur = `--`, peek = `)`
                Statement::Expression(Expression::Postfix { operator: Token::MinusMinus, operand: name })
            } else {
                let expr = self.parse_expression(Precedence::LOWEST);
                Statement::Expression(expr)
            }
        } else {
            let expr = self.parse_expression(Precedence::LOWEST);
            Statement::Expression(expr)
        };

        // Expect )
        if !self.expect_peek(Token::RParen) {
            self.error_at_current("Expected `)` after for loop clauses");
            return None;
        }

        // Expect {
        if !self.expect_peek(Token::LBrace) {
            self.error_at_current("Expected `{` for for-loop body");
            return None;
        }
        let body = self.parse_block_statements();
        if self.cur_token == Token::RBrace { self.next_token(); }

        Some(Statement::For {
            init: Box::new(init),
            condition,
            update: Box::new(update),
            body,
        })
    }

    // ── Function Statement ──────────────────────────────────────
    fn parse_function_statement(&mut self, is_async: bool) -> Option<Statement> {
        self.next_token();
        if let Token::Identifier(name) = self.cur_token.clone() {
            if !self.expect_peek(Token::LParen) { return None; }
            let params = self.parse_function_parameters();
            if !self.expect_peek(Token::LBrace) { return None; }
            let body = self.parse_block_statements();
            if self.cur_token == Token::RBrace { self.next_token(); }
            return Some(Statement::Function { name, params, body, is_async });
        }
        None
    }

    fn parse_function_parameters(&mut self) -> Vec<String> {
        let mut params = vec![];
        self.next_token();
        if self.cur_token == Token::RParen { return params; }
        if let Token::Identifier(ref s) = self.cur_token { params.push(s.clone()); }
        while self.peek_token == Token::Comma {
            self.next_token(); self.next_token();
            if let Token::Identifier(ref s) = self.cur_token { params.push(s.clone()); }
        }
        self.expect_peek(Token::RParen);
        params
    }

    fn parse_call_expression(&mut self, function: Expression) -> Expression {
        let mut arguments = vec![];
        if self.peek_token == Token::RParen {
            self.next_token();
            return Expression::Call { function: Box::new(function), arguments };
        }
        self.next_token();
        arguments.push(self.parse_expression(Precedence::LOWEST));
        while self.peek_token == Token::Comma {
            self.next_token(); self.next_token();
            arguments.push(self.parse_expression(Precedence::LOWEST));
        }
        self.expect_peek(Token::RParen);
        Expression::Call { function: Box::new(function), arguments }
    }

    fn parse_infix_expression(&mut self, left: Expression) -> Expression {
        let operator = self.cur_token.clone();
        let precedence = self.cur_precedence();
        self.next_token();
        let right = self.parse_expression(precedence);
        Expression::Binary { left: Box::new(left), operator, right: Box::new(right) }
    }

    fn parse_prefix_expression(&mut self) -> Expression {
        let operator = self.cur_token.clone();
        self.next_token();
        let right = self.parse_expression(Precedence::PREFIX);
        Expression::Prefix { operator, right: Box::new(right) }
    }

    fn parse_shell_expression(&mut self) -> Expression {
        self.next_token();
        let expr = self.parse_expression(Precedence::LOWEST);
        if self.peek_token == Token::Dollar {
            self.next_token();
        }
        Expression::Shell(Box::new(expr))
    }

    fn parse_import_statement(&mut self) -> Option<Statement> {
        self.next_token();
        if let Token::String(path) = self.cur_token.clone() {
            self.next_token();
            return Some(Statement::Import { path, item: None });
        }
        None
    }

    fn parse_paren_or_arrow_expression(&mut self) -> Expression {
        let mut lexer_clone = self.lexer.clone();
        let mut paren_depth = 1;
        let mut is_arrow = false;
        let mut next = lexer_clone.next_token();
        while next != Token::EOF {
            if next == Token::LParen { paren_depth += 1; }
            else if next == Token::RParen {
                paren_depth -= 1;
                if paren_depth == 0 {
                    if lexer_clone.next_token() == Token::Arrow { is_arrow = true; }
                    break;
                }
            }
            next = lexer_clone.next_token();
        }

        if is_arrow {
            self.next_token(); 
            let params = self.parse_function_parameters_inline();
            self.expect_peek(Token::RParen);
            self.expect_peek(Token::Arrow);
            self.next_token(); 
            let body = if self.cur_token == Token::LBrace {
                self.parse_block_statements()
            } else {
                vec![Statement::Return(self.parse_expression(Precedence::LOWEST))]
            };
            Expression::FunctionLiteral { params, body }
        } else {
            self.next_token();
            let expr = self.parse_expression(Precedence::LOWEST);
            self.expect_peek(Token::RParen);
            expr
        }
    }

    fn parse_function_parameters_inline(&mut self) -> Vec<String> {
        let mut params = vec![];
        if self.cur_token == Token::RParen { return params; }
        if let Token::Identifier(ref s) = self.cur_token { params.push(s.clone()); }
        while self.peek_token == Token::Comma {
            self.next_token(); self.next_token();
            if let Token::Identifier(ref s) = self.cur_token { params.push(s.clone()); }
        }
        params
    }

    fn peek_precedence(&self) -> Precedence {
        match self.peek_token {
            Token::Equal | Token::NotEqual => Precedence::EQUALS,
            Token::LT | Token::GT | Token::LTE | Token::GTE => Precedence::LESSGREATER,
            Token::Plus | Token::Minus => Precedence::SUM,
            Token::Star | Token::Slash | Token::Modulo => Precedence::PRODUCT,
            Token::LParen => Precedence::CALL,
            Token::LBracket | Token::Dot => Precedence::INDEX,
            _ => Precedence::LOWEST,
        }
    }

    fn cur_precedence(&self) -> Precedence {
        match self.cur_token {
            Token::Equal | Token::NotEqual => Precedence::EQUALS,
            Token::LT | Token::GT | Token::LTE | Token::GTE => Precedence::LESSGREATER,
            Token::Plus | Token::Minus => Precedence::SUM,
            Token::Star | Token::Slash | Token::Modulo => Precedence::PRODUCT,
            Token::LParen => Precedence::CALL,
            Token::LBracket | Token::Dot => Precedence::INDEX,
            _ => Precedence::LOWEST,
        }
    }

    fn expect_peek(&mut self, t: Token) -> bool {
        if self.peek_token == t {
            self.next_token();
            true
        } else {
            self.error_at_current(&format!("Expected {:?}, got {:?} instead", t, self.peek_token));
            false
        }
    }
}
