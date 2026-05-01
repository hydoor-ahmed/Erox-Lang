use crate::compiler::parser::{Expression, Statement};
use crate::vm::opcodes::Opcode;
use crate::vm::object::{ErroObject, CompiledFunction};
use crate::compiler::lexer::Token;
use std::collections::HashSet;
use std::rc::Rc;

pub struct Compiler {
    pub instructions: Vec<Opcode>,
    pub constants: Vec<ErroObject>,
    pub symbols: Vec<String>,
    pub is_function: bool,
    pub upvalues: Vec<UpvalueInfo>,
    pub is_top_level: bool,
    pub const_symbols: HashSet<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpvalueInfo {
    pub index: usize,
    pub is_local: bool,
}

impl Compiler {
    pub fn new() -> Self {
        Compiler {
            instructions: Vec::new(),
            constants: Vec::new(),
            symbols: Vec::new(),
            is_function: false,
            upvalues: Vec::new(),
            is_top_level: true,
            const_symbols: HashSet::new(),
        }
    }

    pub fn compile(&mut self, statements: Vec<Statement>) {
        let mut parents = Vec::new();
        for stmt in statements {
            self.compile_statement(stmt, &mut parents);
        }
    }

    fn compile_statement(&mut self, stmt: Statement, parents: &mut Vec<*mut Compiler>) {
        match stmt {
            Statement::Import { path: _, item: _ } => {
                // Import logic — placeholder
            }
            Statement::Let { name, value } => {
                self.compile_expression(value, parents);
                let idx = self.add_symbol(name);
                if self.is_top_level {
                    self.emit(Opcode::OpSetGlobal(idx));
                } else {
                    self.emit(Opcode::OpSetLocal(idx));
                }
                self.emit(Opcode::OpPop);
            }
            Statement::Const { name, value } => {
                self.compile_expression(value, parents);
                self.const_symbols.insert(name.clone());
                let idx = self.add_symbol(name);
                if self.is_top_level {
                    self.emit(Opcode::OpSetGlobal(idx));
                } else {
                    self.emit(Opcode::OpSetLocal(idx));
                }
                self.emit(Opcode::OpPop);
            }
            Statement::Assign { name, value } => {
                // Compile-time const enforcement
                if self.const_symbols.contains(&name) {
                    panic!("EROX Compile Error: Cannot reassign constant '{}'", name);
                }
                self.compile_expression(value, parents);
                let scope = self.resolve_symbol(&name, parents);
                match scope {
                    SymbolScope::Global(idx) => self.emit(Opcode::OpSetGlobal(idx)),
                    SymbolScope::Local(idx) => self.emit(Opcode::OpSetLocal(idx)),
                    SymbolScope::Upvalue(idx) => self.emit(Opcode::OpSetUpvalue(idx)),
                }
                self.emit(Opcode::OpPop);
            }
            Statement::Return(expr) => {
                self.compile_expression(expr, parents);
                self.emit(Opcode::OpReturnValue);
            }
            Statement::Block(stmts) => {
                for stmt in stmts {
                    self.compile_statement(stmt, parents);
                }
            }
            Statement::If {
                condition,
                consequence,
                alternative,
            } => {
                self.compile_expression(condition, parents);
                let jump_if_false_idx = self.instructions.len();
                self.emit(Opcode::OpJumpIfFalse(0));

                for stmt in consequence {
                    self.compile_statement(stmt, parents);
                }

                if let Some(alt) = alternative {
                    let jump_idx = self.instructions.len();
                    self.emit(Opcode::OpJump(0));

                    let start_of_else = self.instructions.len();
                    self.instructions[jump_if_false_idx] = Opcode::OpJumpIfFalse(start_of_else);

                    for stmt in alt {
                        self.compile_statement(stmt, parents);
                    }

                    let after_else = self.instructions.len();
                    self.instructions[jump_idx] = Opcode::OpJump(after_else);
                } else {
                    let after_consequence = self.instructions.len();
                    self.instructions[jump_if_false_idx] = Opcode::OpJumpIfFalse(after_consequence);
                }
            }
            Statement::While { condition, body } => {
                let loop_start = self.instructions.len();
                self.compile_expression(condition, parents);
                let jump_if_false_idx = self.instructions.len();
                self.emit(Opcode::OpJumpIfFalse(0));

                for stmt in body {
                    self.compile_statement(stmt, parents);
                }

                self.emit(Opcode::OpJump(loop_start));
                let after_loop = self.instructions.len();
                self.instructions[jump_if_false_idx] = Opcode::OpJumpIfFalse(after_loop);
            }
            Statement::For { init, condition, update, body } => {
                // Compile init
                self.compile_statement(*init, parents);

                // Loop start: condition check
                let loop_start = self.instructions.len();
                self.compile_expression(condition, parents);
                let jump_if_false_idx = self.instructions.len();
                self.emit(Opcode::OpJumpIfFalse(0));

                // Body
                for stmt in body {
                    self.compile_statement(stmt, parents);
                }

                // Update (runs after body, before next condition)
                self.compile_statement(*update, parents);

                // Jump back to condition
                self.emit(Opcode::OpJump(loop_start));
                let after_loop = self.instructions.len();
                self.instructions[jump_if_false_idx] = Opcode::OpJumpIfFalse(after_loop);
            }
            Statement::Function { name, params, body, is_async: _ } => {
                let mut child_compiler = Compiler::new();
                child_compiler.is_function = true;
                child_compiler.is_top_level = false;
                for param in params {
                    child_compiler.add_symbol(param);
                }
                
                parents.push(self as *mut Compiler);
                for stmt in body {
                    child_compiler.compile_statement(stmt, parents);
                }
                parents.pop();

                let num_upvalues = child_compiler.upvalues.len();
                let func_obj = ErroObject::Function(Rc::new(CompiledFunction {
                    name: name.clone(),
                    instructions: child_compiler.instructions,
                    num_locals: child_compiler.symbols.len(),
                    num_upvalues,
                }));

                let const_idx = self.add_constant(func_obj, parents);

                if num_upvalues > 0 {
                    self.emit(Opcode::OpClosure(const_idx, num_upvalues));
                    for uv in child_compiler.upvalues {
                        if uv.is_local {
                            self.emit(Opcode::OpGetLocal(uv.index));
                        } else {
                            self.emit(Opcode::OpGetUpvalue(uv.index));
                        }
                    }
                } else {
                    self.emit(Opcode::OpConstant(const_idx));
                }

                let idx = self.add_symbol(name);
                if self.is_top_level {
                    self.emit(Opcode::OpSetGlobal(idx));
                } else {
                    self.emit(Opcode::OpSetLocal(idx));
                }
                self.emit(Opcode::OpPop);
            }
            Statement::Expression(expr) => {
                self.compile_expression(expr, parents);
                self.emit(Opcode::OpPop);
            }
        }
    }

