use crate::vm::opcodes::Opcode;
use crate::vm::object::{ErroObject, Upvalue};
use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use std::process::Command;
use std::io::{self, Write};

const MAX_STACK_SIZE: usize = 1024;
const MAX_INSTRUCTIONS: usize = 10_000_000; // Increased for loops

pub struct VM {
    pub stack: Vec<ErroObject>,
    pub globals: HashMap<usize, ErroObject>,
    pub frames: Vec<Frame>,
    pub open_upvalues: Vec<Rc<RefCell<Upvalue>>>,
}

pub struct Frame {
    pub instructions: Vec<Opcode>,
    pub ip: usize,
    pub base_pointer: usize,
    pub upvalues: Vec<Rc<RefCell<Upvalue>>>,
}

macro_rules! binary_op {
    ($self:ident, $op:expr) => {
        {
            if $self.stack.len() < 2 {
                eprintln!("VM Stack Underflow! Stack: {:?}", $self.stack);
                $self.stack.push(ErroObject::Null);
                continue;
            }
            let b = $self.stack.pop().unwrap();
            let a = $self.stack.pop().unwrap();
            match (a, b) {
                (ErroObject::Number(n1), ErroObject::Number(n2)) => $self.stack.push(ErroObject::Number($op(n1, n2))),
                (ErroObject::String(s1), ErroObject::String(s2)) => $self.stack.push(ErroObject::String(format!("{}{}", s1, s2))),
                (ErroObject::String(s), other) => $self.stack.push(ErroObject::String(format!("{}{}", s, other.inspect()))),
                (other, ErroObject::String(s)) => $self.stack.push(ErroObject::String(format!("{}{}", other.inspect(), s))),
                _ => $self.stack.push(ErroObject::Null),
            }
        }
    };
}

macro_rules! comparison_op {
    ($self:ident, $op:expr) => {
        {
            if $self.stack.len() < 2 {
                eprintln!("VM Stack Underflow! Stack: {:?}", $self.stack);
                $self.stack.push(ErroObject::Boolean(false));
                continue;
            }
            let b = $self.stack.pop().unwrap();
            let a = $self.stack.pop().unwrap();
            match (a, b) {
                (ErroObject::Number(n1), ErroObject::Number(n2)) => $self.stack.push(ErroObject::Boolean($op(&n1, &n2))),
                (ErroObject::String(s1), ErroObject::String(s2)) => $self.stack.push(ErroObject::Boolean($op(&s1, &s2))),
                _ => $self.stack.push(ErroObject::Boolean(false)),
            }
        }
    };
}

impl VM {
    pub fn new() -> Self {
        VM {
            stack: Vec::with_capacity(1024),
            globals: HashMap::new(),
            frames: Vec::new(),
            open_upvalues: Vec::new(),
        }
    }

