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
}

impl Ini {
    /// Create an empty INI document.
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    /// Parse an INI document from a string.
    pub fn parse(input: &str) -> Result<Self, String> {
        let mut lines = Vec::new();

        for raw_line in input.lines() {
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

        Ok(Self { lines })
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
    /// If the key doesn't exist but the section does, a new line is appended to the section.
    /// If the section doesn't exist, both section and key are appended at the end.
    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        let mut in_section = false;
        let mut section_last_idx: Option<usize> = None;

        // Try to find and update existing key
        for (i, line) in self.lines.iter_mut().enumerate() {
            match line {
                Line::Section { name, .. } => {
                    in_section = name == section;
                    if in_section {
                        section_last_idx = Some(i);
                    }
                }
                Line::Property {
                    key: k,
                    value: v,
                    modified,
                    ..
                } if in_section && k == key => {
                    *v = value.to_string();
                    *modified = true;
                    return;
                }
                _ => {
                    if in_section {
                        section_last_idx = Some(i);
                    }
                }
            }
        }

        // Key not found — insert in existing section or create new section
        if let Some(idx) = section_last_idx {
            // Section exists, insert after its last line
            let new_line = Line::Property {
                raw: String::new(),
                key: key.to_string(),
                value: value.to_string(),
                modified: true,
            };
            self.lines.insert(idx + 1, new_line);
        } else {
            // Section doesn't exist — append
            self.lines.push(Line::Blank(String::new()));
            self.lines.push(Line::Section {
                raw: format!("[{}]", section),
                name: section.to_string(),
            });
            self.lines.push(Line::Property {
                raw: String::new(),
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
        for line in &self.lines {
            match line {
                Line::Blank(raw) => writeln!(f, "{}", raw)?,
                Line::Comment(raw) => writeln!(f, "{}", raw)?,
                Line::Section { raw, .. } => writeln!(f, "{}", raw)?,
                Line::Property {
                    raw,
                    key,
                    value,
                    modified,
                } => {
                    if *modified {
                        // Detect original spacing around '=' and reproduce it
                        if let Some(eq_pos) = raw.find('=') {
                            let before_eq = &raw[..eq_pos];
                            let after_eq = &raw[eq_pos + 1..];
                            let space_before = before_eq.ends_with(' ');
                            // If original value was empty, match the style of the key side
                            let space_after = after_eq.starts_with(' ') || (after_eq.trim().is_empty() && space_before);
                            write!(f, "{}", key)?;
                            if space_before { write!(f, " ")? }
                            write!(f, "=")?;
                            if space_after { write!(f, " ")? }
                            writeln!(f, "{}", value)?;
                        } else {
                            // New property (no raw)
                            writeln!(f, "{} = {}", key, value)?;
                        }
                    } else {
                        writeln!(f, "{}", raw)?;
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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
        assert_eq!(
            ini.get("Input", "Mapping.LeftFlipper"),
            Some("Key;225")
        );
        assert_eq!(
            ini.get("Input", "Mapping.Start"),
            Some("Key;30|Joy1;5")
        );

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
