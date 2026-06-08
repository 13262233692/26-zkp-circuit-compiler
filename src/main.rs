use std::fs;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use num_bigint::BigUint;

use zkp_circuit_compiler::ast::Program;
use zkp_circuit_compiler::error::Result;
use zkp_circuit_compiler::flattener;
use zkp_circuit_compiler::lexer::Lexer;
use zkp_circuit_compiler::parser::Parser as CircuitParser;
use zkp_circuit_compiler::r1cs::bn128_prime;
use zkp_circuit_compiler::serializer;

#[derive(Parser)]
#[command(name = "zkp-circuit-compiler")]
#[command(about = "Hardcore ZKP circuit compiler: source → AST → R1CS → .r1cs binary")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Compile {
        #[arg(help = "Input source file (.zkp)")]
        input: PathBuf,
        #[arg(short, long, help = "Output .r1cs binary file")]
        output: Option<PathBuf>,
        #[arg(long, help = "Print AST")]
        show_ast: bool,
        #[arg(long, help = "Print R1CS summary")]
        show_r1cs: bool,
        #[arg(long, help = "Custom field prime (hex, without 0x prefix)")]
        prime: Option<String>,
    },
    Inspect {
        #[arg(help = "Input .r1cs binary file")]
        input: PathBuf,
    },
    DumpAst {
        #[arg(help = "Input source file (.zkp)")]
        input: PathBuf,
    },
}

fn compile_file(
    input: &PathBuf,
    output: &Option<PathBuf>,
    show_ast: bool,
    show_r1cs: bool,
    custom_prime: &Option<String>,
) -> Result<()> {
    let source = fs::read_to_string(input)?;
    let filename = input.display().to_string();

    eprintln!("[1/4] Lexing {} ...", filename);
    let mut lexer = Lexer::new(&source);
    let tokens = lexer.tokenize()?;

    eprintln!("[2/4] Parsing tokens → AST ...");
    let mut parser = CircuitParser::new(tokens);
    let program: Program = parser.parse()?;

    if show_ast {
        println!("=== AST ===");
        for stmt in &program.statements {
            println!("  {}", stmt);
        }
        println!();
    }

    eprintln!("[3/4] Flattening AST → R1CS (A*B=C constraints) ...");
    let prime: BigUint = match custom_prime {
        Some(hex) => BigUint::parse_bytes(hex.as_bytes(), 16).unwrap_or_else(|| {
            eprintln!("Warning: invalid custom prime, falling back to BN128");
            bn128_prime()
        }),
        None => bn128_prime(),
    };
    let system = flattener::flatten(&program, prime)?;

    if show_r1cs {
        println!("{}", system.display_summary());
    }

    let output_path = match output {
        Some(p) => p.clone(),
        None => input.with_extension("r1cs"),
    };

    eprintln!("[4/4] Serializing R1CS → {} ...", output_path.display());
    let mut out_file = fs::File::create(&output_path)?;
    serializer::serialize(&system, &mut out_file)?;

    eprintln!("Done! Generated {} constraints over {} variables.", 
        system.constraints.len(), system.num_variables);
    eprintln!("Output: {}", output_path.display());

    Ok(())
}

fn inspect_file(input: &PathBuf) -> Result<()> {
    let data = fs::read(input)?;
    let report = serializer::deserialize_and_inspect(&data)?;
    println!("{}", report);
    Ok(())
}

fn dump_ast(input: &PathBuf) -> Result<()> {
    let source = fs::read_to_string(input)?;
    let mut lexer = Lexer::new(&source);
    let tokens = lexer.tokenize()?;
    let mut parser = CircuitParser::new(tokens);
    let program = parser.parse()?;

    println!("=== AST for {} ===", input.display());
    for (i, stmt) in program.statements.iter().enumerate() {
        println!("  [{}] {}", i, stmt);
    }
    println!("\nTotal statements: {}", program.statements.len());

    Ok(())
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Compile {
            input,
            output,
            show_ast,
            show_r1cs,
            prime,
        } => compile_file(&input, &output, show_ast, show_r1cs, &prime),
        Commands::Inspect { input } => inspect_file(&input),
        Commands::DumpAst { input } => dump_ast(&input),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
