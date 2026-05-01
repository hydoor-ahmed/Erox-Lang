# Project-Structure: EROX Language 🛰️

Binary Name: erox
File Extension: .erx
Core Engine: EROX-VM

## 🎯 Goal

Build a high-performance, memory-safe, and concurrent programming language using Rust to compete with Python's versatility and C++'s speed.

## 🏗️ System Architecture

- **Frontend:** Lexer -> Parser (Pratt) -> Compiler (Codegen).
- **Backend:** Stack-based Virtual Machine (VM).

## 🏗️ Core Architecture

- **Engine:** Stack-based Bytecode Virtual Machine.
- **Functions:** Support for Arrow Functions `() => {}` and Closures.
- **Memory:** Hybrid Stack/Heap management via Upvalues for Closures.
- **Module System:** Python-style `import` and `from ... import ...` support. [COMPLETED]
- **Shell Integration:** Dynamic execution of shell commands via `$ expr $`. [COMPLETED] ✅
- **String Interpolation:** High-performance `${expression}` inside strings. [COMPLETED]

## 📁 File Map

- `/src/compiler`: Handling the source code analysis.
- `/src/vm`: The execution engine (High-speed).

## 📁 Implementation Map

- `src/vm/opcodes.rs`: Comprehensive Instruction Set with Shell support. [COMPLETED]
- `src/vm/object.rs`: RC-based Object System with Native Function support. [COMPLETED] ✅
- `src/vm/machine.rs`: EROX-VM execution engine with cross-platform Shell execution. [COMPLETED] ✅
- `src/compiler/lexer.rs`: Tokenizer with Shell and Interpolation support. [COMPLETED] ✅
- `src/compiler/parser.rs`: Pratt Parser with Dynamic Shell support. [COMPLETED] ✅
- `src/compiler/codegen.rs`: Bytecode generator for all EROX features. [COMPLETED] ✅

## 🚀 TODO List

- [x] Initialize Cargo Project (Renamed to EROX).
- [x] Define Basic Syntax.
- [x] Build Lexer for Keywords and Identifiers (incl. `import`, `from`).
- [x] Implement String Interpolation logic.
- [x] Implement Dynamic Shell Execution syntax. ✅
- [x] Full Frontend (Lexer + Parser + Arrow Functions ✅).
- [x] Instruction Set (Opcodes).
- [x] Bytecode Compiler (Emitting Instructions).
- [x] Virtual Machine (Stack Execution & Globals).
- [x] Implement Function Calls & Call Frames (100% Operational ✅).
- [x] Implement Closures & Upvalue Capture (100% Operational ✅).
- [x] Memory Management (Closing Upvalues ✅).
- [x] Native Print Function & Real-time Flushing. ✅
- [X] Concurrency: Async/Await System. 🔴

## 🛠️ Current Engineering Focus
Finalizing the **Concurrency System (Async/Await)**. The core language features including dynamic modules, shell integration, and interpolation are now fully operational and optimized.

## 🛠️ Tech Stack

- **Language:** Rust 🦀
- **OS:** Cross-platform (Linux/macOS/Windows) 🐧🍎🪟
- **Environment:** Performance-oriented.
