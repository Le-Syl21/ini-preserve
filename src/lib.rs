//! # ini-preserve
//!
//! Format-preserving INI parser for Rust.
//!
//! Read, modify and write back INI files **without losing comments, ordering or formatting**.
//!
//! Unlike most INI parsers that discard comments and reorder sections when writing,
//! `ini-preserve` keeps the original file structure intact. Only the values you explicitly
//! change are modified — everything else (comments, blank lines, key order, spacing) is preserved.
//!
//! ## Example
//!
//! ```rust
//! use ini_preserve::Ini;
//!
//! let input = "\
//! ; Global settings
//! [Player]
//! ; Screen resolution
//! Width = 1920
//! Height = 1080
//! ";
//!
//! let mut ini = Ini::parse(input).unwrap();
//! assert_eq!(ini.get("Player", "Width"), Some("1920"));
//!
//! ini.set("Player", "Width", "3840");
//! ini.set("Player", "Height", "2160");
//!
//! let output = ini.to_string();
//! assert!(output.contains("; Global settings"));
//! assert!(output.contains("; Screen resolution"));
//! assert!(output.contains("Width = 3840"));
//! assert!(output.contains("Height = 2160"));
//! ```
//!
//! # Community & support
//!
//! Questions, bugs, beta testing — join the Discord: <https://discord.gg/T37DYHmt2j>

use std::path::Path;

/// A single line in the INI file.
#[derive(Clone, Debug)]
enum Line {
    /// A blank line (preserved as-is)
    Blank(String),
    /// A comment line starting with ; or # (preserved as-is)
    Comment(String),
    /// A section header like [SectionName]
    Section {
        /// Raw line (e.g. "[Player]")
        raw: String,
        /// Parsed section name (e.g. "Player")
        name: String,
    },
    /// A key=value property
    Property {
        /// Raw line before modification (e.g. "Width = 1920")
        raw: String,
        /// Parsed key (e.g. "Width")
        key: String,
        /// Current value (may differ from raw if modified)
        value: String,
        /// Has this property been modified since parsing?
        modified: bool,
    },
}

/// A format-preserving INI document.
///
/// Stores every line of the original file. When writing back, unmodified lines
/// are output exactly as they were read. Modified properties are written with
/// the original key and spacing style.
#[derive(Clone, Debug)]
pub struct Ini {
    lines: Vec<Line>,
    /// The line ending of the source (its first line), given to lines added
    /// later so a Windows file does not end up with mixed endings. Lines read
    /// from the source keep their own: a `\r` stays inside their `raw` text.
    crlf: bool,
    /// Whether the source ended with a newline. Without it, the last line is
    /// written without one too, or an unmodified file would not round-trip.
    final_newline: bool,
    /// How keys added by `set` are written.
    key_style: KeyStyle,
}

/// How [`Ini::set`] writes a key that is not in the document yet.
///
/// Existing lines never change their spacing: a modified value keeps the
/// spacing of the line it replaces. This only decides the shape of lines the
/// crate creates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyStyle {
    /// Follow the document: copy the spacing of the last key of the target
    /// section, or of the last key before it in the file, so a `key=value`
    /// file (Qt's `QSettings`, for one) gets `key=value` lines. With no key
    /// to copy from, fall back to [`Spaced`][KeyStyle::Spaced].
    #[default]
    Auto,
    /// `key = value`
    Spaced,
    /// `key=value`, as `QSettings` writes it.
    Compact,
}

