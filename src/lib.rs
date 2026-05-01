pub mod compiler {
    pub mod lexer;
    pub mod parser;
    pub mod codegen;
    pub mod diagnostics;
}

pub mod vm {
    pub mod opcodes;
    pub mod machine;
    pub mod object;
}
