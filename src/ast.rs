use num_bigint::BigUint;
use std::fmt;

#[derive(Debug, Clone)]
pub enum SignalKind {
    Input,
    Output,
    Intermediate,
}

#[derive(Debug, Clone)]
pub struct SignalDecl {
    pub kind: SignalKind,
    pub name: String,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Const(BigUint),
    Var(String),
    Add(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum Statement {
    SignalDecl(SignalDecl),
    Assign {
        target: String,
        value: Expr,
    },
    Constraint {
        lhs: Expr,
        rhs: Expr,
    },
    AssertBool(String),
    Conditional {
        condition: String,
        then_stmt: Box<Statement>,
        else_stmt: Option<Box<Statement>>,
    },
}

#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Statement>,
}

impl fmt::Display for SignalKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SignalKind::Input => write!(f, "input"),
            SignalKind::Output => write!(f, "output"),
            SignalKind::Intermediate => write!(f, "intermediate"),
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Const(n) => write!(f, "{}", n),
            Expr::Var(s) => write!(f, "{}", s),
            Expr::Add(a, b) => write!(f, "({} + {})", a, b),
            Expr::Mul(a, b) => write!(f, "({} * {})", a, b),
            Expr::Sub(a, b) => write!(f, "({} - {})", a, b),
            Expr::Neg(a) => write!(f, "(-{})", a),
        }
    }
}

impl fmt::Display for Statement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Statement::SignalDecl(s) => match s.kind {
                SignalKind::Input => write!(f, "signal input {};", s.name),
                SignalKind::Output => write!(f, "signal output {};", s.name),
                SignalKind::Intermediate => write!(f, "signal {};", s.name),
            },
            Statement::Assign { target, value } => write!(f, "{} <== {};", target, value),
            Statement::Constraint { lhs, rhs } => write!(f, "{} === {};", lhs, rhs),
            Statement::AssertBool(name) => write!(f, "assert_bool({});", name),
            Statement::Conditional {
                condition,
                then_stmt,
                else_stmt,
            } => {
                write!(f, "if {} then {}", condition, then_stmt)?;
                if let Some(e) = else_stmt {
                    write!(f, " else {}", e)?;
                }
                Ok(())
            }
        }
    }
}
