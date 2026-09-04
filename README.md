# ini-preserve

[![Crates.io](https://img.shields.io/crates/v/ini-preserve.svg)](https://crates.io/crates/ini-preserve)
[![Documentation](https://docs.rs/ini-preserve/badge.svg)](https://docs.rs/ini-preserve)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Discord](https://img.shields.io/badge/Discord-Le--Syl21%20Tools-5865F2?logo=discord&logoColor=white)](https://discord.gg/T37DYHmt2j)

**Format-preserving INI parser for Rust.**

Read, modify and write back INI files **without losing comments, ordering or formatting**.

Unlike most INI parsers that discard comments and reorder sections when writing,
`ini-preserve` keeps the original file structure intact. Only the values you explicitly
change are modified. Everything else (comments, blank lines, key order, spacing around `=`) is preserved.

## Community & support

Questions, bug reports, beta testing, or just want to chat? Join the Discord:

[![Discord](https://img.shields.io/badge/Discord-Le--Syl21%20Tools-5865F2?logo=discord&logoColor=white)](https://discord.gg/T37DYHmt2j)

## Features

- **Round-trip safe** — parse and write back produces identical output
- **Comments preserved** — `;` and `#` comment lines are kept as-is
- **Ordering preserved** — sections and keys stay in their original order
- **Spacing preserved** — `Key=Value`, `Key = Value`, `Key =Value` all keep their style
- **Semicolons in values** — `Key = foo;bar` works correctly (`;` is not treated as inline comment)
- **Atomic writes** — `save()` writes to a temp file then renames
- **No dependencies** — pure Rust, no external crates
- **Simple API** — `load()`, `get()`, `set()`, `remove()`, `add_section()`, `remove_section()`, `save()`

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

// Add a section, or remove a single key, or a whole section with its contents
ini.add_section("Standalone");
ini.remove("Player", "Height");
ini.remove_section("Obsolete");

// Save back — comments and formatting preserved
ini.save("config.ini").unwrap();
```

`set()` already creates a section when it writes a key into one that does not
exist, so `add_section()` is for the case where the header is wanted on its
own. Either way exactly one blank line separates it from what precedes it.

A section runs from its header to just before the next one, so `remove_section`
takes its keys, its comments and the blank line that followed it. Comments
written *above* the header are left alone — they are as likely to be a
file-level banner as a description of the section.

## Why?

Many applications (like VPinballX) generate INI files with extensive comments documenting
every setting and its default value. Using a standard INI parser to modify one value
destroys all those comments. `ini-preserve` solves this by treating the file as a
sequence of lines and only modifying the specific lines that need to change.

## License

MIT