    pub fn run(&mut self, instructions: Vec<Opcode>, constants: Vec<ErroObject>) {
        let mut frame = Frame::new(instructions, 0, Vec::new());
        let mut instruction_count: usize = 0;
        loop {
            while frame.ip < frame.instructions.len() {
                // Emergency: hard instruction limit to prevent infinite loops
                instruction_count += 1;
                if instruction_count > MAX_INSTRUCTIONS {
                    panic!("EROX VM Error: Exceeded maximum instruction count ({}). Likely infinite loop. IP={}, Stack size={}", MAX_INSTRUCTIONS, frame.ip, self.stack.len());
                }
                // Emergency: hard stack limit to prevent OOM
                if self.stack.len() > MAX_STACK_SIZE {
                    panic!("EROX VM Error: Stack Overflow! Stack size {} exceeds limit {}. IP={}, OP={:?}", self.stack.len(), MAX_STACK_SIZE, frame.ip, frame.instructions[frame.ip]);
                }
                let op = frame.instructions[frame.ip];
                frame.ip += 1;

                match op {
                    Opcode::OpConstant(idx) => self.stack.push(constants[idx].clone()),
                    Opcode::OpAdd => binary_op!(self, |a, b| a + b),
                    Opcode::OpSub => binary_op!(self, |a, b| a - b),
                    Opcode::OpMul => binary_op!(self, |a, b| a * b),
                    Opcode::OpDiv => binary_op!(self, |a, b| a / b),
                    Opcode::OpModulo => {
                        if self.stack.len() < 2 { self.stack.push(ErroObject::Null); continue; }
                        let b = self.stack.pop().unwrap();
                        let a = self.stack.pop().unwrap();
                        match (a, b) {
                            (ErroObject::Number(n1), ErroObject::Number(n2)) => {
                                self.stack.push(ErroObject::Number(n1 % n2));
                            }
                            _ => self.stack.push(ErroObject::Null),
                        }
                    }
                    Opcode::OpTrue => self.stack.push(ErroObject::Boolean(true)),
                    Opcode::OpFalse => self.stack.push(ErroObject::Boolean(false)),
                    Opcode::OpNull => self.stack.push(ErroObject::Null),
                    Opcode::OpEqual => comparison_op!(self, |a: &_, b: &_| a == b),
                    Opcode::OpNotEqual => comparison_op!(self, |a: &_, b: &_| a != b),
                    Opcode::OpGT => comparison_op!(self, |a: &_, b: &_| a > b),
                    Opcode::OpLT => comparison_op!(self, |a: &_, b: &_| a < b),
                    Opcode::OpGTE => comparison_op!(self, |a: &_, b: &_| a >= b),
                    Opcode::OpLTE => comparison_op!(self, |a: &_, b: &_| a <= b),
                    Opcode::OpNot => {
                        if let Some(v) = self.stack.pop() {
                            match v {
                                ErroObject::Boolean(b) => self.stack.push(ErroObject::Boolean(!b)),
                                ErroObject::Null => self.stack.push(ErroObject::Boolean(true)),
                                _ => self.stack.push(ErroObject::Boolean(false)),
                            }
                        }
                    }
                    Opcode::OpPop => { self.stack.pop(); }
                    Opcode::OpSetGlobal(idx) => { if let Some(v) = self.stack.last() { self.globals.insert(idx, v.clone()); } }
                    Opcode::OpGetGlobal(idx) => { self.stack.push(self.globals.get(&idx).cloned().unwrap_or(ErroObject::Null)); }
                    Opcode::OpSetLocal(idx) => { if let Some(v) = self.stack.last() { self.stack[frame.base_pointer + idx] = v.clone(); } }
                    Opcode::OpGetLocal(idx) => { self.stack.push(self.stack[frame.base_pointer + idx].clone()); }
                    Opcode::OpJump(pos) => {
                        if pos > frame.instructions.len() {
                            panic!("EROX VM Error: OpJump target {} is out of bounds (instruction count: {})", pos, frame.instructions.len());
                        }
                        frame.ip = pos;
                    }
                    Opcode::OpJumpIfFalse(pos) => {
                        if pos > frame.instructions.len() {
                            panic!("EROX VM Error: OpJumpIfFalse target {} is out of bounds (instruction count: {})", pos, frame.instructions.len());
                        }
                        if let Some(v) = self.stack.pop() {
                            if matches!(v, ErroObject::Boolean(false) | ErroObject::Null) {
                                frame.ip = pos;
                            }
                        }
                    }
                    Opcode::OpCall(arg_count) => self.handle_call(&mut frame, arg_count),
                    Opcode::OpReturnValue => {
                        let res = self.stack.pop().unwrap_or(ErroObject::Null);
                        self.close_upvalues(frame.base_pointer);
                        self.stack.truncate(frame.base_pointer - 1);
                        self.stack.push(res);
                        break;
                    }
                    Opcode::OpGetUpvalue(idx) => {
                        let val = match *frame.upvalues[idx].borrow() {
                            Upvalue::Open(i) => self.stack[i].clone(),
                            Upvalue::Closed(ref v) => v.clone(),
                        };
                        self.stack.push(val);
                    }
                    Opcode::OpSetUpvalue(idx) => {
                        if let Some(v) = self.stack.pop() {
                            let mut uv = frame.upvalues[idx].borrow_mut();
                            match *uv {
                                Upvalue::Open(i) => { self.stack[i] = v; }
                                Upvalue::Closed(ref mut old) => { *old = v; }
                            }
                        }
                    }
                    Opcode::OpClosure(const_idx, num_upvalues) => {
                        if let ErroObject::Function(f) = &constants[const_idx] {
                            let mut uvs = Vec::with_capacity(num_upvalues);
                            for _ in 0..num_upvalues {
                                let op = frame.instructions[frame.ip];
                                frame.ip += 1;
                                match op {
                                    Opcode::OpGetLocal(idx) => uvs.push(self.capture_upvalue(frame.base_pointer + idx)),
                                    Opcode::OpGetUpvalue(idx) => uvs.push(Rc::clone(&frame.upvalues[idx])),
                                    _ => {}
                                }
                            }
                            self.stack.push(ErroObject::Closure(Rc::new(crate::vm::object::ClosureObject {
                                function: Rc::clone(f),
                                upvalues: uvs,
                            })));
                        }
                    }
                    Opcode::OpShellExecute => {
                        if let Some(cmd_obj) = self.stack.pop() {
                            let cmd_str = cmd_obj.inspect();
                            let output = self.execute_shell(&cmd_str);
                            self.stack.push(ErroObject::String(output));
                        }
                    }
                    // ── Data Structure Opcodes ──────────────────────────
                    Opcode::OpArray(count) => {
                        let start = self.stack.len() - count;
                        let elements: Vec<ErroObject> = self.stack.drain(start..).collect();
                        self.stack.push(ErroObject::Array(Rc::new(RefCell::new(elements))));
                    }
                    Opcode::OpObject(count) => {
                        // Stack has: [key1, val1, key2, val2, ...] — count pairs = 2*count items
                        let total = count * 2;
                        let start = self.stack.len() - total;
                        let items: Vec<ErroObject> = self.stack.drain(start..).collect();
                        let mut map = HashMap::new();
                        for chunk in items.chunks(2) {
                            if let (ErroObject::String(key), value) = (&chunk[0], &chunk[1]) {
                                map.insert(key.clone(), value.clone());
                            }
                        }
                        self.stack.push(ErroObject::Object(Rc::new(RefCell::new(map))));
                    }
                    Opcode::OpIndex => {
                        if self.stack.len() < 2 { self.stack.push(ErroObject::Null); continue; }
                        let index = self.stack.pop().unwrap();
                        let collection = self.stack.pop().unwrap();
                        match (&collection, &index) {
                            (ErroObject::Array(arr), ErroObject::Number(n)) => {
                                let i = *n as usize;
                                let borrowed = arr.borrow();
                                let val = borrowed.get(i).cloned().unwrap_or(ErroObject::Null);
                                self.stack.push(val);
                            }
                            (ErroObject::Object(obj), ErroObject::String(key)) => {
                                let borrowed = obj.borrow();
                                let val = borrowed.get(key).cloned().unwrap_or(ErroObject::Null);
                                self.stack.push(val);
                            }
                            (ErroObject::String(s), ErroObject::Number(n)) => {
                                let i = *n as usize;
                                let val = s.chars().nth(i)
                                    .map(|c| ErroObject::String(c.to_string()))
                                    .unwrap_or(ErroObject::Null);
                                self.stack.push(val);
                            }
                            _ => {
                                self.stack.push(ErroObject::Null);
                            }
                        }
                    }
                    Opcode::OpSetIndex => {
                        if self.stack.len() < 3 { self.stack.push(ErroObject::Null); continue; }
                        let value = self.stack.pop().unwrap();
                        let index = self.stack.pop().unwrap();
                        let collection = self.stack.pop().unwrap();
                        match (&collection, &index) {
                            (ErroObject::Array(arr), ErroObject::Number(n)) => {
                                let i = *n as usize;
                                let mut borrowed = arr.borrow_mut();
                                if i < borrowed.len() {
                                    borrowed[i] = value.clone();
                                }
                            }
                            (ErroObject::Object(obj), ErroObject::String(key)) => {
                                let mut borrowed = obj.borrow_mut();
                                borrowed.insert(key.clone(), value.clone());
                            }
                            _ => {}
                        }
                        self.stack.push(value);
                    }
                }
            }
            if let Some(prev) = self.frames.pop() {
                frame = prev;
            } else {
                break;
            }
        }
    }

