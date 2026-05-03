use crate::vm::object::{ClosureObject, ErroObject, FutureObject, NativeFnWrapper, Upvalue};
use crate::vm::opcodes::Opcode;
use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::io::{self, Write};
use std::pin::Pin;
use std::process::Command;
use std::rc::Rc;

const MAX_STACK_SIZE: usize = 1024;
const MAX_INSTRUCTIONS: usize = 10_000_000;

pub struct TryHandler {
    pub catch_ip: usize,
    pub frame_idx: usize,
    pub stack_depth: usize,
}

pub struct VM {
    pub stack: Vec<ErroObject>,
    pub globals: HashMap<usize, ErroObject>,
    pub frames: Vec<Frame>,
    pub open_upvalues: Vec<Rc<RefCell<Upvalue>>>,
    pub try_handlers: Vec<TryHandler>,
    pub source: String,
    pub filename: String,
}

pub struct Frame {
    pub instructions: Vec<Opcode>,
    pub ip: usize,
    pub base_pointer: usize,
    pub upvalues: Vec<Rc<RefCell<Upvalue>>>,
}

// ── Macros ──────────────────────────────────────────────────────

macro_rules! binary_op {
    ($self:ident, $op:expr) => {{
        if $self.stack.len() < 2 {
            eprintln!("VM Stack Underflow! Stack: {:?}", $self.stack);
            $self.stack.push(ErroObject::Null);
            continue;
        }
        let b = $self.stack.pop().unwrap();
        let a = $self.stack.pop().unwrap();
        match (a, b) {
            (ErroObject::Number(n1), ErroObject::Number(n2)) => {
                $self.stack.push(ErroObject::Number($op(n1, n2)))
            }
            (ErroObject::String(s1), ErroObject::String(s2)) => $self
                .stack
                .push(ErroObject::String(format!("{}{}", s1, s2))),
            (ErroObject::String(s), other) => {
                $self
                    .stack
                    .push(ErroObject::String(format!("{}{}", s, other.inspect())))
            }
            (other, ErroObject::String(s)) => {
                $self
                    .stack
                    .push(ErroObject::String(format!("{}{}", other.inspect(), s)))
            }
            _ => $self.stack.push(ErroObject::Null),
        }
    }};
}

macro_rules! comparison_op {
    ($self:ident, $op:expr) => {{
        if $self.stack.len() < 2 {
            eprintln!("VM Stack Underflow! Stack: {:?}", $self.stack);
            $self.stack.push(ErroObject::Boolean(false));
            continue;
        }
        let b = $self.stack.pop().unwrap();
        let a = $self.stack.pop().unwrap();
        match (a, b) {
            (ErroObject::Number(n1), ErroObject::Number(n2)) => {
                $self.stack.push(ErroObject::Boolean($op(&n1, &n2)))
            }
            (ErroObject::String(s1), ErroObject::String(s2)) => {
                $self.stack.push(ErroObject::Boolean($op(&s1, &s2)))
            }
            _ => $self.stack.push(ErroObject::Boolean(false)),
        }
    }};
}

// ── VM Implementation ────────────────────────────────────────────

impl VM {
    pub fn new() -> Self {
        VM {
            stack: Vec::with_capacity(1024),
            globals: HashMap::new(),
            frames: Vec::new(),
            open_upvalues: Vec::new(),
            try_handlers: Vec::new(),
            source: String::new(),
            filename: String::new(),
        }
    }

