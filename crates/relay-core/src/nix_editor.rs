//! Presentation data for the native Nix editor. No evaluation on the host.
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Highlight {
    pub location: usize,
    pub length: usize,
    pub kind: &'static str,
}

/// UTF-16 ranges match UIKit/AppKit text storage, including non-BMP characters.
/// Incomplete strings and comments stay highlighted while the user types.
pub fn highlights(source: &str) -> Vec<Highlight> {
    let chars: Vec<char> = source.chars().collect();
    let mut result = Vec::new();
    let mut i = 0;
    let mut location = 0;
    while i < chars.len() {
        let start = i;
        let kind;
        if chars[i] == '#' {
            i += 1;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            kind = "comment";
        } else if chars[i..].starts_with(&['/', '*']) {
            i += 2;
            let mut depth = 1;
            while i < chars.len() && depth > 0 {
                if chars[i..].starts_with(&['/', '*']) {
                    depth += 1;
                    i += 2;
                } else if chars[i..].starts_with(&['*', '/']) {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            kind = "comment";
        } else if chars[i] == '"' {
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\' {
                    i = (i + 2).min(chars.len());
                } else if chars[i] == '"' {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
            kind = "string";
        } else if chars[i..].starts_with(&['\'', '\'']) {
            i += 2;
            while i < chars.len() {
                if chars[i..].starts_with(&['\'', '\'', '\''])
                    || chars[i..].starts_with(&['\'', '\'', '$'])
                {
                    i += 3;
                } else if chars[i..].starts_with(&['\'', '\'']) {
                    i += 2;
                    break;
                } else {
                    i += 1;
                }
            }
            kind = "string";
        } else if chars[i].is_ascii_alphabetic() || chars[i] == '_' {
            i += 1;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || matches!(chars[i], '_' | '-' | '\''))
            {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            kind = if matches!(
                word.as_str(),
                "let"
                    | "in"
                    | "with"
                    | "rec"
                    | "inherit"
                    | "if"
                    | "then"
                    | "else"
                    | "assert"
                    | "or"
                    | "true"
                    | "false"
                    | "null"
            ) {
                "keyword"
            } else {
                "identifier"
            };
        } else if chars[i].is_ascii_digit() {
            i += 1;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            kind = "number";
        } else {
            i += 1;
            kind = if chars[start].is_whitespace() {
                "plain"
            } else {
                "punctuation"
            };
        }
        let length = chars[start..i].iter().map(|c| c.len_utf16()).sum();
        if kind != "plain" {
            result.push(Highlight {
                location,
                length,
                kind,
            });
        }
        location += length;
    }
    result
}

pub fn default_file(name: &str) -> Option<&'static str> {
    match name {
        "flake.nix" => Some(include_str!("../templates/nixos-guest/flake.nix")),
        "configuration.nix" => Some(include_str!("../templates/nixos-guest/configuration.nix")),
        "relay.nix" => Some(include_str!("../templates/nixos-guest/relay.nix")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf16_ranges_survive_unicode_and_incomplete_strings() {
        let s = "# 🦖\nlet x = \"unfinished";
        let spans = highlights(s);
        assert_eq!(
            spans[0],
            Highlight {
                location: 0,
                length: 4,
                kind: "comment"
            }
        );
        assert_eq!(
            spans[1],
            Highlight {
                location: 5,
                length: 3,
                kind: "keyword"
            }
        );
        let last = spans.last().unwrap();
        assert_eq!(last.kind, "string");
        assert_eq!(last.location + last.length, s.encode_utf16().count());
    }
    #[test]
    fn nested_comments_and_indented_string_escapes() {
        let s = "/* outer /* inner */ end */ ''escaped ''${ and ''${ literal'' true";
        let kinds: Vec<_> = highlights(s).iter().map(|s| s.kind).collect();
        assert_eq!(kinds, ["comment", "string", "keyword"]);
    }
    #[test]
    fn every_prefix_of_templates_has_ordered_valid_ranges() {
        for name in ["flake.nix", "configuration.nix", "relay.nix"] {
            let source = default_file(name).unwrap();
            for end in 0..=source.len() {
                if !source.is_char_boundary(end) {
                    continue;
                }
                let s = &source[..end];
                let utf16_length = s.encode_utf16().count();
                let mut previous = 0;
                for token in highlights(s) {
                    assert!(token.location >= previous);
                    assert!(token.length > 0);
                    previous = token.location + token.length;
                    assert!(previous <= utf16_length);
                }
            }
        }
    }
    #[test]
    fn default_files_keep_relay_import_and_flake_pin_relationship() {
        assert!(default_file("configuration.nix")
            .unwrap()
            .contains("./relay.nix"));
        assert!(default_file("flake.nix").unwrap().contains("relay/nixpkgs"));
        assert!(default_file("../secret").is_none());
    }
}
