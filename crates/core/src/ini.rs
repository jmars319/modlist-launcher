//! Reader for the QSettings INI dialect that Mod Organizer 2 writes.
//!
//! Handles quoted values, backslash escapes, `@ByteArray(...)` wrappers and
//! `%XX`-escaped keys. Read-only: there is deliberately no writer.

use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct Ini {
    sections: HashMap<String, HashMap<String, String>>,
}

impl Ini {
    pub fn parse(text: &str) -> Self {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut ini = Ini::default();
        let mut section = String::from("General");
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = unescape_key(&line[1..line.len() - 1]);
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            ini.sections
                .entry(section.to_lowercase())
                .or_default()
                .insert(unescape_key(key.trim()).to_lowercase(), unescape_value(value));
        }
        ini
    }

    /// Looks up a value; section and key are case-insensitive. Keys inside a
    /// group use `\` as separator, for example `1\title`.
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections.get(&section.to_lowercase())?.get(&key.to_lowercase()).map(String::as_str)
    }
}

fn unescape_key(key: &str) -> String {
    let bytes = key.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(b) = key.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn unescape_value(raw: &str) -> String {
    let mut out = String::new();
    let mut in_quotes = false;
    let mut chars = raw.trim_start().chars().peekable();
    // Trailing unquoted whitespace is dropped, quoted whitespace is kept.
    let mut pending_ws = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push_str(&pending_ws);
                pending_ws.clear();
                in_quotes = !in_quotes;
            }
            ';' if !in_quotes => break,
            '\\' => {
                out.push_str(&pending_ws);
                pending_ws.clear();
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => out.push('\r'),
                    Some('0') => out.push('\0'),
                    Some('x') => {
                        let mut hex = String::new();
                        while let Some(h) = chars.peek().copied().filter(char::is_ascii_hexdigit) {
                            hex.push(h);
                            chars.next();
                        }
                        if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                            out.push(ch);
                        }
                    }
                    Some(other) => out.push(other),
                    None => {}
                }
            }
            c if c.is_whitespace() && !in_quotes => pending_ws.push(c),
            c => {
                out.push_str(&pending_ws);
                pending_ws.clear();
                out.push(c);
            }
        }
    }
    unwrap_variant(out)
}

fn unwrap_variant(value: String) -> String {
    if let Some(inner) = value.strip_prefix("@ByteArray(").and_then(|v| v.strip_suffix(')')) {
        return inner.to_string();
    }
    if value == "@Invalid()" {
        return String::new();
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\u{feff}[General]\n\
gameName=Cyberpunk 2077\n\
selected_profile=@ByteArray(5. Project Ultrapunk - Custom)\n\
gamePath=@ByteArray(C:\\\\Program Files (x86)\\\\Steam\\\\steamapps\\\\common\\\\Cyberpunk 2077)\n\
\n\
[customExecutables]\n\
size=2\n\
1\\arguments=\" -skipStartScreen\"\n\
1\\binary=C:/Games/REDprelauncher.exe\n\
1\\title=REDprelauncher\n\
2\\arguments=\\\"C:\\\\Program Files (x86)\\\\Steam\\\\steamapps\\\\common\\\\Cyberpunk 2077\\\"\n\
2\\title=Explore Virtual Folder\n\
\n\
[Settings]\n\
download_directory=C:/Modding/Wabbajack Downloads/Witcher 3 Ascended\n\
my%20key=spaced\n";

    #[test]
    fn parses_mo2_values() {
        let ini = Ini::parse(SAMPLE);
        assert_eq!(ini.get("General", "gameName"), Some("Cyberpunk 2077"));
        assert_eq!(ini.get("general", "selected_profile"), Some("5. Project Ultrapunk - Custom"));
        assert_eq!(
            ini.get("General", "gamePath"),
            Some(r"C:\Program Files (x86)\Steam\steamapps\common\Cyberpunk 2077")
        );
        assert_eq!(ini.get("customExecutables", "size"), Some("2"));
        assert_eq!(ini.get("customExecutables", r"1\arguments"), Some(" -skipStartScreen"));
        assert_eq!(ini.get("customExecutables", r"1\binary"), Some("C:/Games/REDprelauncher.exe"));
        assert_eq!(
            ini.get("customExecutables", r"2\arguments"),
            Some(r#""C:\Program Files (x86)\Steam\steamapps\common\Cyberpunk 2077""#)
        );
        assert_eq!(
            ini.get("Settings", "download_directory"),
            Some("C:/Modding/Wabbajack Downloads/Witcher 3 Ascended")
        );
        assert_eq!(ini.get("Settings", "my key"), Some("spaced"));
        assert_eq!(ini.get("Settings", "missing"), None);
    }

    #[test]
    fn keys_outside_sections_land_in_general() {
        let ini = Ini::parse("LocalSaves=false\nLocalSettings=true\n");
        assert_eq!(ini.get("General", "LocalSaves"), Some("false"));
    }

    #[test]
    fn handles_comments_escapes_and_invalid() {
        let ini = Ini::parse("[a]\n; comment\nx=one ; trailing\ny=\"tab\\there\"\nz=@Invalid()\nw=\\x41b\n");
        assert_eq!(ini.get("a", "x"), Some("one"));
        assert_eq!(ini.get("a", "y"), Some("tab\there"));
        assert_eq!(ini.get("a", "z"), Some(""));
        assert_eq!(ini.get("a", "w"), Some("\u{41b}"));
    }
}
