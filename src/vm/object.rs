use crate::vm::opcodes::Opcode;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::fmt;

/// EROX Object System — Professional RC-based memory management.
#[derive(Clone)]
pub enum ErroObject {
    Number(f64),
    String(String),
    Boolean(bool),
    Function(Rc<CompiledFunction>),
    Closure(Rc<ClosureObject>),
    NativeFunction(NativeFnWrapper),
    Array(Rc<RefCell<Vec<ErroObject>>>),
    Object(Rc<RefCell<HashMap<String, ErroObject>>>),
    Null,
}

#[derive(Clone)]
pub struct NativeFnWrapper {
    pub name: &'static str,
    pub func: fn(Vec<ErroObject>) -> ErroObject,
}

impl PartialEq for NativeFnWrapper {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.func as usize == other.func as usize
    }
}

impl fmt::Debug for NativeFnWrapper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<native function {}>", self.name)
    }
}

impl PartialEq for ErroObject {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ErroObject::Number(a), ErroObject::Number(b)) => a == b,
            (ErroObject::String(a), ErroObject::String(b)) => a == b,
            (ErroObject::Boolean(a), ErroObject::Boolean(b)) => a == b,
            (ErroObject::Function(a), ErroObject::Function(b)) => Rc::ptr_eq(a, b),
            (ErroObject::Closure(a), ErroObject::Closure(b)) => Rc::ptr_eq(a, b),
            (ErroObject::NativeFunction(a), ErroObject::NativeFunction(b)) => a == b,
            (ErroObject::Array(a), ErroObject::Array(b)) => Rc::ptr_eq(a, b),
            (ErroObject::Object(a), ErroObject::Object(b)) => Rc::ptr_eq(a, b),
            (ErroObject::Null, ErroObject::Null) => true,
            _ => false,
        }
    }
}

impl fmt::Debug for ErroObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErroObject::Number(n) => write!(f, "Number({})", n),
            ErroObject::String(s) => write!(f, "String({:?})", s),
            ErroObject::Boolean(b) => write!(f, "Boolean({})", b),
            ErroObject::Function(func) => write!(f, "Function({})", func.name),
            ErroObject::Closure(c) => write!(f, "Closure({})", c.function.name),
            ErroObject::NativeFunction(nf) => nf.fmt(f),
            ErroObject::Array(arr) => write!(f, "Array({:?})", arr.borrow()),
            ErroObject::Object(obj) => write!(f, "Object({:?})", obj.borrow()),
            ErroObject::Null => write!(f, "Null"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledFunction {
    pub name: String,
    pub instructions: Vec<Opcode>,
    pub num_locals: usize,
    pub num_upvalues: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClosureObject {
    pub function: Rc<CompiledFunction>,
    pub upvalues: Vec<Rc<RefCell<Upvalue>>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Upvalue {
    Open(usize),
    Closed(ErroObject),
}

impl ErroObject {
    pub fn inspect(&self) -> String {
        match self {
            ErroObject::Number(n) => {
                if *n == (*n as i64) as f64 && n.is_finite() {
                    format!("{}", *n as i64)
                } else {
                    n.to_string()
                }
            }
            ErroObject::String(s) => s.clone(),
            ErroObject::Boolean(b) => b.to_string(),
            ErroObject::Function(f) => format!("<Function {}>", f.name),
            ErroObject::Closure(c) => format!("<closure {}>", c.function.name),
            ErroObject::NativeFunction(nf) => format!("<native fn {}>", nf.name),
            ErroObject::Array(arr) => {
                let elems: Vec<String> = arr.borrow().iter().map(|e| e.inspect_repr()).collect();
                format!("[{}]", elems.join(", "))
            }
            ErroObject::Object(obj) => {
                let pairs: Vec<String> = obj.borrow().iter()
                    .map(|(k, v)| format!("\"{}\": {}", k, v.inspect_repr()))
                    .collect();
                format!("{{ {} }}", pairs.join(", "))
            }
            ErroObject::Null => "null".to_string(),
        }
    }

    /// Like inspect() but wraps strings in quotes for nested display.
    pub fn inspect_repr(&self) -> String {
        match self {
            ErroObject::String(s) => format!("\"{}\"", s),
            other => other.inspect(),
        }
    }

    /// Check truthiness for conditionals
    pub fn is_truthy(&self) -> bool {
        match self {
            ErroObject::Boolean(false) | ErroObject::Null => false,
            ErroObject::Number(n) => *n != 0.0,
            _ => true,
        }
    }
}