    fn compile_expression(&mut self, expr: Expression, parents: &mut Vec<*mut Compiler>) {
        match expr {
            Expression::Null => {
                self.emit(Opcode::OpNull);
            }
            Expression::String(s) => {
                let idx = self.add_constant(ErroObject::String(s), parents);
                self.emit(Opcode::OpConstant(idx));
            }
            Expression::Number(n) => {
                let idx = self.add_constant(ErroObject::Number(n), parents);
                self.emit(Opcode::OpConstant(idx));
            }
            Expression::InterpolatedString(parts) => {
                for (i, part) in parts.into_iter().enumerate() {
                    self.compile_expression(part, parents);
                    if i > 0 {
                        self.emit(Opcode::OpAdd);
                    }
                }
            }
            Expression::Binary {
                left,
                operator,
                right,
            } => {
                self.compile_expression(*left, parents);
                self.compile_expression(*right, parents);
                match operator {
                    Token::Plus => self.emit(Opcode::OpAdd),
                    Token::Minus => self.emit(Opcode::OpSub),
                    Token::Star => self.emit(Opcode::OpMul),
                    Token::Slash => self.emit(Opcode::OpDiv),
                    Token::Modulo => self.emit(Opcode::OpModulo),
                    Token::Equal => self.emit(Opcode::OpEqual),
                    Token::NotEqual => self.emit(Opcode::OpNotEqual),
                    Token::GT => self.emit(Opcode::OpGT),
                    Token::LT => self.emit(Opcode::OpLT),
                    Token::GTE => self.emit(Opcode::OpGTE),
                    Token::LTE => self.emit(Opcode::OpLTE),
                    _ => {}
                }
            }
            Expression::Identifier(name) => {
                let scope = self.resolve_symbol(&name, parents);
                match scope {
                    SymbolScope::Global(idx) => self.emit(Opcode::OpGetGlobal(idx)),
                    SymbolScope::Local(idx) => self.emit(Opcode::OpGetLocal(idx)),
                    SymbolScope::Upvalue(idx) => self.emit(Opcode::OpGetUpvalue(idx)),
                }
            }
            Expression::Call { function, arguments } => {
                self.compile_expression(*function, parents);
                let arg_count = arguments.len();
                for arg in arguments {
                    self.compile_expression(arg, parents);
                }
                self.emit(Opcode::OpCall(arg_count));
            }
            Expression::FunctionLiteral { params, body } => {
                let mut child_compiler = Compiler::new();
                child_compiler.is_function = true;
                child_compiler.is_top_level = false;
                for param in params {
                    child_compiler.add_symbol(param);
                }

                parents.push(self as *mut Compiler);
                for stmt in body {
                    child_compiler.compile_statement(stmt, parents);
                }
                parents.pop();

                let num_upvalues = child_compiler.upvalues.len();
                let func_obj = ErroObject::Function(Rc::new(CompiledFunction {
                    name: "anonymous".to_string(),
                    instructions: child_compiler.instructions,
                    num_locals: child_compiler.symbols.len(),
                    num_upvalues,
                }));
                let const_idx = self.add_constant(func_obj, parents);
                self.emit(Opcode::OpClosure(const_idx, num_upvalues));
                for uv in child_compiler.upvalues {
                    if uv.is_local {
                        self.emit(Opcode::OpGetLocal(uv.index));
                    } else {
                        self.emit(Opcode::OpGetUpvalue(uv.index));
                    }
                }
            }
            Expression::Prefix { operator, right } => {
                self.compile_expression(*right, parents);
                match operator {
                    Token::Bang => self.emit(Opcode::OpNot), 
                    Token::Minus => {
                        let zero_idx = self.add_constant(ErroObject::Number(0.0), parents);
                        self.emit(Opcode::OpConstant(zero_idx));
                        self.emit(Opcode::OpSub); 
                    }
                    _ => {}
                }
            }
            Expression::Postfix { operator, operand } => {
                // i++ → get i, push 1, add, set i
                // i-- → get i, push 1, sub, set i
                let scope = self.resolve_symbol(&operand, parents);
                match &scope {
                    SymbolScope::Global(idx) => self.emit(Opcode::OpGetGlobal(*idx)),
                    SymbolScope::Local(idx) => self.emit(Opcode::OpGetLocal(*idx)),
                    SymbolScope::Upvalue(idx) => self.emit(Opcode::OpGetUpvalue(*idx)),
                }
                let one_idx = self.add_constant(ErroObject::Number(1.0), parents);
                self.emit(Opcode::OpConstant(one_idx));
                match operator {
                    Token::PlusPlus => self.emit(Opcode::OpAdd),
                    Token::MinusMinus => self.emit(Opcode::OpSub),
                    _ => {}
                }
                match scope {
                    SymbolScope::Global(idx) => self.emit(Opcode::OpSetGlobal(idx)),
                    SymbolScope::Local(idx) => self.emit(Opcode::OpSetLocal(idx)),
                    SymbolScope::Upvalue(idx) => self.emit(Opcode::OpSetUpvalue(idx)),
                }
            }
            Expression::Shell(expr) => {
                self.compile_expression(*expr, parents);
                self.emit(Opcode::OpShellExecute);
            }
            Expression::Boolean(b) => {
                if b { self.emit(Opcode::OpTrue); }
                else { self.emit(Opcode::OpFalse); }
            }
            Expression::Array(elements) => {
                let count = elements.len();
                for elem in elements {
                    self.compile_expression(elem, parents);
                }
                self.emit(Opcode::OpArray(count));
            }
            Expression::Object(pairs) => {
                let count = pairs.len();
                for (key, value) in pairs {
                    let key_idx = self.add_constant(ErroObject::String(key), parents);
                    self.emit(Opcode::OpConstant(key_idx));
                    self.compile_expression(value, parents);
                }
                self.emit(Opcode::OpObject(count));
            }
            Expression::Index { object, index } => {
                self.compile_expression(*object, parents);
                self.compile_expression(*index, parents);
                self.emit(Opcode::OpIndex);
            }
            Expression::Member { object, field } => {
                self.compile_expression(*object, parents);
                let key_idx = self.add_constant(ErroObject::String(field), parents);
                self.emit(Opcode::OpConstant(key_idx));
                self.emit(Opcode::OpIndex);
            }
            Expression::Await(expr) => {
                // For now, await simply evaluates the expression synchronously.
                // In async mode, this would yield the current frame.
                self.compile_expression(*expr, parents);
            }
        }
    }

