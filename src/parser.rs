use crate::ast::{Expr, Program, SignalDecl, SignalKind, Statement};
use crate::error::{CompileError, Result};
use crate::lexer::{Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    fn current(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or_else(|| {
            self.tokens.last().expect("tokens should never be empty")
        })
    }

    fn advance(&mut self) -> &Token {
        let _tok = self.current();
        self.pos += 1;
        self.tokens.get(self.pos - 1).unwrap()
    }

    fn expect(&mut self, expected: &TokenKind) -> Result<&Token> {
        let tok = self.current();
        if std::mem::discriminant(&tok.kind) == std::mem::discriminant(expected) {
            self.advance();
            Ok(self.tokens.get(self.pos - 1).unwrap())
        } else {
            Err(CompileError::ParserError {
                line: tok.line,
                col: tok.col,
                message: format!("expected {}, got {}", expected, tok.kind),
            })
        }
    }

    fn match_kind(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.current().kind) == std::mem::discriminant(kind)
    }

    fn parse_expr(&mut self) -> Result<Expr> {
        self.parse_additive()
    }

    fn parse_additive(&mut self) -> Result<Expr> {
        let mut left = self.parse_multiplicative()?;
        loop {
            if self.match_kind(&TokenKind::Plus) {
                self.advance();
                let right = self.parse_multiplicative()?;
                left = Expr::Add(Box::new(left), Box::new(right));
            } else if self.match_kind(&TokenKind::Minus) {
                self.advance();
                let right = self.parse_multiplicative()?;
                left = Expr::Sub(Box::new(left), Box::new(right));
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            if self.match_kind(&TokenKind::Star) {
                self.advance();
                let right = self.parse_unary()?;
                left = Expr::Mul(Box::new(left), Box::new(right));
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        if self.match_kind(&TokenKind::Minus) {
            self.advance();
            let expr = self.parse_primary()?;
            Ok(Expr::Neg(Box::new(expr)))
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        let tok = self.current().clone();
        match &tok.kind {
            TokenKind::BigInt(n) => {
                self.advance();
                Ok(Expr::Const(n.clone()))
            }
            TokenKind::Ident(s) => {
                self.advance();
                Ok(Expr::Var(s.clone()))
            }
            TokenKind::ParenOpen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::ParenClose)?;
                Ok(expr)
            }
            _ => Err(CompileError::ParserError {
                line: tok.line,
                col: tok.col,
                message: format!("expected expression, got {}", tok.kind),
            }),
        }
    }

    fn parse_statement(&mut self) -> Result<Statement> {
        let tok = self.current().clone();

        match &tok.kind {
            TokenKind::KeywordSignal => {
                self.advance();
                let next = self.current().clone();

                let kind = match &next.kind {
                    TokenKind::KeywordInput => {
                        self.advance();
                        SignalKind::Input
                    }
                    TokenKind::KeywordOutput => {
                        self.advance();
                        SignalKind::Output
                    }
                    _ => SignalKind::Intermediate,
                };

                let name_tok = self.expect(&TokenKind::Ident(String::new()))?;
                let name = match &name_tok.kind {
                    TokenKind::Ident(s) => s.clone(),
                    _ => unreachable!(),
                };
                self.expect(&TokenKind::Semi)?;

                Ok(Statement::SignalDecl(SignalDecl { kind, name }))
            }
            TokenKind::KeywordAssertBool => {
                self.advance();
                self.expect(&TokenKind::ParenOpen)?;
                let name_tok = self.expect(&TokenKind::Ident(String::new()))?;
                let name = match &name_tok.kind {
                    TokenKind::Ident(s) => s.clone(),
                    _ => unreachable!(),
                };
                self.expect(&TokenKind::ParenClose)?;
                self.expect(&TokenKind::Semi)?;
                Ok(Statement::AssertBool(name))
            }
            TokenKind::KeywordIf => {
                self.advance();
                let cond_tok = self.expect(&TokenKind::Ident(String::new()))?;
                let condition = match &cond_tok.kind {
                    TokenKind::Ident(s) => s.clone(),
                    _ => unreachable!(),
                };
                self.expect(&TokenKind::KeywordThen)?;
                let then_stmt = self.parse_statement()?;
                let else_stmt = if self.match_kind(&TokenKind::KeywordElse) {
                    self.advance();
                    Some(Box::new(self.parse_statement()?))
                } else {
                    None
                };
                Ok(Statement::Conditional {
                    condition,
                    then_stmt: Box::new(then_stmt),
                    else_stmt,
                })
            }
            TokenKind::Ident(_) => {
                let name_tok = self.advance().clone();
                let name = match &name_tok.kind {
                    TokenKind::Ident(s) => s.clone(),
                    _ => unreachable!(),
                };

                let next = self.current().clone();
                match &next.kind {
                    TokenKind::LArrow => {
                        self.advance();
                        let value = self.parse_expr()?;
                        self.expect(&TokenKind::Semi)?;
                        Ok(Statement::Assign { target: name, value })
                    }
                    TokenKind::TripleEqual => {
                        self.advance();
                        let rhs = self.parse_expr()?;
                        self.expect(&TokenKind::Semi)?;
                        let lhs = Expr::Var(name);
                        Ok(Statement::Constraint { lhs, rhs })
                    }
                    _ => Err(CompileError::ParserError {
                        line: next.line,
                        col: next.col,
                        message: format!(
                            "expected '<==' or '===' after identifier, got {}",
                            next.kind
                        ),
                    }),
                }
            }
            _ => Err(CompileError::ParserError {
                line: tok.line,
                col: tok.col,
                message: format!("unexpected token: {}", tok.kind),
            }),
        }
    }

    pub fn parse(&mut self) -> Result<Program> {
        let mut statements = Vec::new();
        while !self.match_kind(&TokenKind::Eof) {
            statements.push(self.parse_statement()?);
        }
        Ok(Program { statements })
    }
}