    pub fn run<'a>(
        &'a mut self,
        instructions: Vec<Opcode>,
        constants: Vec<ErroObject>,
    ) -> Pin<Box<dyn Future<Output = ()> + 'a>> {
        Box::pin(async move {
            let mut frame = Frame::new(instructions, 0, Vec::new());

            // FIX Bug 5: per-frame instruction counter, not a global one.
            // Resets every time we enter a new frame so legitimate deep call
            // stacks don't trip the limit just because of earlier frames.
            let mut frame_instruction_count: usize = 0;

            loop {
                // FIX Bug 3 (correct fix): track whether the inner while loop
                // exited via an explicit OpReturnValue (`break`) or by simply
                // running out of instructions (implicit null return).
                // Must be declared here so OpReturnValue can set it before `break`.
                let mut implicit_return = true;

                while frame.ip < frame.instructions.len() {
                    // Per-frame instruction limit
                    frame_instruction_count += 1;
                    if frame_instruction_count > MAX_INSTRUCTIONS {
                        panic!(
                            "EROX VM Error: Frame exceeded max instruction count ({}).\
                             IP={}, Stack={}, base_pointer={}",
                            MAX_INSTRUCTIONS,
                            frame.ip,
                            self.stack.len(),
                            frame.base_pointer,
                        );
                    }

                    if self.stack.len() > MAX_STACK_SIZE {
                        panic!(
                            "EROX VM Error: Stack Overflow! size={} limit={} IP={} OP={:?}",
                            self.stack.len(),
                            MAX_STACK_SIZE,
                            frame.ip,
                            frame.instructions[frame.ip],
                        );
                    }

                    let op = frame.instructions[frame.ip];
                    frame.ip += 1;

                    match op {
                        // ── Literals ─────────────────────────────────────
                        Opcode::OpConstant(idx) => self.stack.push(constants[idx].clone()),
                        Opcode::OpTrue => self.stack.push(ErroObject::Boolean(true)),
                        Opcode::OpFalse => self.stack.push(ErroObject::Boolean(false)),
                        Opcode::OpNull => self.stack.push(ErroObject::Null),

                        // ── Arithmetic ───────────────────────────────────
                        Opcode::OpAdd => binary_op!(self, |a, b| a + b),
                        Opcode::OpSub => binary_op!(self, |a, b| a - b),
                        Opcode::OpMul => binary_op!(self, |a, b| a * b),
                        Opcode::OpDiv => binary_op!(self, |a, b| a / b),

                        Opcode::OpModulo => {
                            if self.stack.len() < 2 {
                                self.stack.push(ErroObject::Null);
                                continue;
                            }
                            let b = self.stack.pop().unwrap();
                            let a = self.stack.pop().unwrap();
                            match (a, b) {
                                (ErroObject::Number(n1), ErroObject::Number(n2)) => {
                                    self.stack.push(ErroObject::Number(n1 % n2));
                                }
                                _ => self.stack.push(ErroObject::Null),
                            }
                        }

                        // ── Comparison ───────────────────────────────────
                        Opcode::OpEqual => comparison_op!(self, |a: &_, b: &_| a == b),
                        Opcode::OpNotEqual => comparison_op!(self, |a: &_, b: &_| a != b),
                        Opcode::OpGT => comparison_op!(self, |a: &_, b: &_| a > b),
                        Opcode::OpLT => comparison_op!(self, |a: &_, b: &_| a < b),
                        Opcode::OpGTE => comparison_op!(self, |a: &_, b: &_| a >= b),
                        Opcode::OpLTE => comparison_op!(self, |a: &_, b: &_| a <= b),

                        Opcode::OpNot => {
                            // FIX: use unwrap_or(Null) to handle empty stack gracefully
                            let v = self.stack.pop().unwrap_or(ErroObject::Null);
                            match v {
                                ErroObject::Boolean(b) => self.stack.push(ErroObject::Boolean(!b)),
                                ErroObject::Null => self.stack.push(ErroObject::Boolean(true)),
                                _ => self.stack.push(ErroObject::Boolean(false)),
                            }
                        }

                        // ── Stack ────────────────────────────────────────
                        Opcode::OpPop => {
                            self.stack.pop();
                        }

                        // ── Globals ──────────────────────────────────────
                        Opcode::OpSetGlobal(idx) => {
                            // Non-destructive peek — keeps value on stack (same as SetLocal)
                            if let Some(v) = self.stack.last() {
                                self.globals.insert(idx, v.clone());
                            }
                        }
                        Opcode::OpGetGlobal(idx) => {
                            self.stack
                                .push(self.globals.get(&idx).cloned().unwrap_or(ErroObject::Null));
                        }

                        // ── Locals ───────────────────────────────────────
                        Opcode::OpSetLocal(idx) => {
                            if let Some(v) = self.stack.last() {
                                self.stack[frame.base_pointer + idx] = v.clone(); // index 5 → PANIC or OOB
                            }
                        }
                        Opcode::OpGetLocal(idx) => {
                            self.stack
                                .push(self.stack[frame.base_pointer + idx].clone());
                        }

                        // ── Jumps ─────────────────────────────────────────
                        Opcode::OpJump(pos) => {
                            if pos > frame.instructions.len() {
                                panic!(
                                    "EROX VM: OpJump target {} out of bounds (len={})",
                                    pos,
                                    frame.instructions.len()
                                );
                            }
                            frame.ip = pos;
                        }

                        // FIX Bug 4: unwrap_or(Null) so an empty stack is treated
                        // as falsy and triggers the jump — no silent no-op.
                        Opcode::OpJumpIfFalse(pos) => {
                            if pos > frame.instructions.len() {
                                panic!(
                                    "EROX VM: OpJumpIfFalse target {} out of bounds (len={})",
                                    pos,
                                    frame.instructions.len()
                                );
                            }
                            let condition = self.stack.pop().unwrap_or(ErroObject::Null);
                            if matches!(condition, ErroObject::Boolean(false) | ErroObject::Null) {
                                frame.ip = pos;
                            }
                        }

                        // ── Calls & Returns ──────────────────────────────
                        Opcode::OpCall(arg_count) => {
                            self.handle_call(&mut frame, arg_count);
                            // FIX Bug 5: entering a new frame resets its budget
                            frame_instruction_count = 0;
                        }

                        // FIX Bug 6: guard base_pointer == 0 before subtracting
                        // to avoid usize underflow panic in debug / wrap in release.
                        Opcode::OpReturnValue => {
                            let res = self.stack.pop().unwrap_or(ErroObject::Null);
                            self.close_upvalues(frame.base_pointer);
                            let new_len = if frame.base_pointer > 0 {
                                frame.base_pointer - 1
                            } else {
                                0
                            };
                            self.stack.truncate(new_len);
                            self.stack.push(res);
                            // FIX Bug 3: signal that the return value is already on
                            // the stack so the frame-restore code does NOT add Null.
                            implicit_return = false;
                            break;
                        }

                        // ── Upvalues ─────────────────────────────────────
                        Opcode::OpGetUpvalue(idx) => {
                            let val = match *frame.upvalues[idx].borrow() {
                                Upvalue::Open(i) => self.stack[i].clone(),
                                Upvalue::Closed(ref v) => v.clone(),
                            };
                            self.stack.push(val);
                        }

                        // FIX Bug 1: was pop() — destroys stack value. Must mirror
                        // SetLocal/SetGlobal and use last().cloned() (non-destructive).
                        Opcode::OpSetUpvalue(idx) => {
                            if let Some(v) = self.stack.last().cloned() {
                                let mut uv = frame.upvalues[idx].borrow_mut();
                                match *uv {
                                    Upvalue::Open(i) => {
                                        self.stack[i] = v;
                                    }
                                    Upvalue::Closed(ref mut old) => {
                                        *old = v;
                                    }
                                }
                            }
                        }

                        // FIX Bug 10: unknown upvalue descriptor is now a hard
                        // compiler-bug panic instead of a silent skip that leaves
                        // `uvs` shorter than num_upvalues (causing later OOB panic).
                        Opcode::OpClosure(const_idx, num_upvalues) => {
                            if let ErroObject::Function(f) = &constants[const_idx] {
                                let mut uvs = Vec::with_capacity(num_upvalues);
                                for uv_slot in 0..num_upvalues {
                                    let descriptor = frame.instructions[frame.ip];
                                    frame.ip += 1;
                                    match descriptor {
                                        Opcode::OpGetLocal(idx) => {
                                            uvs.push(
                                                self.capture_upvalue(frame.base_pointer + idx),
                                            );
                                        }
                                        Opcode::OpGetUpvalue(idx) => {
                                            uvs.push(Rc::clone(&frame.upvalues[idx]));
                                        }
                                        other => {
                                            panic!(
                                                "EROX VM: OpClosure expected upvalue descriptor \
                                                 at slot {}/{} but got {:?}. Compiler bug.",
                                                uv_slot, num_upvalues, other
                                            );
                                        }
                                    }
                                }
                                self.stack.push(ErroObject::Closure(Rc::new(
                                    crate::vm::object::ClosureObject {
                                        function: Rc::clone(f),
                                        upvalues: uvs,
                                    },
                                )));
                            }
                        }

                        // ── Shell ─────────────────────────────────────────
                        Opcode::OpShellExecute => {
                            if let Some(cmd_obj) = self.stack.pop() {
                                let cmd_str = cmd_obj.inspect();
                                let output = self.execute_shell(&cmd_str).await;
                                self.stack.push(ErroObject::String(output));
                            }
                        }

                        // ── Data Structures ──────────────────────────────
                        Opcode::OpArray(count) => {
                            // saturating_sub prevents underflow if count > stack len
                            let start = self.stack.len().saturating_sub(count);
                            let elements: Vec<ErroObject> = self.stack.drain(start..).collect();
                            self.stack
                                .push(ErroObject::Array(Rc::new(RefCell::new(elements))));
                        }

                        Opcode::OpObject(count) => {
                            let total = count * 2;
                            let start = self.stack.len().saturating_sub(total);
                            let items: Vec<ErroObject> = self.stack.drain(start..).collect();
                            let mut map = HashMap::new();
                            for chunk in items.chunks(2) {
                                if let (ErroObject::String(key), value) = (&chunk[0], &chunk[1]) {
                                    map.insert(key.clone(), value.clone());
                                }
                            }
                            self.stack
                                .push(ErroObject::Object(Rc::new(RefCell::new(map))));
                        }

                        Opcode::OpIndex => {
                            if self.stack.len() < 2 {
                                self.stack.push(ErroObject::Null);
                                continue;
                            }
                            let index = self.stack.pop().unwrap();
                            let collection = self.stack.pop().unwrap();
                            let val = match (&collection, &index) {
                                (ErroObject::Array(arr), ErroObject::Number(n)) => arr
                                    .borrow()
                                    .get(*n as usize)
                                    .cloned()
                                    .unwrap_or(ErroObject::Null),
                                (ErroObject::Object(obj), ErroObject::String(key)) => {
                                    obj.borrow().get(key).cloned().unwrap_or(ErroObject::Null)
                                }
                                (ErroObject::String(s), ErroObject::Number(n)) => s
                                    .chars()
                                    .nth(*n as usize)
                                    .map(|c| ErroObject::String(c.to_string()))
                                    .unwrap_or(ErroObject::Null),
                                _ => ErroObject::Null,
                            };
                            self.stack.push(val);
                        }

                        // FIX Bug 9: out-of-bounds array write now extends the array
                        // with Null fill instead of silently discarding the value.
                        Opcode::OpSetIndex => {
                            if self.stack.len() < 3 {
                                self.stack.push(ErroObject::Null);
                                continue;
                            }
                            let value = self.stack.pop().unwrap();
                            let index = self.stack.pop().unwrap();
                            let collection = self.stack.pop().unwrap();
                            match (&collection, &index) {
                                (ErroObject::Array(arr), ErroObject::Number(n)) => {
                                    let i = *n as usize;
                                    let mut b = arr.borrow_mut();
                                    // Extend with Null so sparse writes don't vanish silently
                                    if i >= b.len() {
                                        b.resize(i + 1, ErroObject::Null);
                                    }
                                    b[i] = value.clone();
                                }
                                (ErroObject::Object(obj), ErroObject::String(key)) => {
                                    obj.borrow_mut().insert(key.clone(), value.clone());
                                }
                                _ => {
                                    // Non-indexable target: push Null as error signal
                                    self.stack.push(ErroObject::Null);
                                    continue;
                                }
                            }
                            self.stack.push(value);
                        }

                        // ── Method Dispatch ───────────────────────────────
                        Opcode::OpMethodCall(arg_count) => {
                            self.handle_method_call(arg_count, &mut frame);
                            // FIX Bug 5: method calls enter a new frame context
                            frame_instruction_count = 0;
                        }

                        // ── Exception Handling ────────────────────────────
                        Opcode::OpTryStart(catch_addr) => {
                            self.try_handlers.push(TryHandler {
                                catch_ip: catch_addr,
                                frame_idx: self.frames.len(),
                                stack_depth: self.stack.len(),
                            });
                        }
                        Opcode::OpTryEnd => {
                            self.try_handlers.pop();
                        }
                        Opcode::OpThrow => {
                            let err = self.stack.pop().unwrap_or(ErroObject::Null);
                            self.handle_runtime_error_obj(err, &mut frame);
                        }

                        // ── Module Import ─────────────────────────────────
                        // FIX Bug 8: pass a cloned snapshot of constants to
                        // handle_import so we don't hold a reference into the
                        // moved Vec across an await point while self is also
                        // mutably borrowed.
                        Opcode::OpImport => {
                            if let Some(path_obj) = self.stack.pop() {
                                let path = path_obj.inspect();
                                let constants_snapshot = constants.clone();
                                self.handle_import(&path, &constants_snapshot).await;
                            }
                        }

                        // ── Async Runtime ─────────────────────────────────
                        // FIX Bug 2: OpAwait resolves a Future by re-dispatching
                        // its body as a NON-async function through handle_call.
                        //
                        // Why is_async = false is critical:
                        //   handle_call checks is_async to decide whether to push
                        //   a Future object (deferred) or push a real call frame
                        //   (execute now). Setting it false here makes the run-loop
                        //   execute the body inline, so any OpShellExecute /
                        //   OpImport inside the async function are properly .await-ed
                        //   by this enclosing Rust async context.
                        Opcode::OpAwait => {
                            let val = self.stack.pop().unwrap_or(ErroObject::Null);
                            match val {
                                ErroObject::Future(fut) => {
                                    let arg_count = fut.args.len();

                                    let exec_func = Rc::new(crate::vm::object::CompiledFunction {
                                        name: fut.function.name.clone(),
                                        instructions: fut.function.instructions.clone(),
                                        num_locals: fut.function.num_locals,
                                        num_upvalues: fut.function.num_upvalues,
                                        is_async: false, // ← must be false; see above
                                    });

                                    if !fut.upvalues.is_empty() {
                                        self.stack.push(ErroObject::Closure(Rc::new(
                                            ClosureObject {
                                                function: exec_func,
                                                upvalues: fut.upvalues.clone(),
                                            },
                                        )));
                                    } else {
                                        self.stack.push(ErroObject::Function(exec_func));
                                    }
                                    for arg in fut.args.iter() {
                                        self.stack.push(arg.clone());
                                    }

                                    self.handle_call(&mut frame, arg_count);
                                    // FIX Bug 5: fresh budget for the resolved frame
                                    frame_instruction_count = 0;
                                }

                                // Awaiting a non-Future is a passthrough no-op
                                other => self.stack.push(other),
                            }
                        }
                    } // end match op
                } // end while frame.ip < frame.instructions.len()

                // ── Frame Restore ─────────────────────────────────────────
                if let Some(prev) = self.frames.pop() {
                    // FIX Bug 3 (correct): only push Null for implicit returns
                    // (function fell off the end). Explicit OpReturnValue already
                    // pushed its value and set implicit_return = false before break.
                    if implicit_return {
                        self.stack.push(ErroObject::Null);
                    }
                    frame = prev;
                    // FIX Bug 5: each resumed frame gets a fresh instruction budget
                    frame_instruction_count = 0;
                } else {
                    break;
                }
            }
        })
    }

    // ── handle_call ───────────────────────────────────────────────────────────
    fn handle_call(&mut self, current_frame: &mut Frame, arg_count: usize) {
        let func_pos = self.stack.len() - arg_count - 1;
        let func_obj = self.stack[func_pos].clone();

        match func_obj {
            ErroObject::Function(f) => {
                if f.is_async {
                    let mut args = Vec::with_capacity(arg_count);
                    for _ in 0..arg_count {
                        if let Some(v) = self.stack.pop() {
                            args.push(v);
                        }
                    }
                    args.reverse();
                    self.stack.pop(); // pop the function itself
                    self.stack.push(ErroObject::Future(Rc::new(FutureObject {
                        function: Rc::clone(&f),
                        upvalues: Vec::new(),
                        args,
                    })));
                } else {
                    let mut saved = Frame::new(
                        current_frame.instructions.clone(),
                        current_frame.ip,
                        current_frame.upvalues.clone(),
                    );
                    saved.base_pointer = current_frame.base_pointer;
                    self.frames.push(saved);

                    *current_frame = Frame::new(f.instructions.clone(), 0, Vec::new());
                    // base_pointer points past the function object to arg0
                    // stack layout: [... | func | arg0 | arg1 | ...]
                    //                             ^ base_pointer
                    current_frame.base_pointer = self.stack.len() - arg_count;

                    // ← FIX: reserve Null slots for locals that aren't params
                    if f.num_locals > arg_count {
                        for _ in 0..(f.num_locals - arg_count) {
                            self.stack.push(ErroObject::Null);
                        }
                    }
                }
            }

            ErroObject::Closure(c) => {
                if c.function.is_async {
                    let mut args = Vec::with_capacity(arg_count);
                    for _ in 0..arg_count {
                        if let Some(v) = self.stack.pop() {
                            args.push(v);
                        }
                    }
                    args.reverse();
                    self.stack.pop();
                    self.stack.push(ErroObject::Future(Rc::new(FutureObject {
                        function: Rc::clone(&c.function),
                        upvalues: c.upvalues.clone(),
                        args,
                    })));
                } else {
                    let mut saved = Frame::new(
                        current_frame.instructions.clone(),
                        current_frame.ip,
                        current_frame.upvalues.clone(),
                    );
                    saved.base_pointer = current_frame.base_pointer;
                    self.frames.push(saved);

                    *current_frame =
                        Frame::new(c.function.instructions.clone(), 0, c.upvalues.clone());
                    // same layout as Function above
                    current_frame.base_pointer = self.stack.len() - arg_count;

                    // ← FIX: reserve Null slots for locals that aren't params
                    if c.function.num_locals > arg_count {
                        for _ in 0..(c.function.num_locals - arg_count) {
                            self.stack.push(ErroObject::Null);
                        }
                    }
                }
            }

            ErroObject::NativeFunction(nf) => {
                let mut args = Vec::with_capacity(arg_count);
                for _ in 0..arg_count {
                    if let Some(v) = self.stack.pop() {
                        args.push(v);
                    }
                }
                args.reverse();
                self.stack.pop(); // pop function
                let res = (nf.func)(args);
                self.stack.push(res);
            }

            _ => {
                // Not callable: discard args + callee, push Null
                for _ in 0..arg_count {
                    self.stack.pop();
                }
                self.stack.pop();
                self.stack.push(ErroObject::Null);
            }
        }
    }

    // ── Upvalue helpers ───────────────────────────────────────────────────────

    fn capture_upvalue(&mut self, stack_idx: usize) -> Rc<RefCell<Upvalue>> {
        // Reuse existing open upvalue for the same slot if one exists
        for uv in &self.open_upvalues {
            if let Upvalue::Open(idx) = *uv.borrow() {
                if idx == stack_idx {
                    return Rc::clone(uv);
                }
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
            } else {
                i += 1;
            }
        }
    }

    // ── Shell execution ───────────────────────────────────────────────────────

    async fn execute_shell(&self, cmd: &str) -> String {
        let cmd = cmd.trim_matches('"').trim();
        if cmd.is_empty() {
            return String::new();
        }
        let output = if cfg!(target_os = "windows") {
            tokio::process::Command::new("cmd")
                .args(["/C", cmd])
                .output()
                .await
        } else {
            tokio::process::Command::new("sh")
                .args(["-c", cmd])
                .output()
                .await
        };
        match output {
            Ok(out) => {
                let s = String::from_utf8_lossy(&out.stdout);
                s.trim_end_matches(|c| c == '\n' || c == '\r').to_string()
            }
            Err(_) => String::new(),
        }
    }

    // ── Method dispatch ───────────────────────────────────────────────────────

    fn handle_method_call(&mut self, arg_count: usize, frame: &mut Frame) {
        // Stack layout: [receiver, method_name_string, arg1, arg2, ...]
        let args_start = self.stack.len() - arg_count;
        let args: Vec<ErroObject> = self.stack.drain(args_start..).collect();
        let method_name_obj = self.stack.pop().unwrap_or(ErroObject::Null);
        let receiver = self.stack.pop().unwrap_or(ErroObject::Null);

        let method_name = match &method_name_obj {
            ErroObject::String(s) => s.clone(),
            _ => {
                self.stack.push(ErroObject::Null);
                return;
            }
        };

        let result = match &receiver {
            ErroObject::String(s) => self.dispatch_string_method(s, &method_name, &args),
            ErroObject::Array(arr) => self.dispatch_array_method(arr, &method_name, args),
            ErroObject::Number(_) => self.dispatch_number_method(&receiver, &method_name),
            ErroObject::Boolean(_) => self.dispatch_bool_method(&receiver, &method_name),
            ErroObject::Object(obj) => self.dispatch_object_method(obj, &method_name, &args),
            _ => Err(format!(
                "{} does not have method '{}'",
                receiver.type_name(),
                method_name
            )),
        };

        match result {
            Ok(val) => self.stack.push(val),
            Err(msg) => {
                let mut err_map = HashMap::new();
                err_map.insert("msg".to_string(), ErroObject::String(msg));
                err_map.insert(
                    "type".to_string(),
                    ErroObject::String("MethodError".to_string()),
                );
                err_map.insert("line".to_string(), ErroObject::Number(0.0));
                let err_obj = ErroObject::Object(Rc::new(RefCell::new(err_map)));
                self.handle_runtime_error_obj(err_obj, frame);
            }
        }
    }

    fn dispatch_string_method(
        &self,
        s: &str,
        method: &str,
        args: &[ErroObject],
    ) -> Result<ErroObject, String> {
        match method {
            "len" => Ok(ErroObject::Number(s.len() as f64)),
            "upper" => Ok(ErroObject::String(s.to_uppercase())),
            "lower" => Ok(ErroObject::String(s.to_lowercase())),
            "type" => Ok(ErroObject::String("string".to_string())),
            "trim" => Ok(ErroObject::String(s.trim().to_string())),
            "split" => {
                let sep = match args.first() {
                    Some(ErroObject::String(sep)) => sep.as_str(),
                    _ => " ",
                };
                let parts: Vec<ErroObject> = s
                    .split(sep)
                    .map(|p| ErroObject::String(p.to_string()))
                    .collect();
                Ok(ErroObject::Array(Rc::new(RefCell::new(parts))))
            }
            "contains" => {
                if let Some(ErroObject::String(needle)) = args.first() {
                    Ok(ErroObject::Boolean(s.contains(needle.as_str())))
                } else {
                    Ok(ErroObject::Boolean(false))
                }
            }
            "starts_with" => {
                if let Some(ErroObject::String(prefix)) = args.first() {
                    Ok(ErroObject::Boolean(s.starts_with(prefix.as_str())))
                } else {
                    Ok(ErroObject::Boolean(false))
                }
            }
            "ends_with" => {
                if let Some(ErroObject::String(suffix)) = args.first() {
                    Ok(ErroObject::Boolean(s.ends_with(suffix.as_str())))
                } else {
                    Ok(ErroObject::Boolean(false))
                }
            }
            "replace" => {
                if args.len() >= 2 {
                    if let (ErroObject::String(from), ErroObject::String(to)) = (&args[0], &args[1])
                    {
                        return Ok(ErroObject::String(s.replace(from.as_str(), to.as_str())));
                    }
                }
                Ok(ErroObject::String(s.to_string()))
            }
            _ => Err(format!("String does not have method '{}'", method)),
        }
    }

    fn dispatch_array_method(
        &self,
        arr: &Rc<RefCell<Vec<ErroObject>>>,
        method: &str,
        args: Vec<ErroObject>,
    ) -> Result<ErroObject, String> {
        match method {
            "len" => Ok(ErroObject::Number(arr.borrow().len() as f64)),
            "type" => Ok(ErroObject::String("array".to_string())),
            "push" => {
                if let Some(val) = args.into_iter().next() {
                    arr.borrow_mut().push(val);
                }
                Ok(ErroObject::Null)
            }
            "pop" => Ok(arr.borrow_mut().pop().unwrap_or(ErroObject::Null)),
            "join" => {
                let sep = match args.first() {
                    Some(ErroObject::String(s)) => s.clone(),
                    _ => ",".to_string(),
                };
                let joined = arr
                    .borrow()
                    .iter()
                    .map(|e| e.inspect())
                    .collect::<Vec<_>>()
                    .join(&sep);
                Ok(ErroObject::String(joined))
            }
            "contains" => {
                if let Some(target) = args.first() {
                    let found = arr.borrow().iter().any(|e| e == target);
                    Ok(ErroObject::Boolean(found))
                } else {
                    Ok(ErroObject::Boolean(false))
                }
            }
            "reverse" => {
                arr.borrow_mut().reverse();
                Ok(ErroObject::Null)
            }
            _ => Err(format!("Array does not have method '{}'", method)),
        }
    }

    fn dispatch_number_method(
        &self,
        _receiver: &ErroObject,
        method: &str,
    ) -> Result<ErroObject, String> {
        match method {
            "type" => Ok(ErroObject::String("number".to_string())),
            _ => Err(format!("Number does not have method '{}'", method)),
        }
    }

    fn dispatch_bool_method(
        &self,
        _receiver: &ErroObject,
        method: &str,
    ) -> Result<ErroObject, String> {
        match method {
            "type" => Ok(ErroObject::String("boolean".to_string())),
            _ => Err(format!("Boolean does not have method '{}'", method)),
        }
    }

    fn dispatch_object_method(
        &self,
        obj: &Rc<RefCell<HashMap<String, ErroObject>>>,
        method: &str,
        args: &[ErroObject],
    ) -> Result<ErroObject, String> {
        match method {
            "type" => Ok(ErroObject::String("object".to_string())),
            "keys" => {
                let keys: Vec<ErroObject> = obj
                    .borrow()
                    .keys()
                    .map(|k| ErroObject::String(k.clone()))
                    .collect();
                Ok(ErroObject::Array(Rc::new(RefCell::new(keys))))
            }
            "values" => {
                let vals: Vec<ErroObject> = obj.borrow().values().cloned().collect();
                Ok(ErroObject::Array(Rc::new(RefCell::new(vals))))
            }
            _ => {
                let borrowed = obj.borrow();
                if let Some(val) = borrowed.get(method) {
                    if let ErroObject::NativeFunction(nf) = val {
                        Ok((nf.func)(args.to_vec()))
                    } else {
                        Ok(val.clone())
                    }
                } else {
                    Err(format!("Object does not have method '{}'", method))
                }
            }
        }
    }

    // ── Error handling ────────────────────────────────────────────────────────

    fn handle_runtime_error_obj(&mut self, err_obj: ErroObject, frame: &mut Frame) {
    if let Some(handler) = self.try_handlers.pop() {
        self.stack.truncate(handler.stack_depth);
        self.stack.push(err_obj);
        frame.ip = handler.catch_ip;
    } else {
        // Extract fields from the error object if it's a map
        let (msg, err_type, line, col) = match &err_obj {
            ErroObject::Object(map) => {
                let b = map.borrow();
                let msg = match b.get("msg") {
                    Some(ErroObject::String(s)) => s.clone(),
                    _ => err_obj.inspect(),
                };
                let err_type = match b.get("type") {
                    Some(ErroObject::String(s)) => s.clone(),
                    _ => "RuntimeError".to_string(),
                };
                let line = match b.get("line") {
                    Some(ErroObject::Number(n)) => *n as usize,
                    _ => 0,
                };
                let col = match b.get("col") {
                    Some(ErroObject::Number(n)) => *n as usize,
                    _ => 0,
                };
                (msg, err_type, line, col)
            }
            // Plain string error or any other type
            other => (other.inspect(), "RuntimeError".to_string(), 0, 0),
        };

        let reporter = crate::compiler::diagnostics::DiagnosticReporter::new(&self.source, &self.filename);
        let span = crate::compiler::lexer::Span { line, col };
        let formatted = reporter.report_error(span, &format!("[{}] {}", err_type, msg));
        eprintln!("{}", formatted);

        std::process::exit(1);
    }
}

    pub fn raise_error(
        &mut self,
        msg: &str,
        err_type: &str,
        line: usize,
        col: usize,
        frame: &mut Frame,
    ) {
        let mut err_map = HashMap::new();
        err_map.insert("msg".to_string(), ErroObject::String(msg.to_string()));
        err_map.insert("type".to_string(), ErroObject::String(err_type.to_string()));
        err_map.insert("line".to_string(), ErroObject::Number(line as f64));
        err_map.insert("col".to_string(), ErroObject::Number(col as f64));
        let err_obj = ErroObject::Object(Rc::new(RefCell::new(err_map)));
        self.handle_runtime_error_obj(err_obj, frame);
    }

    // ── Module import ─────────────────────────────────────────────────────────

    async fn handle_import(&mut self, path: &str, _constants: &[ErroObject]) {
        match path {
            "net" | "fs" | "json" | "random" | "os" | "crypto" => {
                let module = create_stdlib_module(path);
                self.stack.push(module);
            }
            _ => {
                if path.ends_with(".erx") {
                    match std::fs::read_to_string(path) {
                        Ok(source) => {
                            let lexer = crate::compiler::lexer::Lexer::new(source);
                            let mut parser = crate::compiler::parser::Parser::new(lexer);
                            let program = parser.parse_program();
                            let mut compiler = crate::compiler::codegen::Compiler::new();
                            compiler.compile(program);
                            let mut child_vm = VM::new();
                            // Share globals with child so imported symbols are visible
                            for (k, v) in &self.globals {
                                child_vm.globals.insert(*k, v.clone());
                            }
                            child_vm
                                .run(compiler.instructions, compiler.constants)
                                .await;
                            // Merge child globals back into parent
                            for (k, v) in child_vm.globals {
                                self.globals.insert(k, v);
                            }
                        }
                        Err(e) => eprintln!("[ImportError] Failed to read '{}': {}", path, e),
                    }
                } else {
                    eprintln!("[ImportError] Unknown module '{}'", path);
                }
                self.stack.push(ErroObject::Null);
            }
        }
    }
}

