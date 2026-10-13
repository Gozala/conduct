// Copyright 2026 The Dialog DB Project
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The highlighting of conduct source in the shelf's editor.
//!
//! It runs on every keystroke, on the page, so it is a scan and not a
//! read: it knows no more of the program than its characters. It
//! follows the reader's rules for where a token ends.

mod kind;
mod token;

pub use kind::Kind;
pub use token::Token;

/// The forms that define.
const DEFINITION: &[&str] = &["define", "claim", "type", "macro"];

/// The forms that decide or bind.
const CONTROL: &[&str] = &["if", "match", "do", "let", "fn", "cond", "and", "or", "with", "without", "rec"];

/// Whether `byte` ends a symbol, as it ends one for the reader.
fn ends(byte: u8) -> bool {
    byte.is_ascii_whitespace() || b"()[]{}\";,".contains(&byte)
}

/// The tokens of `source`, in order.
pub fn tokenize(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut at = 0;
    let mut push = |start: usize, end: usize, kind: Kind| tokens.push(Token { start, end, kind });
    while at < bytes.len() {
        let byte = bytes[at];
        if byte.is_ascii_whitespace() || byte == b',' {
            at += 1;
        } else if byte == b';' {
            let end = source[at..].find('\n').map_or(bytes.len(), |n| at + n);
            push(at, end, Kind::Comment);
            at = end;
        } else if byte == b'"' {
            let start = at;
            at += 1;
            while at < bytes.len() && bytes[at] != b'"' {
                at += if bytes[at] == b'\\' { 2 } else { 1 };
            }
            at = (at + 1).min(bytes.len());
            push(start, at, Kind::String);
        } else if b"()[]{}".contains(&byte) {
            push(at, at + 1, Kind::Punctuation);
            at += 1;
        } else {
            let start = at;
            while at < bytes.len() && !ends(bytes[at]) {
                at += 1;
            }
            let word = &source[start..at];
            push(start, at, word_kind(word));
        }
    }
    tokens
}

/// The kind of the word `word`.
fn word_kind(word: &str) -> Kind {
    let digits = word.strip_prefix('-').unwrap_or(word);
    if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
        Kind::Number
    } else if word == "true" || word == "false" {
        Kind::Bool
    } else if word.starts_with(':') && word.len() > 1 {
        // A keyword: a variant's name.
        Kind::Type
    } else if word.ends_with(':') {
        // A label: a parameter's or an attribute's.
        Kind::Property
    } else if word.starts_with('?') {
        // A type variable.
        Kind::Meta
    } else if word.ends_with('!') {
        // A task: what performs it has an effect.
        Kind::Modifier
    } else if DEFINITION.contains(&word) {
        Kind::Definition
    } else if CONTROL.contains(&word) {
        Kind::Control
    } else if word == "..." || word == "_" {
        Kind::OperatorKeyword
    } else {
        Kind::Variable
    }
}

/// `source` as HTML: each token in a `span` whose class names its kind,
/// and every other character as it is, escaped.
pub fn html(source: &str) -> String {
    let mut out = String::with_capacity(source.len() * 2);
    let mut at = 0;
    for token in tokenize(source) {
        escape_into(&mut out, &source[at..token.start]);
        out.push_str("<span class=\"");
        out.push_str(token.kind.class());
        out.push_str("\">");
        escape_into(&mut out, &source[token.start..token.end]);
        out.push_str("</span>");
        at = token.end;
    }
    escape_into(&mut out, &source[at..]);
    out
}

/// Append `text` to `out`, with the characters HTML gives a meaning
/// escaped.
fn escape_into(out: &mut String, text: &str) {
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<(&str, Kind)> {
        tokenize(source).into_iter().map(|t| (&source[t.start..t.end], t.kind)).collect()
    }

    #[wcmp_macros::test]
    fn it_tells_the_forms_apart() {
        assert_eq!(
            kinds("(define (save! x model: m) :none) ; done"),
            vec![
                ("(", Kind::Punctuation),
                ("define", Kind::Definition),
                ("(", Kind::Punctuation),
                ("save!", Kind::Modifier),
                ("x", Kind::Variable),
                ("model:", Kind::Property),
                ("m", Kind::Variable),
                (")", Kind::Punctuation),
                (":none", Kind::Type),
                (")", Kind::Punctuation),
                ("; done", Kind::Comment),
            ]
        );
        assert_eq!(
            kinds("\"a\\\"b\" -12 ?a"),
            vec![("\"a\\\"b\"", Kind::String), ("-12", Kind::Number), ("?a", Kind::Meta)]
        );
    }

    #[wcmp_macros::test]
    fn it_escapes_html() {
        assert_eq!(html("\"<b>\""), "<span class=\"tok-string\">\"&lt;b&gt;\"</span>");
    }
}