    fn handle_call(&mut self, current_frame: &mut Frame, arg_count: usize) {
        let func_pos = self.stack.len() - arg_count - 1;
        let func_obj = self.stack[func_pos].clone();
        match func_obj {
            ErroObject::Function(f) => {
                let mut saved = Frame::new(current_frame.instructions.clone(), current_frame.ip, current_frame.upvalues.clone());
                saved.base_pointer = current_frame.base_pointer;
                self.frames.push(saved);
                *current_frame = Frame::new(f.instructions.clone(), 0, Vec::new());
                current_frame.base_pointer = self.stack.len() - arg_count;
            }
            ErroObject::Closure(c) => {
                let mut saved = Frame::new(current_frame.instructions.clone(), current_frame.ip, current_frame.upvalues.clone());
                saved.base_pointer = current_frame.base_pointer;
                self.frames.push(saved);
                *current_frame = Frame::new(c.function.instructions.clone(), 0, c.upvalues.clone());
                current_frame.base_pointer = self.stack.len() - arg_count;
            }
            ErroObject::NativeFunction(nf) => {
                let mut args = Vec::with_capacity(arg_count);
                for _ in 0..arg_count { if let Some(v) = self.stack.pop() { args.push(v); } }
                args.reverse();
                self.stack.pop(); // Pop function
                let res = (nf.func)(args);
                self.stack.push(res);
            }
            _ => {
                for _ in 0..arg_count { self.stack.pop(); }
                self.stack.pop();
                self.stack.push(ErroObject::Null);
            }
        }
    }