// ── Frame ────────────────────────────────────────────────────────────────────

impl Frame {
    pub fn new(instructions: Vec<Opcode>, ip: usize, upvalues: Vec<Rc<RefCell<Upvalue>>>) -> Self {
        Frame {
            instructions,
            ip,
            base_pointer: 0,
            upvalues,
        }
    }
}

// ── Built-in functions ───────────────────────────────────────────────────────

pub fn native_print(args: Vec<ErroObject>) -> ErroObject {
    for arg in args {
        print!("{}", arg.inspect());
    }
    println!();
    let _ = io::stdout().flush();
    ErroObject::Null
}

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

pub fn native_push(args: Vec<ErroObject>) -> ErroObject {
    if args.len() >= 2 {
        if let ErroObject::Array(ref arr) = args[0] {
            arr.borrow_mut().push(args[1].clone());
            return args[0].clone();
        }
    }
    ErroObject::Null
}

pub fn native_typeof(args: Vec<ErroObject>) -> ErroObject {
    if let Some(arg) = args.first() {
        ErroObject::String(arg.type_name().to_string())
    } else {
        ErroObject::String("undefined".to_string())
    }
}

// ── Standard library module factory ─────────────────────────────────────────

pub fn create_stdlib_module(name: &str) -> ErroObject {
    let mut map = HashMap::new();
    match name {
        "os" => {
            map.insert(
                "name".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "os.name",
                    func: stdlib_os_name,
                }),
            );
            map.insert(
                "env".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "os.env",
                    func: stdlib_os_env,
                }),
            );
            map.insert(
                "cmd".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "os.cmd",
                    func: stdlib_os_cmd,
                }),
            );
            map.insert(
                "cpu".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "os.cpu",
                    func: stdlib_os_cpu,
                }),
            );
        }
        "fs" => {
            map.insert(
                "read".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "fs.read",
                    func: stdlib_fs_read,
                }),
            );
            map.insert(
                "write".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "fs.write",
                    func: stdlib_fs_write,
                }),
            );
            map.insert(
                "add".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "fs.add",
                    func: stdlib_fs_add,
                }),
            );
            map.insert(
                "exists".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "fs.exists",
                    func: stdlib_fs_exists,
                }),
            );
            map.insert(
                "info".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "fs.info",
                    func: stdlib_fs_info,
                }),
            );
        }
        "json" => {
            map.insert(
                "to_obj".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "json.to_obj",
                    func: stdlib_json_to_obj,
                }),
            );
            map.insert(
                "to_txt".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "json.to_txt",
                    func: stdlib_json_to_txt,
                }),
            );
        }
        "random" => {
            map.insert(
                "range".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "random.range",
                    func: stdlib_random_range,
                }),
            );
            map.insert(
                "pick".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "random.pick",
                    func: stdlib_random_pick,
                }),
            );
            map.insert(
                "string".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "random.string",
                    func: stdlib_random_string,
                }),
            );
            map.insert(
                "bool".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "random.bool",
                    func: stdlib_random_bool,
                }),
            );
        }
        "crypto" => {
            map.insert(
                "md5".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "crypto.md5",
                    func: stdlib_crypto_md5,
                }),
            );
            map.insert(
                "sha256".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "crypto.sha256",
                    func: stdlib_crypto_sha256,
                }),
            );
            map.insert(
                "base64_enc".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "crypto.base64_enc",
                    func: stdlib_crypto_base64_enc,
                }),
            );
            map.insert(
                "base64_dec".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "crypto.base64_dec",
                    func: stdlib_crypto_base64_dec,
                }),
            );
        }
        "net" => {
            map.insert(
                "get".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "net.get",
                    func: stdlib_net_get,
                }),
            );
            map.insert(
                "post".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "net.post",
                    func: stdlib_net_post,
                }),
            );
            map.insert(
                "ping".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "net.ping",
                    func: stdlib_net_ping,
                }),
            );
            map.insert(
                "ip".to_string(),
                ErroObject::NativeFunction(NativeFnWrapper {
                    name: "net.ip",
                    func: stdlib_net_ip,
                }),
            );
        }
        _ => {}
    }
    ErroObject::Object(Rc::new(RefCell::new(map)))
}

