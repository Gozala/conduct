//! Code without claims checks in reasonable time and space: a library of
//! combinators, each polymorphic and each used many times, with no
//! signature to keep its types small.

use conduct::{Codebase, library};
use std::time::{Duration, Instant};

/// `source` without its `(claim ..)` forms.
fn unclaimed(source: &str) -> String {
    let mut out = String::new();
    let mut rest = source;
    while let Some(at) = rest.find("(claim ") {
        out.push_str(&rest[..at]);
        let mut depth = 0;
        let mut in_text = false;
        let mut escaped = false;
        let mut end = at;
        for (i, c) in rest[at..].char_indices() {
            match c {
                _ if escaped => escaped = false,
                '\\' if in_text => escaped = true,
                '"' => in_text = !in_text,
                '(' if !in_text => depth += 1,
                ')' if !in_text => {
                    depth -= 1;
                    if depth == 0 {
                        end = at + i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

#[test]
fn a_parser_library_and_json_without_claims() {
    // Debug builds use large stack frames; the depth of checking is a few
    // hundred, which a release build fits in far less.
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(|| {
            let start = Instant::now();
            let mut cb = Codebase::new();
            cb.add_source(&unclaimed(library::PARSER)).unwrap_or_else(|e| panic!("{e}"));
            cb.add_source(&unclaimed(library::JSON)).unwrap_or_else(|e| panic!("{e}"));
            assert!(start.elapsed() < Duration::from_secs(30), "checking took {:?}", start.elapsed());
            // Every type can be shown, though the largest only in part.
            for name in ["from-text", "to-text", "value-literal", "member-literal", "string-literal"] {
                let shown = cb.type_of(&format!("conduct.json/{name}")).unwrap();
                assert!(shown.len() < 200_000, "{name}: {} characters", shown.len());
            }
            assert_eq!(cb.type_of("conduct.json/plain?").unwrap(), "((\\ integer) : boolean)");
        })
        .unwrap()
        .join()
        .unwrap();
}
