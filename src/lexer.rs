use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    KeywordSignal,
    KeywordInput,
    KeywordOutput,
    KeywordAssertBool,
    KeywordIf,
    KeywordThen,
    KeywordElse,
    Ident(String),
    BigInt(num_bigint::BigUint),
    Plus,
    Star,
    Minus,
    Equal,
    DoubleEqual,
    TripleEqual,
    LArrow,
    ParenOpen,
    ParenClose,
    Semi,
    Comma,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub col: usize,
}

impl Token {
    pub fn new(kind: TokenKind, line: usize, col: usize) -> Self {
        Token { kind, line, col }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::KeywordSignal => write!(f, "signal"),
            TokenKind::KeywordInput => write!(f, "input"),
            TokenKind::KeywordOutput => write!(f, "output"),
            TokenKind::KeywordAssertBool => write!(f, "assert_bool"),
            TokenKind::KeywordIf => write!(f, "if"),
            TokenKind::KeywordThen => write!(f, "then"),
            TokenKind::KeywordElse => write!(f, "else"),
            TokenKind::Ident(s) => write!(f, "{}", s),
            TokenKind::BigInt(n) => write!(f, "{}", n),
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Star => write!(f, "*"),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Equal => write!(f, "="),
            TokenKind::DoubleEqual => write!(f, "=="),
            TokenKind::TripleEqual => write!(f, "==="),
            TokenKind::LArrow => write!(f, "<=="),
            TokenKind::ParenOpen => write!(f, "("),
            TokenKind::ParenClose => write!(f, ")"),
            TokenKind::Semi => write!(f, ";"),
            TokenKind::Comma => write!(f, ","),
            TokenKind::Eof => write!(f, "<eof>"),
        }
    }
}

pub struct Lexer {
    source: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Lexer {
            source: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn peek(&self) -> Option<char> {
        self.source.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.source.get(self.pos).copied();
        if let Some(c) = ch {
            self.pos += 1;
            if c == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
        ch
    }

    fn peek_ahead(&self, offset: usize) -> Option<char> {
        self.source.get(self.pos + offset).copied()
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.peek() {
                Some(' ') | Some('\t') | Some('\r') | Some('\n') => {
                    self.advance();
                }
                Some('/') if self.peek_ahead(1) == Some('/') => {
                    while self.peek().map_or(false, |c| c != '\n') {
                        self.advance();
                    }
                }
                _ => break,
            }
        }
    }

    pub fn tokenize(&mut self) -> crate::error::Result<Vec<Token>> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            let line = self.line;
            let col = self.col;
            match self.peek() {
                None => {
                    tokens.push(Token::new(TokenKind::Eof, line, col));
                    break;
                }
                Some(ch) => {
                    let kind = match ch {
                        '+' => {
                            self.advance();
                            TokenKind::Plus
                        }
                        '*' => {
                            self.advance();
                            TokenKind::Star
                        }
                        '-' => {
                            self.advance();
                            TokenKind::Minus
                        }
                        '(' => {
                            self.advance();
                            TokenKind::ParenOpen
                        }
                        ')' => {
                            self.advance();
                            TokenKind::ParenClose
                        }
                        ';' => {
                            self.advance();
                            TokenKind::Semi
                        }
                        ',' => {
                            self.advance();
                            TokenKind::Comma
                        }
                        '=' => {
                            self.advance();
                            if self.peek() == Some('=') {
                                self.advance();
                                if self.peek() == Some('=') {
                                    self.advance();
                                    TokenKind::TripleEqual
                                } else {
                                    TokenKind::DoubleEqual
                                }
                            } else {
                                TokenKind::Equal
                            }
                        }
                        '<' => {
                            self.advance();
                            if self.peek() == Some('=') {
                                self.advance();
                                if self.peek() == Some('=') {
                                    self.advance();
                                    TokenKind::LArrow
                                } else {
                                    return Err(crate::error::CompileError::LexerError {
                                        line,
                                        col,
                                        message: "unexpected token: <=".to_string(),
                                    });
                                }
                            } else {
                                return Err(crate::error::CompileError::LexerError {
                                    line,
                                    col,
                                    message: "unexpected token: <".to_string(),
                                });
                            }
                        }
                        c if c.is_ascii_digit() => {
                            let mut num_str = String::new();
                            while self.peek().map_or(false, |c| c.is_ascii_digit()) {
                                num_str.push(self.advance().unwrap());
                            }
                            let n = num_str.parse::<num_bigint::BigUint>().map_err(|e| {
                                crate::error::CompileError::LexerError {
                                    line,
                                    col,
                                    message: format!("invalid number: {}", e),
                                }
                            })?;
                            TokenKind::BigInt(n)
                        }
                        c if c.is_ascii_alphabetic() || c == '_' => {
                            let mut ident = String::new();
                            while self
                                .peek()
                                .map_or(false, |c| c.is_ascii_alphanumeric() || c == '_')
                            {
                                ident.push(self.advance().unwrap());
                            }
                            match ident.as_str() {
                                "signal" => TokenKind::KeywordSignal,
                                "input" => TokenKind::KeywordInput,
                                "output" => TokenKind::KeywordOutput,
                                "assert_bool" => TokenKind::KeywordAssertBool,
                                "if" => TokenKind::KeywordIf,
                                "then" => TokenKind::KeywordThen,
                                "else" => TokenKind::KeywordElse,
                                _ => TokenKind::Ident(ident),
                            }
                        }
                        _ => {
                            self.advance();
                            return Err(crate::error::CompileError::LexerError {
                                line,
                                col,
                                message: format!("unexpected character: '{}'", ch),
                            });
                        }
                    };
                    tokens.push(Token::new(kind, line, col));
                }
            }
        }
        Ok(tokens)
    }
}