    fn capture_upvalue(&mut self, stack_idx: usize) -> Rc<RefCell<Upvalue>> {
        for uv in &self.open_upvalues {
            if let Upvalue::Open(idx) = *uv.borrow() {
                if idx == stack_idx { return Rc::clone(uv); }
            }
        }
        let new_uv = Rc::new(RefCell::new(Upvalue::Open(stack_idx)));
        self.open_upvalues.push(Rc::clone(&new_uv));
        new_uv
    }

    fn close_upvalues(&mut self, last_slot: usize) {
        let mut i = 0;
        while i < self.open_upvalues.len() {
            let uv_rc = Rc::clone(&self.open_upvalues[i]);
            let should_close = match *uv_rc.borrow() {
                Upvalue::Open(idx) => idx >= last_slot,
                Upvalue::Closed(_) => false,
            };
            if should_close {
                self.open_upvalues.remove(i);
                let mut uv = uv_rc.borrow_mut();
                if let Upvalue::Open(idx) = *uv {
                    let v = self.stack[idx].clone();
                    *uv = Upvalue::Closed(v);
                }
            } else { i += 1; }
        }
    }

    fn execute_shell(&self, cmd: &str) -> String {
        let cmd = cmd.trim_matches('"').trim();
        if cmd.is_empty() { return String::new(); }
        let output = if cfg!(target_os = "windows") {
            Command::new("cmd").args(["/C", cmd]).output()
        } else {
            Command::new("sh").args(["-c", cmd]).output()
        };
        match output {
            Ok(out) => {
                let s = String::from_utf8_lossy(&out.stdout);
                s.trim_end_matches(|c| c == '\n' || c == '\r').to_string()
            }
            Err(_) => String::new()
        }
    }
}

impl Frame {
    pub fn new(instructions: Vec<Opcode>, ip: usize, upvalues: Vec<Rc<RefCell<Upvalue>>>) -> Self {
        Frame { instructions, ip, base_pointer: 0, upvalues }
    }
}

// ── Built-in Functions ──────────────────────────────────────────

pub fn native_print(args: Vec<ErroObject>) -> ErroObject {
    for arg in args {
        print!("{}", arg.inspect());
    }
    println!();
    let _ = io::stdout().flush();
    ErroObject::Null
}

/// Built-in: len(array_or_string) -> number
pub fn native_len(args: Vec<ErroObject>) -> ErroObject {
    if let Some(arg) = args.first() {
        match arg {
            ErroObject::Array(arr) => ErroObject::Number(arr.borrow().len() as f64),
            ErroObject::String(s) => ErroObject::Number(s.len() as f64),
            ErroObject::Object(obj) => ErroObject::Number(obj.borrow().len() as f64),
            _ => ErroObject::Number(0.0),
        }
    } else {
        ErroObject::Number(0.0)
    }
}

/// Built-in: push(array, value) -> array
pub fn native_push(args: Vec<ErroObject>) -> ErroObject {
    if args.len() >= 2 {
        if let ErroObject::Array(ref arr) = args[0] {
            arr.borrow_mut().push(args[1].clone());
            return args[0].clone();
        }
    }
    ErroObject::Null
}

/// Built-in: typeof(value) -> string
pub fn native_typeof(args: Vec<ErroObject>) -> ErroObject {
    if let Some(arg) = args.first() {
        let t = match arg {
            ErroObject::Number(_) => "number",
            ErroObject::String(_) => "string",
            ErroObject::Boolean(_) => "boolean",
            ErroObject::Array(_) => "array",
            ErroObject::Object(_) => "object",
            ErroObject::Function(_) | ErroObject::Closure(_) | ErroObject::NativeFunction(_) => "function",
            ErroObject::Null => "null",
        };
        ErroObject::String(t.to_string())
    } else {
        ErroObject::String("undefined".to_string())
    }
}
