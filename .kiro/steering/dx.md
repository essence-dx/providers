# Rust DX Project Standards

Today is 2 Arpil 2026 so please do web search and mention latest details "12 March 2026" details!!!

## CRITICAL: User Intent First
- **DO EXACTLY WHAT THE USER ASKS** - nothing more, nothing less
- If the user says "fix X", fix X - don't change Y or Z
- If you can't do what the user asks, ASK CLARIFICATION QUESTIONS
- NEVER make assumptions about what the user "really wants"
- NEVER add "improvements" or "optimizations" that weren't requested
- NEVER change defaults, behaviors, or unrelated code without explicit permission

## CRITICAL: Three-Strike Rule for Complex Tasks
- **If you attempt the same task 3 times and fail**, STOP trying
- **Create a `HELP_NEEDED_*.md` file** documenting:
  1. What you tried to accomplish
  2. What approaches you attempted (all 3 attempts)
  3. Why each attempt failed
  4. Complete configuration/code details needed
  5. Reference implementations (if available)
  6. Latest web search results (with current date)
  7. Clear request for expert help from a better model
  8. Expected outcome and success criteria
- **DO NOT create stub code** - only create documentation
- **DO NOT keep retrying** - accept your limitations
- **Examples of when to use this:**
  - Complex OAuth implementations you can't complete
  - Advanced async/concurrency patterns beyond your capability
  - Integration with unfamiliar APIs after 3 failed attempts
  - Any task where you're stuck in a retry loop
- **See `HELP_NEEDED_GEMINI_OAUTH.md` and `HELP_NEEDED_QWEN_OAUTH.md` as examples**

## Critical Rules

### Error Handling
- NEVER use `.unwrap()` or `.expect()` in library code - always return `Result<T, E>`
- Use `thiserror` for custom error types with descriptive messages
- Propagate errors with `?` operator
- Only panic in truly unrecoverable situations (document why)

### Code Quality Standards
- Run `cargo clippy -- -D warnings` before committing (zero warnings policy)
- Format all code with `cargo fmt`
- Use `#[must_use]` on functions returning important values
- Implement `Debug` for all public types, `Display` for user-facing types

### Documentation Requirements
- Document ALL public APIs with `///` doc comments
- Include usage examples in doc comments for complex APIs
- Add `# Examples` and `# Errors` sections where applicable
- Keep internal implementation details private

## Architecture Patterns

### Type Safety
- Use newtype pattern to wrap primitives (e.g., `struct UserId(u64)`)
- Leverage type state pattern for compile-time state guarantees
- Make invalid states unrepresentable through types

### Resource Management
- Use RAII pattern - implement `Drop` for cleanup
- Prefer owned types, use `Cow<'_, T>` for conditional ownership
- Use `Arc` for shared ownership, `Rc` for single-threaded sharing

### Async/Concurrency
- Use `async`/`await` for I/O-bound operations
- Choose appropriate synchronization: `Mutex` for exclusive access, `RwLock` for read-heavy workloads
- Use `RefCell` only for single-threaded interior mutability

## Module Organization

This project structure:
- `src/lib.rs` - Library root, re-exports public API
- `src/prompts/` - Individual prompt implementations (input, select, confirm, etc.)
- `src/ui/` - UI components, animations, chat interface
- `src/llm/` - LLM provider integrations and configurations
- `src/theme.rs` - Theming and styling
- `src/tui.rs` - Terminal UI utilities

When adding new features:
- Create focused modules with single responsibility
- Use `mod.rs` for module organization or file-per-module
- Keep public API minimal - expose only what users need
- Group related functionality together

## Testing Strategy

### Test Organization
- Unit tests: Use `#[cfg(test)]` module in same file
- Integration tests: Place in `tests/` directory
- Property-based tests: Use `proptest` for complex logic validation

### Test Commands (LOW-END DEVICE - SKIP THESE)
```bash
# DO NOT RUN ON LOW-END DEVICE:
# cargo test --all-features
# cargo test --lib
# cargo build --release
```

### Coverage Goals
- Critical paths: 80%+ coverage
- Public APIs: 100% coverage
- Error handling: Test all error branches

## Dependencies

### Dependency Guidelines
- Minimize dependencies - each adds compile time and maintenance burden
- Prefer well-maintained crates with active communities
- Check crate versions and compatibility before adding
- Use `cargo audit` regularly for security vulnerabilities

### Version Management
- Pin major versions in `Cargo.toml` (e.g., `serde = "1.0"`)
- Use `cargo update` cautiously, test after updates
- Document why each dependency is needed

## Performance Considerations

- Profile before optimizing (`cargo flamegraph`, `perf`)
- Avoid allocations in hot paths
- Use `#[inline]` for small, frequently-called functions
- Prefer iterators over collecting to intermediate `Vec`
- Use `&str` over `String` when ownership not needed

## Shell Commands (Git Bash)

Always use git bash commands

## Pre-Commit Checklist

Before committing code, verify:
1. `cargo fmt` - Code is formatted
2. `cargo clippy -- -D warnings` - No clippy warnings
3. `cargo run` - Binary runs successfully (LOW-END DEVICE: skip tests/builds)
4. Public APIs are documented
5. No `.unwrap()` or `.expect()` in library code
6. Error handling is comprehensive

## Web Search Triggers

Use web search tools when:
- Checking latest crate versions or API changes
- Verifying current Rust best practices or RFC status
- Researching error messages or compatibility issues
- Looking up design patterns or idiomatic solutions
- Confirming feature availability in Rust edition