// ── OS module ────────────────────────────────────────────────────────────────

fn stdlib_os_name(_args: Vec<ErroObject>) -> ErroObject {
    ErroObject::String(std::env::consts::OS.to_string())
}
fn stdlib_os_env(args: Vec<ErroObject>) -> ErroObject {
    if let Some(ErroObject::String(key)) = args.first() {
        match std::env::var(key) {
            Ok(val) => ErroObject::String(val),
            Err(_) => ErroObject::Null,
        }
    } else {
        ErroObject::Null
    }
}
fn stdlib_os_cmd(args: Vec<ErroObject>) -> ErroObject {
    if let Some(ErroObject::String(cmd)) = args.first() {
        let output = if cfg!(target_os = "windows") {
            Command::new("cmd").args(["/C", cmd]).output()
        } else {
            Command::new("sh").args(["-c", cmd]).output()
        };
        match output {
            Ok(out) => ErroObject::String(String::from_utf8_lossy(&out.stdout).trim().to_string()),
            Err(_) => ErroObject::Null,
        }
    } else {
        ErroObject::Null
    }
}
fn stdlib_os_cpu(_args: Vec<ErroObject>) -> ErroObject {
    ErroObject::String(std::env::consts::ARCH.to_string())
}

// ── FS module ─────────────────────────────────────────────────────────────────

