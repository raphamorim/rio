# Rio Terminal - Development Guide

## Build Commands
- `cargo build --release` - Release build
- `cargo run -p rioterm --release` - Run Rio terminal
- `cargo run -p rioterm` - Development run
- `MTL_HUD_ENABLED=1 cargo run -p rioterm` - Dev with Metal HUD (macOS)
- `RIO_LOG_LEVEL=debug cargo run -p rioterm` - Debug logging

## Test Commands
- `cargo test --release` - Run all tests
- `cargo test -p rio-backend` - Test specific package
- `cargo test test_name` - Run single test
- `RUST_BACKTRACE=full cargo test` - Tests with full backtrace

## Lint/Format Commands
- `cargo fmt` - Format code (max_width=90, tab_spaces=4)
- `cargo fmt -- --check` - Check formatting
- `cargo clippy --all-targets --all-features -- -D warnings` - Lint

## Code Style Guidelines
- **Imports**: std → external crates → local modules, grouped logically
- **Naming**: snake_case (functions/vars), PascalCase (types), SCREAMING_SNAKE_CASE (constants)
- **Error handling**: Custom error types with `From` traits, `Result<T, Box<dyn Error>>`
- **Logging**: Use `tracing` crate for structured logging
- **Config**: Serde with defaults, validation, and fallbacks
- **Platform code**: Use `#[cfg(target_os = "...")]` for platform-specific code
- **Documentation**: Inline comments for complex logic, TODO/FIXME for improvements
- **Testing**: Comprehensive test modules with `#[cfg(test)]`
- **Performance**: Use `Arc<FairMutex<T>>` for shared state, efficient buffer management
- **Modules**: Well-organized hierarchy with `mod.rs` files