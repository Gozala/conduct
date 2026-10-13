//! The reader turns source text into s-expressions.
//!
//! The syntax is a small Clojure-like dialect: lists `( )`, vectors
//! `[ ]`, maps `{ }`, strings, integers, keywords (`:name`), labels
//! (`name:`, and `:` alone), and symbols. A `;` starts a comment that
//! runs to the end of the line.

use std::fmt;

/// A byte range in the source, for error messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Sexp {
    Int(i64, Span),
    Str(String, Span),
    Symbol(String, Span),
    Keyword(String, Span),
    /// `name:`: a parameter's label or a record's attribute. `:` alone is
    /// the label with no name, which a claim puts before its result.
    Label(String, Span),
    List(Vec<Sexp>, Span),
    Vector(Vec<Sexp>, Span),
    Map(Vec<Sexp>, Span),
}

impl Sexp {
    pub fn span(&self) -> Span {
        match self {
            Sexp::Int(_, s)
            | Sexp::Str(_, s)
            | Sexp::Symbol(_, s)
            | Sexp::Keyword(_, s)
            | Sexp::Label(_, s)
            | Sexp::List(_, s)
            | Sexp::Vector(_, s)
            | Sexp::Map(_, s) => *s,
        }
    }

    /// A symbol's name, without the mark a macro's expansion gave it.
    pub fn as_symbol(&self) -> Option<&str> {
        match self {
            Sexp::Symbol(name, _) => Some(bare(name)),
            _ => None,
        }
    }
}

/// What separates a symbol's name from the mark of the expansion that
/// wrote it. No symbol read from source has one, so a marked name is
/// never one a person wrote.
pub const MARK: char = '\u{1}';
/// What separates a marked name from the hash it referred to where the
/// macro that wrote it was defined.
pub const PIN: char = '\u{3}';

/// A symbol's name as written: without its mark and pin.
pub fn bare(name: &str) -> &str {
    name.split([MARK, PIN]).next().unwrap_or(name)
}

/// What follows a marked name's bare part: its mark and pin, or nothing.
pub fn mark_of(name: &str) -> &str {
    &name[bare(name).len()..]
}

/// The hash, in hex, a template symbol was pinned to where its macro
/// was defined.
pub fn pin_of(name: &str) -> Option<&str> {
    name.split_once(PIN).map(|(_, hex)| hex)
}

impl fmt::Display for Sexp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn seq(f: &mut fmt::Formatter<'_>, open: &str, items: &[Sexp], close: &str) -> fmt::Result {
            write!(f, "{open}")?;
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    write!(f, " ")?;
                }
                write!(f, "{item}")?;
            }
            write!(f, "{close}")
        }
        match self {
            Sexp::Int(n, _) => write!(f, "{n}"),
            Sexp::Str(s, _) => write!(f, "{s:?}"),
            // As a person wrote it: without an expansion's mark.
            Sexp::Symbol(s, _) => write!(f, "{}", bare(s)),
            Sexp::Keyword(s, _) => write!(f, ":{s}"),
            Sexp::Label(s, _) => write!(f, "{s}:"),
            Sexp::List(items, _) => seq(f, "(", items, ")"),
            Sexp::Vector(items, _) => seq(f, "[", items, "]"),
            Sexp::Map(items, _) => seq(f, "{", items, "}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReadError {
    pub message: String,
    pub span: Span,
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at byte {})", self.message, self.span.start)
    }
}

impl std::error::Error for ReadError {}

/// Read every top-level form in `source`.
pub fn read(source: &str) -> Result<Vec<Sexp>, ReadError> {
    let mut reader = Reader { src: source.as_bytes(), text: source, pos: 0 };
    let mut forms = Vec::new();
    loop {
        reader.skip_trivia();
        if reader.pos >= reader.src.len() {
            return Ok(forms);
        }
        forms.push(reader.form()?);
    }
}

struct Reader<'a> {
    src: &'a [u8],
    text: &'a str,
    pos: usize,
}

fn is_delimiter(b: u8) -> bool {
    b.is_ascii_whitespace() || matches!(b, b'(' | b')' | b'[' | b']' | b'{' | b'}' | b'"' | b';' | b',')
}