fn stdlib_fs_read(args: Vec<ErroObject>) -> ErroObject {
    if let Some(ErroObject::String(path)) = args.first() {
        match std::fs::read_to_string(path) {
            Ok(content) => ErroObject::String(content),
            Err(e) => ErroObject::String(format!("Error: {}", e)),
        }
    } else {
        ErroObject::Null
    }
}
fn stdlib_fs_write(args: Vec<ErroObject>) -> ErroObject {
    if args.len() >= 2 {
        if let (ErroObject::String(path), data) = (&args[0], &args[1]) {
            return ErroObject::Boolean(std::fs::write(path, data.inspect()).is_ok());
        }
    }
    ErroObject::Boolean(false)
}
fn stdlib_fs_add(args: Vec<ErroObject>) -> ErroObject {
    if args.len() >= 2 {
        if let (ErroObject::String(path), data) = (&args[0], &args[1]) {
            use std::fs::OpenOptions;
            return match OpenOptions::new().append(true).create(true).open(path) {
                Ok(mut file) => {
                    let _ = file.write_all(data.inspect().as_bytes());
                    ErroObject::Boolean(true)
                }
                Err(_) => ErroObject::Boolean(false),
            };
        }
    }
    ErroObject::Boolean(false)
}
fn stdlib_fs_exists(args: Vec<ErroObject>) -> ErroObject {
    if let Some(ErroObject::String(path)) = args.first() {
        ErroObject::Boolean(std::path::Path::new(path).exists())
    } else {
        ErroObject::Boolean(false)
    }
}
fn stdlib_fs_info(args: Vec<ErroObject>) -> ErroObject {
    if let Some(ErroObject::String(path)) = args.first() {
        match std::fs::metadata(path) {
            Ok(meta) => {
                let mut map = HashMap::new();
                map.insert("size".to_string(), ErroObject::Number(meta.len() as f64));
                map.insert("is_dir".to_string(), ErroObject::Boolean(meta.is_dir()));
                map.insert("is_file".to_string(), ErroObject::Boolean(meta.is_file()));
                ErroObject::Object(Rc::new(RefCell::new(map)))
            }
            Err(_) => ErroObject::Null,
        }
    } else {
        ErroObject::Null
    }
}