    pub fn add_symbol(&mut self, name: String) -> usize {
        if let Some(pos) = self.symbols.iter().position(|s| s == &name) {
            return pos;
        }
        self.symbols.push(name);
        self.symbols.len() - 1
    }

    fn add_constant(&mut self, obj: ErroObject, parents: &Vec<*mut Compiler>) -> usize {
        if !parents.is_empty() {
            let top_compiler_ptr = parents[0];
            return unsafe { (&mut *top_compiler_ptr).add_constant(obj, &vec![]) };
        }
        self.constants.push(obj);
        self.constants.len() - 1
    }

    fn emit(&mut self, op: Opcode) {
        self.instructions.push(op);
    }

    fn resolve_symbol(&mut self, name: &str, parents: &mut Vec<*mut Compiler>) -> SymbolScope {
        if let Some(pos) = self.symbols.iter().position(|s| s == name) {
            if self.is_top_level {
                return SymbolScope::Global(pos);
            }
            return SymbolScope::Local(pos);
        }

        if !parents.is_empty() {
            let mut parents_copy = parents.clone();
            let parent_ptr = parents_copy.pop().unwrap();
            let parent = unsafe { &mut *parent_ptr };
            let scope = parent.resolve_symbol(name, &mut parents_copy);
            
            match scope {
                SymbolScope::Local(idx) => {
                    let uv_idx = self.add_upvalue(idx, true);
                    return SymbolScope::Upvalue(uv_idx);
                }
                SymbolScope::Upvalue(idx) => {
                    let uv_idx = self.add_upvalue(idx, false);
                    return SymbolScope::Upvalue(uv_idx);
                }
                SymbolScope::Global(idx) => return SymbolScope::Global(idx),
            }
        }

        SymbolScope::Global(self.add_symbol(name.to_string()))
    }

    fn add_upvalue(&mut self, index: usize, is_local: bool) -> usize {
        for (i, uv) in self.upvalues.iter().enumerate() {
            if uv.index == index && uv.is_local == is_local {
                return i;
            }
        }
        self.upvalues.push(UpvalueInfo { index, is_local });
        self.upvalues.len() - 1
    }
}

pub enum SymbolScope {
    Global(usize),
    Local(usize),
    Upvalue(usize),
}
