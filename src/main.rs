use erox_lib::compiler::codegen::Compiler;
use erox_lib::compiler::diagnostics::DiagnosticReporter;
use erox_lib::compiler::lexer::{Lexer, Span};
use erox_lib::compiler::parser::Parser;
use erox_lib::vm::machine::{VM, native_print, native_len, native_push, native_typeof};
use erox_lib::vm::object::{ErroObject, NativeFnWrapper};
use std::env;
use std::fs;
use std::process;

/// EROX Language CLI — Production Entry Point
fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: erox <filename.erx>");
        process::exit(1);
    }

    let filename = &args[1];
    let source = fs::read_to_string(filename).unwrap_or_else(|err| {
        eprintln!("EROX IO Error: Failed to read '{}' -> {}", filename, err);
        process::exit(1);
    });

    if let Err(e) = execute(&source, filename) {
        eprintln!("{}", e);
        process::exit(1);
    }
}

/// Pipeline: Source -> Lexer -> Parser -> Compiler -> VM
fn execute(source: &str, filename: &str) -> Result<(), String> {
    let lexer = Lexer::new(source.to_string());
    let mut parser = Parser::new(lexer);
    let program = parser.parse_program();

    if !parser.errors.is_empty() {
        let reporter = DiagnosticReporter::new(source, filename);
        // Pair spans with error messages
        let pairs: Vec<(Span, String)> = parser.spans.iter()
            .zip(parser.errors.iter())
            .map(|(s, e)| (*s, e.clone()))
            .collect();
        if !pairs.is_empty() {
            return Err(reporter.report_errors(&pairs));
        } else {
            // Fallback: use default span
            let mut err_msg = String::from("EROX Syntax Error:\n");
            for err in parser.errors {
                err_msg.push_str(&format!("  - {}\n", err));
            }
            return Err(err_msg);
        }
    }

    let mut compiler = Compiler::new();
    // Register built-in symbols (order matters — indices must match globals)
    compiler.add_symbol("print".to_string());   // 0
    compiler.add_symbol("len".to_string());     // 1
    compiler.add_symbol("push".to_string());    // 2
    compiler.add_symbol("typeof".to_string());  // 3
    compiler.compile(program);

    let mut vm = VM::new();
    // Register built-in functions
    vm.globals.insert(0, ErroObject::NativeFunction(NativeFnWrapper {
        name: "print",
        func: native_print,
    }));
    vm.globals.insert(1, ErroObject::NativeFunction(NativeFnWrapper {
        name: "len",
        func: native_len,
    }));
    vm.globals.insert(2, ErroObject::NativeFunction(NativeFnWrapper {
        name: "push",
        func: native_push,
    }));
    vm.globals.insert(3, ErroObject::NativeFunction(NativeFnWrapper {
        name: "typeof",
        func: native_typeof,
    }));
    
    vm.run(compiler.instructions, compiler.constants);

    Ok(())
}