// ── JSON module ───────────────────────────────────────────────────────────────

fn stdlib_json_to_obj(args: Vec<ErroObject>) -> ErroObject {
    if let Some(ErroObject::String(s)) = args.first() {
        match serde_json::from_str::<serde_json::Value>(s) {
            Ok(val) => json_value_to_erox(&val),
            Err(_) => ErroObject::Null,
        }
    } else {
        ErroObject::Null
    }
}
fn json_value_to_erox(val: &serde_json::Value) -> ErroObject {
    match val {
        serde_json::Value::Null => ErroObject::Null,
        serde_json::Value::Bool(b) => ErroObject::Boolean(*b),
        serde_json::Value::Number(n) => ErroObject::Number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => ErroObject::String(s.clone()),
        serde_json::Value::Array(arr) => {
            let items: Vec<ErroObject> = arr.iter().map(json_value_to_erox).collect();
            ErroObject::Array(Rc::new(RefCell::new(items)))
        }
        serde_json::Value::Object(obj) => {
            let mut map = HashMap::new();
            for (k, v) in obj {
                map.insert(k.clone(), json_value_to_erox(v));
            }
            ErroObject::Object(Rc::new(RefCell::new(map)))
        }
    }
}
fn stdlib_json_to_txt(args: Vec<ErroObject>) -> ErroObject {
    if let Some(obj) = args.first() {
        let val = erox_to_json_value(obj);
        ErroObject::String(serde_json::to_string_pretty(&val).unwrap_or_default())
    } else {
        ErroObject::Null
    }
}
fn erox_to_json_value(obj: &ErroObject) -> serde_json::Value {
    match obj {
        ErroObject::Null => serde_json::Value::Null,
        ErroObject::Boolean(b) => serde_json::Value::Bool(*b),
        ErroObject::Number(n) => serde_json::json!(*n),
        ErroObject::String(s) => serde_json::Value::String(s.clone()),
        ErroObject::Array(arr) => {
            let items: Vec<serde_json::Value> =
                arr.borrow().iter().map(erox_to_json_value).collect();
            serde_json::Value::Array(items)
        }
        ErroObject::Object(obj) => {
            let mut map = serde_json::Map::new();
            for (k, v) in obj.borrow().iter() {
                map.insert(k.clone(), erox_to_json_value(v));
            }
            serde_json::Value::Object(map)
        }
        _ => serde_json::Value::Null,
    }
}

