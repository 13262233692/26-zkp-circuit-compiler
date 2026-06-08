use thiserror::Error;

#[derive(Error, Debug)]
pub enum CompileError {
    #[error("Lexer error at line {line}, col {col}: {message}")]
    LexerError { line: usize, col: usize, message: String },

    #[error("Parser error at line {line}, col {col}: {message}")]
    ParserError { line: usize, col: usize, message: String },

    #[error("R1CS error: {message}")]
    R1csError { message: String },

    #[error("Serialization error: {message}")]
    SerializeError { message: String },

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, CompileError>;