impl Reader<'_> {
    fn error<T>(&self, message: impl Into<String>, start: usize) -> Result<T, ReadError> {
        Err(ReadError { message: message.into(), span: Span { start, end: self.pos.max(start) } })
    }

    fn skip_trivia(&mut self) {
        while self.pos < self.src.len() {
            let b = self.src[self.pos];
            if b.is_ascii_whitespace() || b == b',' {
                self.pos += 1;
            } else if b == b';' {
                while self.pos < self.src.len() && self.src[self.pos] != b'\n' {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn form(&mut self) -> Result<Sexp, ReadError> {
        self.skip_trivia();
        let start = self.pos;
        let Some(&b) = self.src.get(self.pos) else {
            return self.error("unexpected end of input", start);
        };
        match b {
            b'(' => self.seq(b')').map(|(items, span)| Sexp::List(items, span)),
            b'[' => self.seq(b']').map(|(items, span)| Sexp::Vector(items, span)),
            b'{' => self.seq(b'}').map(|(items, span)| Sexp::Map(items, span)),
            b')' | b']' | b'}' => {
                self.pos += 1;
                self.error(format!("unexpected `{}`", b as char), start)
            }
            b'"' => self.string(),
            _ => self.atom(),
        }
    }

    fn seq(&mut self, close: u8) -> Result<(Vec<Sexp>, Span), ReadError> {
        let start = self.pos;
        self.pos += 1;
        let mut items = Vec::new();
        loop {
            self.skip_trivia();
            match self.src.get(self.pos) {
                None => return self.error(format!("missing `{}`", close as char), start),
                Some(&b) if b == close => {
                    self.pos += 1;
                    return Ok((items, Span { start, end: self.pos }));
                }
                Some(_) => items.push(self.form()?),
            }
        }
    }

    fn string(&mut self) -> Result<Sexp, ReadError> {
        let start = self.pos;
        self.pos += 1;
        let mut out = String::new();
        loop {
            let Some(c) = self.text[self.pos..].chars().next() else {
                return self.error("unterminated string", start);
            };
            self.pos += c.len_utf8();
            match c {
                '"' => return Ok(Sexp::Str(out, Span { start, end: self.pos })),
                '\\' => {
                    let Some(e) = self.text[self.pos..].chars().next() else {
                        return self.error("unterminated string", start);
                    };
                    self.pos += e.len_utf8();
                    out.push(match e {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        '"' => '"',
                        '\\' => '\\',
                        other => return self.error(format!("unknown escape `\\{other}`"), start),
                    });
                }
                c => out.push(c),
            }
        }
    }

    fn atom(&mut self) -> Result<Sexp, ReadError> {
        let start = self.pos;
        while self.pos < self.src.len() && !is_delimiter(self.src[self.pos]) {
            self.pos += 1;
        }
        let text = &self.text[start..self.pos];
        let span = Span { start, end: self.pos };
        // Marks and pins are the compiler's: a name a person writes has
        // neither.
        if text.chars().any(|c| c.is_control()) {
            return self.error("a name cannot contain control characters", start);
        }
        if text == ":" {
            return Ok(Sexp::Label(String::new(), span));
        }
        if let Some(name) = text.strip_suffix(':') {
            if name.starts_with(':') {
                return self.error(format!("`{text}` is neither a keyword nor a label"), start);
            }
            return Ok(Sexp::Label(name.to_string(), span));
        }
        if let Some(name) = text.strip_prefix(':') {
            if name.is_empty() {
                return self.error("empty keyword", start);
            }
            return Ok(Sexp::Keyword(name.to_string(), span));
        }
        let digits = text.strip_prefix('-').unwrap_or(text);
        if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            return match text.parse::<i64>() {
                Ok(n) if (-(1i64 << 62)..(1i64 << 62)).contains(&n) => Ok(Sexp::Int(n, span)),
                _ => self.error(format!("integer `{text}` does not fit in 63 bits"), start),
            };
        }
        Ok(Sexp::Symbol(text.to_string(), span))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nested_forms() {
        let forms = read("(defn f [x] {:a x \"k\" [1 -2]}) ; trailing\n:kw").unwrap();
        assert_eq!(forms.len(), 2);
        assert_eq!(forms[0].to_string(), "(defn f [x] {:a x \"k\" [1 -2]})");
        assert_eq!(forms[1].to_string(), ":kw");
    }

    #[test]
    fn reads_labels() {
        let forms = read("(sum 1 with: 2) {io.gozala.person/name: \"Ada\"} :").unwrap();
        assert!(matches!(&forms[0], Sexp::List(items, _) if items[2] == Sexp::Label("with".into(), items[2].span())));
        assert!(
            matches!(&forms[1], Sexp::Map(items, _) if matches!(&items[0], Sexp::Label(n, _) if n == "io.gozala.person/name"))
        );
        assert!(matches!(&forms[2], Sexp::Label(n, _) if n.is_empty()));
    }

    #[test]
    fn reports_unbalanced() {
        assert!(read("(f [x)").is_err());
        assert!(read("(f").is_err());
    }

    #[test]
    fn reads_escapes_and_unicode() {
        let forms = read(r#""a\n\"b\" ×""#).unwrap();
        assert_eq!(forms[0], Sexp::Str("a\n\"b\" ×".into(), forms[0].span()));
    }
}