// ── Random module ─────────────────────────────────────────────────────────────

fn stdlib_random_range(args: Vec<ErroObject>) -> ErroObject {
    if args.len() >= 2 {
        if let (ErroObject::Number(min), ErroObject::Number(max)) = (&args[0], &args[1]) {
            let val = rand::random_range((*min as i64)..=(*max as i64));
            return ErroObject::Number(val as f64);
        }
    }
    ErroObject::Number(0.0)
}
fn stdlib_random_pick(args: Vec<ErroObject>) -> ErroObject {
    if let Some(ErroObject::Array(arr)) = args.first() {
        let borrowed = arr.borrow();
        if !borrowed.is_empty() {
            let idx = rand::random_range(0..borrowed.len());
            return borrowed[idx].clone();
        }
    }
    ErroObject::Null
}
fn stdlib_random_string(args: Vec<ErroObject>) -> ErroObject {
    let len = match args.first() {
        Some(ErroObject::Number(n)) => *n as usize,
        _ => 8,
    };
    let chars: String = (0..len)
        .map(|_| {
            let idx = rand::random_range(0u8..36u8);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + (idx - 10)) as char
            }
        })
        .collect();
    ErroObject::String(chars)
}
fn stdlib_random_bool(_args: Vec<ErroObject>) -> ErroObject {
    ErroObject::Boolean(rand::random_bool(0.5))
}

