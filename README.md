# ini-preserve

[![Crates.io](https://img.shields.io/crates/v/ini-preserve.svg)](https://crates.io/crates/ini-preserve)
[![Documentation](https://docs.rs/ini-preserve/badge.svg)](https://docs.rs/ini-preserve)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**Format-preserving INI parser for Rust.**

Read, modify and write back INI files **without losing comments, ordering or formatting**.

Unlike most INI parsers that discard comments and reorder sections when writing,
`ini-preserve` keeps the original file structure intact. Only the values you explicitly
change are modified. Everything else (comments, blank lines, key order, spacing around `=`) is preserved.

## Features

- **Round-trip safe** — parse and write back produces identical output
- **Comments preserved** — `;` and `#` comment lines are kept as-is
- **Ordering preserved** — sections and keys stay in their original order
- **Spacing preserved** — `Key=Value`, `Key = Value`, `Key =Value` all keep their style
- **Semicolons in values** — `Key = foo;bar` works correctly (`;` is not treated as inline comment)
- **Atomic writes** — `save()` writes to a temp file then renames
- **No dependencies** — pure Rust, no external crates
- **Simple API** — `load()`, `get()`, `set()`, `save()`

## Usage

```rust
use ini_preserve::Ini;

// Load an existing INI file
let mut ini = Ini::load("config.ini").unwrap();

// Read values
if let Some(value) = ini.get("Player", "Width") {
    println!("Width = {}", value);
}

// Modify values (only changed lines are rewritten)
ini.set("Player", "Width", "3840");
ini.set("Player", "Height", "2160");

// Save back — comments and formatting preserved
ini.save("config.ini").unwrap();
```

## Why?

Many applications (like VPinballX) generate INI files with extensive comments documenting
every setting and its default value. Using a standard INI parser to modify one value
destroys all those comments. `ini-preserve` solves this by treating the file as a
sequence of lines and only modifying the specific lines that need to change.

## License

MIT
