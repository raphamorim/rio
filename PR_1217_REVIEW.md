# PR #1217 Review: Canonicalize working-dir argument

## Summary
The PR adds path canonicalization to the `--working-dir` CLI argument to resolve relative paths and symlinks to absolute paths.

## Issues & Recommendations

### 1. Error Handling Problem
**Issue**: The code uses `?` operator on `canonicalize()` but then catches conversion errors with `match`. This is inconsistent - if canonicalization fails, the program will panic due to `?`, but conversion errors are handled gracefully.

**Current Code:**
```rust
let wd_canonical = match std::fs::canonicalize(&working_dir_cli)? 
    .into_os_string()
    .into_string() 
{
    Ok(x) => Some(x),
    Err(_) => {
        eprintln!("There was a problem when canonicalizing the input path {}. Opening at the default path instead.", working_dir_cli);
        None
    }
};
```

**Recommended Fix:**
```rust
let wd_canonical = match std::fs::canonicalize(&working_dir_cli)
    .and_then(|path| path.into_os_string().into_string().map_err(|_| 
        std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid UTF-8 in path")))
{
    Ok(canonical_path) => Some(canonical_path),
    Err(e) => {
        tracing::warn!("Failed to canonicalize working directory '{}': {}. Using default instead.", working_dir_cli, e);
        None
    }
};
```

### 2. Logging Consistency
**Issue**: The code uses `eprintln!` directly instead of the `tracing` crate used elsewhere in the codebase.

**Recommendation**: Use `tracing::warn!` for consistency with the rest of the codebase.

### 3. Path Validation Enhancement
**Recommendation**: Consider validating that the canonicalized path is actually a directory:

```rust
let wd_canonical = match std::fs::canonicalize(&working_dir_cli)
    .and_then(|path| {
        if path.is_dir() {
            path.into_os_string().into_string().map_err(|_| 
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid UTF-8 in path"))
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::NotADirectory, "Path is not a directory"))
        }
    })
{
    Ok(canonical_path) => Some(canonical_path),
    Err(e) => {
        tracing::warn!("Failed to set working directory '{}': {}. Using default instead.", working_dir_cli, e);
        None
    }
};
```

## Positive Aspects
- **Good Intent**: Canonicalizing paths is a good practice for CLI tools
- **Graceful Fallback**: Falls back to default when canonicalization fails
- **User Feedback**: Provides clear error message to user

## Overall Recommendation
**Approve with suggested improvements** - The core functionality is sound but error handling could be more robust and consistent with the codebase patterns.

## Code Style Compliance
- ✅ Follows Rio's error handling patterns (with suggested improvements)
- ⚠️ Should use `tracing` instead of `eprintln!` for logging
- ✅ Proper variable naming (`wd_canonical`)
- ✅ Appropriate use of `match` for error handling