// ── Crypto module ─────────────────────────────────────────────────────────────

fn stdlib_crypto_md5(args: Vec<ErroObject>) -> ErroObject {
    if let Some(ErroObject::String(text)) = args.first() {
        use md5::{Digest, Md5};
        let hash = Md5::digest(text.as_bytes());
        let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
        ErroObject::String(hex)
    } else {
        ErroObject::Null
    }
}
fn stdlib_crypto_sha256(args: Vec<ErroObject>) -> ErroObject {
    if let Some(ErroObject::String(text)) = args.first() {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(text.as_bytes());
        let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
        ErroObject::String(hex)
    } else {
        ErroObject::Null
    }
}
fn stdlib_crypto_base64_enc(args: Vec<ErroObject>) -> ErroObject {
    if let Some(ErroObject::String(text)) = args.first() {
        use base64::Engine;
        ErroObject::String(base64::engine::general_purpose::STANDARD.encode(text.as_bytes()))
    } else {
        ErroObject::Null
    }
}
fn stdlib_crypto_base64_dec(args: Vec<ErroObject>) -> ErroObject {
    if let Some(ErroObject::String(text)) = args.first() {
        use base64::Engine;
        match base64::engine::general_purpose::STANDARD.decode(text.as_bytes()) {
            Ok(bytes) => ErroObject::String(String::from_utf8_lossy(&bytes).to_string()),
            Err(_) => ErroObject::Null,
        }
    } else {
        ErroObject::Null
    }
}

// ── Net module ────────────────────────────────────────────────────────────────

fn stdlib_net_get(args: Vec<ErroObject>) -> ErroObject {
    if let Some(ErroObject::String(url)) = args.first() {
        match reqwest::blocking::get(url) {
            Ok(resp) => ErroObject::String(resp.text().unwrap_or_default()),
            Err(e) => ErroObject::String(format!("Error: {}", e)),
        }
    } else {
        ErroObject::Null
    }
}
fn stdlib_net_post(args: Vec<ErroObject>) -> ErroObject {
    if args.len() >= 2 {
        if let (ErroObject::String(url), data) = (&args[0], &args[1]) {
            let client = reqwest::blocking::Client::new();
            return match client.post(url).body(data.inspect()).send() {
                Ok(resp) => ErroObject::String(resp.text().unwrap_or_default()),
                Err(e) => ErroObject::String(format!("Error: {}", e)),
            };
        }
    }
    ErroObject::Null
}
fn stdlib_net_ping(args: Vec<ErroObject>) -> ErroObject {
    if let Some(ErroObject::String(host)) = args.first() {
        let output = if cfg!(target_os = "windows") {
            Command::new("ping")
                .args(["-n", "1", "-w", "2000", host])
                .output()
        } else {
            Command::new("ping")
                .args(["-c", "1", "-W", "2", host])
                .output()
        };
        match output {
            Ok(out) => ErroObject::Boolean(out.status.success()),
            Err(_) => ErroObject::Boolean(false),
        }
    } else {
        ErroObject::Boolean(false)
    }
}
fn stdlib_net_ip(_args: Vec<ErroObject>) -> ErroObject {
    let output = if cfg!(target_os = "windows") {
        Command::new("powershell")
            .args(["-Command", "(Get-NetIPAddress -AddressFamily IPv4 | Where-Object { $_.InterfaceAlias -ne 'Loopback*' } | Select-Object -First 1).IPAddress"])
            .output()
    } else {
        Command::new("hostname").arg("-I").output()
    };
    match output {
        Ok(out) => {
            let ip = String::from_utf8_lossy(&out.stdout)
                .trim()
                .split_whitespace()
                .next()
                .unwrap_or("unknown")
                .to_string();
            ErroObject::String(ip)
        }
        Err(_) => ErroObject::String("unknown".to_string()),
    }
}
