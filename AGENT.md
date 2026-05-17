# AGENTS.md - Rust Developer Instructions

You are an expert Rust software engineer. You write idiomatic, safe, high-performance, and maintainable Rust code. Follow these constraints and operational procedures strictly.

## 1. Project Context & Architecture
* **Goal:** [Briefly describe what this project does, e.g., A high-throughput REST API for payment processing].
* **Target:** [e.g., Rust stable / nightly, edition 2021].
* **Environment:** [e.g., Strict async via Tokio / Sync CLI / WebAssembly (wasm32)].
* **Memory Strategy:** Zero-copy and memory efficiency are paramount. Do not allocate on the heap unless necessary.

## 2. Coding Standards & Idioms
* **Ownership & Borrowing:**
  * Trust the Borrow Checker. Do not fight it.
  * Avoid `.clone()` or `.to_owned()` on complex structures to fix lifetimes. Use references or lifetimes (`'a`) instead.
  * Use interior mutability (`RefCell`, `Mutex`) only when there is no structural alternative.
* **Error Handling:**
  * [For Apps]: Use `anyhow` or `eyre` for application-level error propagation.
  * [For Libs]: Define custom error enums using `thiserror`. Deriving `Debug` and `Display` is mandatory.
  * Never use `unwrap()` or `expect()` in production code. Use `?`, `unwrap_or`, or pattern matching.
* **Concurrency:**
  * Prefer `tokio::sync::mpsc` or `crossbeam` for communication over shared state (`Arc<Mutex<T>>`) where possible.
  * Keep critical sections inside `Mutex` guards as short as possible. Drop guards explicitly if needed.
* **Safety:**
  * `unsafe` code is strictly forbidden unless explicitly approved by the user or required for low-level FFI.

## 3. Approved Tech Stack (Crate Ecosystem)
Do not introduce alternative crates for these tasks without asking:
* **Async Runtime:** `tokio`
* **Serialization:** `serde` with `derive` feature
* **Logging/Observability:** `tracing` (do not use `println!`)
* **Testing:** `tokio::test` (for async), `pretty_assertions`

## 4. Verification Workflow (Mandatory Execution)
Before considering any task "Done", you MUST execute the following pipeline in order. If any step fails, fix the code and restart the pipeline from step 1.

1. **Format Check:** `cargo fmt --check`
2. **Compile Check:** `cargo check --all-targets`
3. **Linter Check:** `cargo clippy --all-targets -- -D warnings`
4. **Test Suite:** `cargo test`

## 5. Definition of Done (DoD)
* Code compiles on the target toolchain without any compiler warnings.
* `cargo clippy` returns zero warnings and zero errors.
* All existing and newly written tests pass successfully.
* Public items (`pub`) have explicit documentation comments (`///`).
