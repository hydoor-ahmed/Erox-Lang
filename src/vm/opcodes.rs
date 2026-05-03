#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Opcode {
    OpConstant(usize),
    OpAdd,
    OpSub,
    OpMul,
    OpDiv,
    OpModulo,
    OpTrue,
    OpFalse,
    OpNull,
    OpEqual,
    OpNotEqual,
    OpGT,
    OpLT,
    OpGTE,
    OpLTE,
    OpNot,
    OpPop,
    OpSetGlobal(usize),
    OpGetGlobal(usize),
    OpSetLocal(usize),
    OpGetLocal(usize),
    OpJump(usize),
    OpJumpIfFalse(usize),
    OpCall(usize),
    OpReturnValue,
    OpGetUpvalue(usize),
    OpSetUpvalue(usize),
    OpClosure(usize, usize),
    OpShellExecute,
    // Data structure opcodes
    OpArray(usize),       // Pop N elements, create array, push
    OpObject(usize),      // Pop N key-value pairs, create object, push
    OpIndex,              // Pop index + collection, push element
    OpSetIndex,           // Pop value + index + collection, set element, push value
    // Method dispatch
    OpMethodCall(usize),  // Pop receiver + method_name_const + args, dispatch method. usize = arg count
    // Exception handling
    OpTryStart(usize),    // Push try handler, usize = catch block address
    OpTryEnd,             // Pop try handler (successful try completion)
    OpThrow,              // Pop TOS as error, jump to catch handler
    // Module system
    OpImport,             // Pop TOS (path string), execute import
    // Async runtime
    OpAwait,              // Pop TOS (Future), suspend until resolved, push result
}
