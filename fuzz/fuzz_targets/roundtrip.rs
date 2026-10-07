//! The crate's promise, checked on arbitrary text: whatever `Ini::parse`
//! accepts, `to_string` writes back byte-identical. Then edits on top of it
//! must not panic.

#![no_main]

use ini_preserve::{Ini, KeyStyle};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(mut ini) = Ini::parse(text) else {
        return;
    };
    assert_eq!(ini.to_string(), text, "an unmodified file must round-trip byte-identical");

    ini.set_key_style(match data.len() % 3 {
        0 => KeyStyle::Auto,
        1 => KeyStyle::Spaced,
        _ => KeyStyle::Compact,
    });

    let sections: Vec<String> = ini.sections().into_iter().map(String::from).collect();
    for section in sections.iter().take(8) {
        let _ = ini.get(section, "key");
        ini.set(section, "key", "value");
        let _ = ini.remove(section, "key");
    }
    ini.add_section("Fuzz");
    ini.set("Fuzz", "k", "v");
    if let Some(first) = sections.first() {
        ini.remove_section(first);
    }
    let _ = Ini::parse(&ini.to_string());
});