impl Ini {
    /// Create an empty INI document.
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            crlf: false,
            final_newline: true,
            key_style: KeyStyle::default(),
        }
    }

    /// Choose how keys added by [`set`][Self::set] are written.
    ///
    /// ```rust
    /// use ini_preserve::{Ini, KeyStyle};
    ///
    /// let mut ini = Ini::new().with_key_style(KeyStyle::Compact);
    /// ini.set("General", "launchOnSystemStartup", "true");
    /// assert_eq!(ini.to_string(), "[General]\nlaunchOnSystemStartup=true\n");
    /// ```
    pub fn with_key_style(mut self, style: KeyStyle) -> Self {
        self.key_style = style;
        self
    }

    /// Change how keys added from now on by [`set`][Self::set] are written.
    pub fn set_key_style(&mut self, style: KeyStyle) {
        self.key_style = style;
    }

    /// How keys added by [`set`][Self::set] are written.
    pub fn key_style(&self) -> KeyStyle {
        self.key_style
    }

    /// The `raw` template of a property created by `set`: just the separator
    /// (Display only reads the spacing around `=` and the line ending from
    /// it). `like` is the line whose style `Auto` copies, if any.
    fn new_property_raw(&self, like: Option<usize>) -> String {
        let spaced = match self.key_style {
            KeyStyle::Spaced => true,
            KeyStyle::Compact => false,
            KeyStyle::Auto => {
                // The given line, else the last key before the end of the file.
                let from = like
                    .and_then(|i| self.lines.get(i))
                    .filter(|l| matches!(l, Line::Property { .. }))
                    .or_else(|| {
                        self.lines
                            .iter()
                            .rev()
                            .find(|l| matches!(l, Line::Property { .. }))
                    });
                match from {
                    Some(Line::Property { raw, .. }) => match raw.find('=') {
                        Some(eq) => raw[..eq].ends_with(' '),
                        None => true,
                    },
                    _ => true,
                }
            }
        };
        let cr = if self.crlf { "\r" } else { "" };
        if spaced {
            format!(" = {cr}")
        } else {
            format!("={cr}")
        }
    }

    /// Parse an INI document from a string.
    pub fn parse(input: &str) -> Result<Self, String> {
        let mut lines = Vec::new();

        // Use split('\n') instead of lines() to preserve consecutive blank lines
        let raw_lines: Vec<&str> = input.split('\n').collect();
        // Remove trailing empty element (artifact of file ending with \n)
        let raw_lines = if raw_lines.last() == Some(&"") {
            &raw_lines[..raw_lines.len() - 1]
        } else {
            &raw_lines[..]
        };

        // A CRLF line keeps its `\r` in `raw`, so Display writes it back as it
        // was. Every decision below works on trimmed text, which drops it.
        for &raw_line in raw_lines {
            let trimmed = raw_line.trim();

            if trimmed.is_empty() {
                lines.push(Line::Blank(raw_line.to_string()));
            } else if trimmed.starts_with(';') || trimmed.starts_with('#') {
                lines.push(Line::Comment(raw_line.to_string()));
            } else if trimmed.starts_with('[') {
                if let Some(end) = trimmed.find(']') {
                    let name = trimmed[1..end].trim().to_string();
                    lines.push(Line::Section {
                        raw: raw_line.to_string(),
                        name,
                    });
                } else {
                    return Err(format!("Invalid section header: {}", raw_line));
                }
            } else if let Some(eq_pos) = raw_line.find('=') {
                let key = raw_line[..eq_pos].trim().to_string();
                let value = raw_line[eq_pos + 1..].trim().to_string();
                lines.push(Line::Property {
                    raw: raw_line.to_string(),
                    key,
                    value,
                    modified: false,
                });
            } else {
                // Unknown line — preserve as comment
                lines.push(Line::Comment(raw_line.to_string()));
            }
        }

        let crlf = input
            .find('\n')
            .is_some_and(|i| i > 0 && input.as_bytes()[i - 1] == b'\r');
        let final_newline = input.is_empty() || input.ends_with('\n');
        Ok(Self {
            lines,
            crlf,
            final_newline,
            key_style: KeyStyle::default(),
        })
    }

    /// Load an INI file from disk.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| format!("Failed to read {}: {}", path.as_ref().display(), e))?;
        Self::parse(&content)
    }

    /// Write the INI document to disk, preserving the original format.
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), String> {
        let content = self.to_string();
        // Atomic write: write to temp file, then rename
        let tmp = path.as_ref().with_extension("ini.tmp");
        std::fs::write(&tmp, &content)
            .map_err(|e| format!("Failed to write {}: {}", tmp.display(), e))?;
        std::fs::rename(&tmp, path.as_ref())
            .map_err(|e| format!("Failed to rename to {}: {}", path.as_ref().display(), e))?;
        Ok(())
    }

    /// Get a value by section and key. Returns `None` if not found or empty.
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        let mut in_section = false;

        for line in &self.lines {
            match line {
                Line::Section { name, .. } => {
                    in_section = name == section;
                }
                Line::Property {
                    key: k, value: v, ..
                } if in_section && k == key => {
                    if v.is_empty() {
                        return None;
                    }
                    return Some(v.as_str());
                }
                _ => {}
            }
        }
        None
    }

    /// Set a value by section and key.
    ///
    /// If the key exists in the section, its value is updated in place.
    /// If the key doesn't exist but the section does, a new line is added
    /// right after the section's last key (or after its header if it has
    /// none), so the blank lines and comments that close the section stay
    /// where they are.
    /// If the section doesn't exist, both section and key are appended at the end.
    ///
    /// A new key is written in the [`KeyStyle`] set on the document, which by
    /// default follows the file's own `key = value` / `key=value` spacing.
    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        let mut in_section = false;
        // Where a new key goes: after the section's last key, or after its
        // header. Not after its last line — that is usually the blank line
        // separating it from the next section, and a key written there would
        // get a blank line before it and none after.
        let mut insert_after: Option<usize> = None;
        // The section's last key, whose spacing `KeyStyle::Auto` copies.
        let mut last_key: Option<usize> = None;

        // Try to find and update existing key
        for (i, line) in self.lines.iter_mut().enumerate() {
            match line {
                Line::Section { name, .. } => {
                    in_section = name == section;
                    if in_section {
                        insert_after = Some(i);
                    }
                }
                Line::Property {
                    key: k,
                    value: v,
                    modified,
                    ..
                } if in_section => {
                    if k == key {
                        *v = value.to_string();
                        *modified = true;
                        return;
                    }
                    insert_after = Some(i);
                    last_key = Some(i);
                }
                _ => {}
            }
        }

        // Key not found — insert in existing section or create new section
        if let Some(idx) = insert_after {
            let raw = self.new_property_raw(last_key);
            self.lines.insert(
                idx + 1,
                Line::Property {
                    raw,
                    key: key.to_string(),
                    value: value.to_string(),
                    modified: true,
                },
            );
        } else {
            let raw = self.new_property_raw(None);
            self.append_section_header(section);
            self.lines.push(Line::Property {
                raw,
                key: key.to_string(),
                value: value.to_string(),
                modified: true,
            });
        }
    }

    /// Remove a key from a section. Returns true if the key was found and removed.
    pub fn remove(&mut self, section: &str, key: &str) -> bool {
        let mut in_section = false;

        for i in 0..self.lines.len() {
            match &self.lines[i] {
                Line::Section { name, .. } => {
                    in_section = name == section;
                }
                Line::Property { key: k, .. } if in_section && k == key => {
                    self.lines.remove(i);
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    /// Append a section header, preceded by one blank line — and only one.
    ///
    /// The separator is skipped when there is nothing to separate from (an
    /// empty document would otherwise start with a blank line) and when the
    /// document already ends with one (which is how a second blank crept in
    /// after a `remove_section`, since a removal correctly takes the blank
    /// that followed it).
    fn append_section_header(&mut self, section: &str) {
        let needs_separator =
            !self.lines.is_empty() && !matches!(self.lines.last(), Some(Line::Blank(_)));
        let cr = if self.crlf { "\r" } else { "" };
        if needs_separator {
            self.lines.push(Line::Blank(cr.to_string()));
        }
        self.lines.push(Line::Section {
            raw: format!("[{section}]{cr}"),
            name: section.to_string(),
        });
    }

    /// Add an empty section, if it is not already there. Returns true if one
    /// was added.
    ///
    /// [`set`][Self::set] already creates a section when it writes a key into
    /// one that does not exist, so this is for the case where the header is
    /// wanted on its own — and for symmetry with
    /// [`remove_section`][Self::remove_section], which is where anyone will
    /// look for it.
    ///
    /// ```rust
    /// use ini_preserve::Ini;
    ///
    /// let mut ini = Ini::parse("[A]\na = 1\n").unwrap();
    /// assert!(ini.add_section("B"));
    /// assert!(!ini.add_section("B"), "already there");
    /// assert_eq!(ini.to_string(), "[A]\na = 1\n\n[B]\n");
    /// ```
    pub fn add_section(&mut self, section: &str) -> bool {
        if self.sections().contains(&section) {
            return false;
        }
        self.append_section_header(section);
        true
    }

    /// Remove a whole section: its header, its keys, and the blank lines and
    /// comments that sit inside it. Returns true if a section was removed.
    ///
    /// A section runs from its header to just before the next header, so
    /// everything written under it goes with it — including the blank line
    /// that separated it from what follows, which is what stops a removal
    /// from leaving a growing gap behind.
    ///
    /// **Comments above the header stay.** They are as likely to be a
    /// file-level banner as a description of the section, and silently
    /// deleting the top of someone's file is the worse mistake of the two.
    /// Remove them yourself if you want them gone.
    ///
    /// A name that appears more than once is removed everywhere, which is the
    /// only reading of "remove the section" that leaves nothing behind.
    ///
    /// ```rust
    /// use ini_preserve::Ini;
    ///
    /// let mut ini = Ini::parse("\
    /// ; keep me
    /// [Keep]
    /// a = 1
    ///
    /// ; about Drop
    /// [Drop]
    /// b = 2
    ///
    /// [Also]
    /// c = 3
    /// ").unwrap();
    ///
    /// assert!(ini.remove_section("Drop"));
    /// assert!(!ini.remove_section("Drop"), "already gone");
    ///
    /// let out = ini.to_string();
    /// assert!(!out.contains("[Drop]"));
    /// assert!(!out.contains("b = 2"));
    /// assert!(out.contains("; keep me"));
    /// assert!(out.contains("[Also]"));
    /// // The comment written above the header is left where it was.
    /// assert!(out.contains("; about Drop"));
    /// ```
    pub fn remove_section(&mut self, section: &str) -> bool {
        let mut kept: Vec<Line> = Vec::with_capacity(self.lines.len());
        let mut dropping = false;
        let mut removed = false;
        for line in std::mem::take(&mut self.lines) {
            if let Line::Section { name, .. } = &line {
                // A new header always ends the previous section, whether or
                // not it is the one being dropped.
                dropping = name == section;
                if dropping {
                    removed = true;
                    continue;
                }
            }
            if !dropping {
                kept.push(line);
            }
        }
        self.lines = kept;
        removed
    }

    /// Iterate over all sections and their key-value pairs.
    pub fn sections(&self) -> Vec<&str> {
        let mut result = Vec::new();
        for line in &self.lines {
            if let Line::Section { name, .. } = line {
                if !result.contains(&name.as_str()) {
                    result.push(name.as_str());
                }
            }
        }
        result
    }

    /// Iterate over all key-value pairs in a section.
    pub fn keys(&self, section: &str) -> Vec<(&str, &str)> {
        let mut result = Vec::new();
        let mut in_section = false;

        for line in &self.lines {
            match line {
                Line::Section { name, .. } => {
                    in_section = name == section;
                }
                Line::Property { key, value, .. } if in_section => {
                    result.push((key.as_str(), value.as_str()));
                }
                _ => {}
            }
        }
        result
    }
}

impl Default for Ini {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for Ini {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let last = self.lines.len().saturating_sub(1);
        for (i, line) in self.lines.iter().enumerate() {
            match line {
                Line::Blank(raw) | Line::Comment(raw) | Line::Section { raw, .. } => {
                    f.write_str(raw)?
                }
                Line::Property {
                    raw,
                    key,
                    value,
                    modified,
                } => {
                    if !*modified {
                        f.write_str(raw)?;
                    } else if let Some(eq_pos) = raw.find('=') {
                        // Detect original spacing around '=' and reproduce it
                        let before_eq = &raw[..eq_pos];
                        let after_eq = &raw[eq_pos + 1..];
                        let space_before = before_eq.ends_with(' ');
                        // If original value was empty, match the style of the key side
                        let space_after = after_eq.starts_with(' ')
                            || (after_eq.trim().is_empty() && space_before);
                        write!(f, "{}", key)?;
                        if space_before {
                            write!(f, " ")?
                        }
                        write!(f, "=")?;
                        if space_after {
                            write!(f, " ")?
                        }
                        write!(f, "{}", value)?;
                        // A line read from the source keeps its own ending.
                        if raw.ends_with('\r') {
                            f.write_str("\r")?;
                        }
                    } else {
                        // Not reached: `set` gives new keys a `raw` holding
                        // their separator. Kept as a safe default.
                        write!(f, "{} = {}", key, value)?;
                        if self.crlf {
                            f.write_str("\r")?;
                        }
                    }
                }
            }
            // The last line ends the way the source did, with or without one.
            if i < last || self.final_newline {
                f.write_str("\n")?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    /// The README promises that an unmodified file writes back identical. A
    /// file without a final newline gained one, and a Windows file came back
    /// with LF endings: `\r` was stripped on read and never written again.
    #[test]
    fn line_endings_and_final_newline_round_trip() {
        for input in [
            "[A]\nk=v",
            "[A]\r\nk = v\r\n\r\n[B]\r\n",
            "[A]\r\nk=v",
            "[A]\nk=v\r\n; mixed\n",
            "\u{feff}[A]\nk=v",
            "",
            "\n",
        ] {
            let ini = Ini::parse(input).unwrap();
            assert_eq!(ini.to_string(), input, "round-trip of {input:?}");
        }
    }

    /// Lines added to a Windows file use its CRLF ending, and a changed value
    /// keeps the ending of the line it replaces.
    #[test]
    fn edits_follow_the_source_line_ending() {
        let mut ini = Ini::parse("[A]\r\nk = 1\r\n").unwrap();
        ini.set("A", "k", "2");
        ini.set("A", "n", "3");
        ini.set("B", "m", "4");
        assert_eq!(
            ini.to_string(),
            "[A]\r\nk = 2\r\nn = 3\r\n\r\n[B]\r\nm = 4\r\n"
        );
    }

    /// Creating a section in an empty document must not open the file with a
    /// blank line, and creating one after an existing blank must not double
    /// it. Both were happening.
    #[test]
    fn a_created_section_gets_exactly_one_separating_blank() {
        let mut empty = Ini::new();
        empty.set("New", "k", "v");
        assert_eq!(empty.to_string(), "[New]\nk = v\n", "no leading blank");

        let mut trailing = Ini::parse("[A]\na = 1\n\n").unwrap();
        trailing.set("New", "k", "v");
        assert_eq!(
            trailing.to_string(),
            "[A]\na = 1\n\n[New]\nk = v\n",
            "the blank already there is the separator"
        );

        let mut tight = Ini::parse("[A]\na = 1\n").unwrap();
        tight.set("New", "k", "v");
        assert_eq!(tight.to_string(), "[A]\na = 1\n\n[New]\nk = v\n");
    }

    /// A key added to an existing section goes right after its last key: the
    /// blank line that closes the section stays in front of the next header
    /// instead of ending up in front of the new key.
    #[test]
    fn a_key_added_to_a_section_gets_no_blank_line() {
        let mut ini = Ini::parse("[A]\na = 1\n\n[B]\nb = 2\n").unwrap();
        ini.set("A", "c", "3");
        assert_eq!(ini.to_string(), "[A]\na = 1\nc = 3\n\n[B]\nb = 2\n");

        // Same for the last section, which has its blank at the end of file.
        let mut ini = Ini::parse("[A]\na = 1\n\n").unwrap();
        ini.set("A", "c", "3");
        assert_eq!(ini.to_string(), "[A]\na = 1\nc = 3\n\n");

        // A section with no key yet gets it right under its header.
        let mut ini = Ini::parse("[A]\n\n[B]\nb = 2\n").unwrap();
        ini.set("A", "c", "3");
        assert_eq!(ini.to_string(), "[A]\nc = 3\n\n[B]\nb = 2\n");
    }

    /// `KeyStyle::Auto` copies the file's own spacing; the explicit styles
    /// override it; and a document with nothing to copy keeps the historical
    /// `key = value`.
    #[test]
    fn new_keys_follow_the_requested_style() {
        let mut ini = Ini::parse("[A]\na=1\n").unwrap();
        ini.set("A", "b", "2");
        ini.set("New", "c", "3");
        assert_eq!(ini.to_string(), "[A]\na=1\nb=2\n\n[New]\nc=3\n");

        let mut ini = Ini::parse("[A]\na = 1\n").unwrap();
        ini.set("A", "b", "2");
        assert_eq!(ini.to_string(), "[A]\na = 1\nb = 2\n");

        // The target section's own style wins over the rest of the file.
        let mut ini = Ini::parse("[A]\na = 1\n[B]\nb=2\n[C]\nc = 3\n").unwrap();
        ini.set("B", "x", "y");
        assert!(ini.to_string().contains("b=2\nx=y\n"));

        let mut ini = Ini::new();
        ini.set("A", "k", "v");
        assert_eq!(ini.to_string(), "[A]\nk = v\n");

        let mut ini = Ini::parse("[A]\na = 1\n").unwrap();
        ini.set_key_style(KeyStyle::Compact);
        assert_eq!(ini.key_style(), KeyStyle::Compact);
        ini.set("A", "b", "2");
        ini.set("A", "a", "9");
        // An existing line keeps its own spacing whatever the style.
        assert_eq!(ini.to_string(), "[A]\na = 9\nb=2\n");

        let mut ini = Ini::parse("[A]\r\na=1\r\n")
            .unwrap()
            .with_key_style(KeyStyle::Spaced);
        ini.set("A", "b", "2");
        assert_eq!(ini.to_string(), "[A]\r\na=1\r\nb = 2\r\n");
    }

    /// The case that motivated `KeyStyle::Auto`: the Nextcloud desktop
    /// client's `nextcloud.cfg`, written by Qt's `QSettings`. Adding a folder
    /// to an account must produce exactly what `QSettings` would have
    /// written, and leave the `\\` escapes and `@Variant` values alone.
    #[test]
    fn qsettings_file_gets_qsettings_lines() {
        let source = r#"[General]
clientVersion=3.14.1
launchOnSystemStartup=true

[Accounts]
0\Folders\1\ignoreHiddenFiles=false
0\Folders\1\journalPath=.sync_0123456789ab.db
0\Folders\1\localPath=/home/user/Nextcloud/
0\Folders\1\paused=false
0\Folders\1\targetPath=/
0\Folders\1\version=2
0\authType=webflow
0\dav_user=user
0\url=https://cloud.example.org
0\webflow_user=user
version=2

[Proxy]
type=2
"#;
        let mut ini = Ini::parse(source).unwrap();
        assert_eq!(ini.to_string(), source, "untouched round-trip");
        assert_eq!(
            ini.get("Accounts", r"0\Folders\1\localPath"),
            Some("/home/user/Nextcloud/")
        );

        ini.set(
            "Accounts",
            r"0\Folders\2\localPath",
            r"C:\\Users\\me\\Docs/",
        );
        ini.set("Accounts", r"0\Folders\2\targetPath", "/Docs");
        ini.set("Accounts", r"0\Folders\2\paused", "false");
        ini.set("Accounts", "0\\url", "https://cloud.example.net");

        let expected = r#"[General]
clientVersion=3.14.1
launchOnSystemStartup=true

[Accounts]
0\Folders\1\ignoreHiddenFiles=false
0\Folders\1\journalPath=.sync_0123456789ab.db
0\Folders\1\localPath=/home/user/Nextcloud/
0\Folders\1\paused=false
0\Folders\1\targetPath=/
0\Folders\1\version=2
0\authType=webflow
0\dav_user=user
0\url=https://cloud.example.net
0\webflow_user=user
version=2
0\Folders\2\localPath=C:\\Users\\me\\Docs/
0\Folders\2\targetPath=/Docs
0\Folders\2\paused=false

[Proxy]
type=2
"#;
        assert_eq!(ini.to_string(), expected);
        assert_eq!(
            ini.get("Accounts", r"0\Folders\2\localPath"),
            Some(r"C:\\Users\\me\\Docs/")
        );
    }

    /// Removing a section and writing it back must land where it started,
    /// not one blank line further down each time.
    #[test]
    fn remove_then_set_is_a_round_trip() {
        let source = "[A]\na = 1\n\n[B]\nb = 2\n";
        let mut ini = Ini::parse(source).unwrap();
        assert!(ini.remove_section("B"));
        ini.set("B", "b", "2");
        assert_eq!(ini.to_string(), source);
    }

    /// `add_section` is the symmetric counterpart of `remove_section`, and
    /// says no when the section is already there.
    #[test]
    fn adding_a_section_that_exists_changes_nothing() {
        let source = "[A]\na = 1\n";
        let mut ini = Ini::parse(source).unwrap();
        assert!(!ini.add_section("A"));
        assert_eq!(ini.to_string(), source);

        assert!(ini.add_section("B"));
        assert_eq!(ini.sections(), vec!["A", "B"]);
        // An empty section is a header and nothing else.
        assert_eq!(ini.keys("B"), Vec::<(&str, &str)>::new());
    }

    /// The last section has no header after it, so it runs to the end of the
    /// file — the case an "up to the next header" implementation forgets.
    #[test]
    fn removing_the_last_section_takes_it_to_the_end_of_the_file() {
        let mut ini = Ini::parse("[A]\na = 1\n\n[B]\nb = 2\nc = 3\n").unwrap();
        assert!(ini.remove_section("B"));
        assert_eq!(ini.to_string(), "[A]\na = 1\n\n");
        assert_eq!(ini.sections(), vec!["A"]);
    }

    /// A name written twice is one section as far as `sections()` and `get()`
    /// are concerned, so removing it has to take both blocks.
    #[test]
    fn a_section_written_twice_is_removed_everywhere() {
        let mut ini =
            Ini::parse("[Dup]\na = 1\n[Other]\nb = 2\n[Dup]\nc = 3\n[Last]\nd = 4\n").unwrap();
        assert!(ini.remove_section("Dup"));
        assert_eq!(ini.sections(), vec!["Other", "Last"]);
        let out = ini.to_string();
        assert!(!out.contains("a = 1") && !out.contains("c = 3"));
        assert!(out.contains("b = 2") && out.contains("d = 4"));
    }

    /// Removing what is not there must not touch the document.
    #[test]
    fn removing_an_absent_section_changes_nothing() {
        let source = "; banner\n[A]\na = 1\n";
        let mut ini = Ini::parse(source).unwrap();
        assert!(!ini.remove_section("Nope"));
        assert_eq!(ini.to_string(), source);
    }

    /// The whole point of the crate: what survives a removal is byte-identical
    /// to what was read, spacing and comments included.
    #[test]
    fn what_survives_a_removal_is_untouched() {
        let source = "\
; file banner
[Keep]
   spaced   =   value
# a hash comment
other=1

[Gone]
x = 1
[Tail]
y = 2
";
        let mut ini = Ini::parse(source).unwrap();
        assert!(ini.remove_section("Gone"));
        let out = ini.to_string();
        assert!(out.contains("   spaced   =   value"), "spacing preserved");
        assert!(out.contains("# a hash comment"));
        assert!(out.contains("; file banner"));
        assert!(!out.contains("[Gone]") && !out.contains("x = 1"));
        assert!(out.contains("[Tail]") && out.contains("y = 2"));
    }
    use super::*;

    #[test]
    fn parse_and_roundtrip() {
        let input = "; Header comment
[Section1]
; This is a comment
Key1 = Value1
Key2 = Value2

[Section2]
Key3=Value3
";
        let ini = Ini::parse(input).unwrap();
        let output = ini.to_string();
        assert_eq!(input, output);
    }

    #[test]
    fn get_values() {
        let input = "[Player]
Width = 1920
Height = 1080
";
        let ini = Ini::parse(input).unwrap();
        assert_eq!(ini.get("Player", "Width"), Some("1920"));
        assert_eq!(ini.get("Player", "Height"), Some("1080"));
        assert_eq!(ini.get("Player", "Missing"), None);
        assert_eq!(ini.get("NoSection", "Width"), None);
    }

    #[test]
    fn get_empty_value_returns_none() {
        let input = "[Player]
Width =
Height = 1080
";
        let ini = Ini::parse(input).unwrap();
        assert_eq!(ini.get("Player", "Width"), None);
        assert_eq!(ini.get("Player", "Height"), Some("1080"));
    }

    #[test]
    fn set_existing_key_preserves_format() {
        let input = "; Config file
[Player]
; Resolution
Width = 1920
Height = 1080
";
        let mut ini = Ini::parse(input).unwrap();
        ini.set("Player", "Width", "3840");

        let output = ini.to_string();
        assert!(output.contains("; Config file"));
        assert!(output.contains("; Resolution"));
        assert!(output.contains("Width = 3840"));
        assert!(output.contains("Height = 1080"));
    }

    #[test]
    fn set_preserves_spacing_style() {
        let input = "[S1]
Key1=Value1
Key2 = Value2
Key3 =Value3
";
        let mut ini = Ini::parse(input).unwrap();
        ini.set("S1", "Key1", "New1");
        ini.set("S1", "Key2", "New2");
        ini.set("S1", "Key3", "New3");

        let output = ini.to_string();
        assert!(output.contains("Key1=New1"));
        assert!(output.contains("Key2 = New2"));
        assert!(output.contains("Key3 =New3"));
    }

    #[test]
    fn set_new_key_in_existing_section() {
        let input = "[Player]
Width = 1920
";
        let mut ini = Ini::parse(input).unwrap();
        ini.set("Player", "Height", "1080");

        let output = ini.to_string();
        assert!(output.contains("Width = 1920"));
        assert!(output.contains("Height = 1080"));
    }

    #[test]
    fn set_new_section() {
        let input = "[Player]
Width = 1920
";
        let mut ini = Ini::parse(input).unwrap();
        ini.set("Backglass", "Output", "1");

        let output = ini.to_string();
        assert!(output.contains("[Player]"));
        assert!(output.contains("[Backglass]"));
        assert!(output.contains("Output = 1"));
    }

    #[test]
    fn semicolon_in_value_preserved() {
        let input = "[Input]
Mapping.LeftFlipper = Key;225
Mapping.Start = Key;30|Joy1;5
";
        let ini = Ini::parse(input).unwrap();
        assert_eq!(ini.get("Input", "Mapping.LeftFlipper"), Some("Key;225"));
        assert_eq!(ini.get("Input", "Mapping.Start"), Some("Key;30|Joy1;5"));

        // Roundtrip
        let output = ini.to_string();
        assert!(output.contains("Mapping.LeftFlipper = Key;225"));
        assert!(output.contains("Mapping.Start = Key;30|Joy1;5"));
    }

    #[test]
    fn remove_key() {
        let input = "[Player]
Width = 1920
Height = 1080
";
        let mut ini = Ini::parse(input).unwrap();
        assert!(ini.remove("Player", "Width"));
        assert!(!ini.remove("Player", "Width")); // already removed
        assert_eq!(ini.get("Player", "Width"), None);
        assert_eq!(ini.get("Player", "Height"), Some("1080"));
    }

    #[test]
    fn sections_and_keys() {
        let input = "[A]
X = 1
[B]
Y = 2
Z = 3
";
        let ini = Ini::parse(input).unwrap();
        assert_eq!(ini.sections(), vec!["A", "B"]);
        assert_eq!(ini.keys("B"), vec![("Y", "2"), ("Z", "3")]);
    }

    #[test]
    fn comments_fully_preserved() {
        let input = "; ###############################
; # Visual Pinball X settings  #
; ###############################

[Version]
; VPX Version: version that saved this file [Default: '10814848']
VPinball = 10814848

[Player]
; Display: Display used for the main Playfield window [Default: '']
PlayfieldDisplay =
";
        let ini = Ini::parse(input).unwrap();
        let output = ini.to_string();
        assert_eq!(input, output);
    }

    #[test]
    fn load_and_save_file() {
        let dir = std::env::temp_dir().join("ini_preserve_test");
        std::fs::create_dir_all(&dir).unwrap();

        let path = dir.join("test.ini");
        let content = "[Test]\nKey = Value\n";
        std::fs::write(&path, content).unwrap();

        let mut ini = Ini::load(&path).unwrap();
        ini.set("Test", "Key", "NewValue");
        ini.save(&path).unwrap();

        let result = std::fs::read_to_string(&path).unwrap();
        assert!(result.contains("Key = NewValue"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn vpx_ini_realistic_roundtrip() {
        let input = "; #######################################################
; #  Visual Pinball X settings file
; #######################################################


[Version]
; VPX Version: VPX version that saved this file [Default: '10814848']
VPinball = 10814848



[Player]
; Backglass Volume: Main volume [Default: 100 in 0 .. 100]
MusicVolume =
; Playfield Volume: Main volume [Default: 100 in 0 .. 100]
SoundVolume =
; Display: Display used for the main Playfield window [Default: '']
PlayfieldDisplay =
; Sound3D mode [Default: '2 Front channels', 0='2 Front channels']
Sound3D = 5

[Input]
Devices =
Mapping.LeftFlipper = Key;225
Mapping.RightFlipper = Key;229
";
        let mut ini = Ini::parse(input).unwrap();

        // Verify semicolons in values work
        assert_eq!(ini.get("Input", "Mapping.LeftFlipper"), Some("Key;225"));

        // Modify some values
        ini.set("Player", "MusicVolume", "80");
        ini.set("Player", "PlayfieldDisplay", "Samsung 42\"");

        let output = ini.to_string();

        // Comments preserved
        assert!(output.contains("; #######################################################"));
        assert!(output.contains("; Backglass Volume:"));

        // Unmodified lines identical
        assert!(output.contains("Sound3D = 5"));
        assert!(output.contains("Mapping.LeftFlipper = Key;225"));

        // Modified values updated
        assert!(output.contains("MusicVolume = 80"));
        assert!(output.contains("PlayfieldDisplay = Samsung 42\""));
    }
}

#[test]
fn roundtrip_real_vpx_ini() {
    let path = "/home/pincab/.local/share/VPinballX/10.8/VPinballX.ini";
    if !std::path::Path::new(path).exists() {
        return;
    }
    let input = std::fs::read_to_string(path).unwrap();
    let ini = Ini::parse(&input).unwrap();
    let output = ini.to_string();
    let in_lines: Vec<&str> = input.lines().collect();
    let out_lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        in_lines.len(),
        out_lines.len(),
        "Line count mismatch: input={} output={}",
        in_lines.len(),
        out_lines.len()
    );
}

#[test]
fn roundtrip_vpx_with_set() {
    let path = "/home/pincab/.local/share/VPinballX/10.8/VPinballX.ini";
    if !std::path::Path::new(path).exists() {
        return;
    }
    let input = std::fs::read_to_string(path).unwrap();
    let mut ini = Ini::parse(&input).unwrap();

    // Set a value that already exists (empty)
    ini.set("Player", "PlayfieldDisplay", "Test 42\"");
    ini.set("Player", "BGSet", "1");

    let output = ini.to_string();
    let in_lines: Vec<&str> = input.lines().collect();
    let out_lines: Vec<&str> = output.lines().collect();

    eprintln!("Input:  {} lines", in_lines.len());
    eprintln!("Output: {} lines", out_lines.len());

    assert_eq!(
        in_lines.len(),
        out_lines.len(),
        "Line count mismatch after set: input={} output={}",
        in_lines.len(),
        out_lines.len()
    );
    assert!(output.contains("PlayfieldDisplay = Test 42\""));
    assert!(output.contains("BGSet = 1"));
}
