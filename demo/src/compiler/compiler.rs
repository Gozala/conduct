// Copyright 2026 The Dialog DB Project
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The conduct compiler, instantiated on the polyfill.
//!
//! The component exports `compile` and `check`. The language service
//! the shelf's editor asks for is this host's, over the last check: what
//! a definition's type is, where it is written, and the names a source
//! defines.

use wcmp::{Component, Engine, Error, Func, Linker, Store, Val};

use crate::language::{Completion, Diagnostic, Hover, Location, Severity};
use crate::platform;

use super::{CompileError, CompileRequest, Compiled};

/// The interface the compiler exports.
const COMPILER: &str = "conduct:compiler/compiler";

/// The forms a completion offers beside a source's own names.
const FORMS: &[&str] = &["define", "claim", "type", "macro", "match", "if", "do", "let", "fn", "cond"];

/// The conduct compiler, instantiated once on the polyfill, and again
/// after a call that traps it.
pub struct Compiler {
    engine: Engine,
    component: Component,
    store: Store<()>,
    exports: Exports,
    /// The last check: its file, its source, and what it showed.
    checked: Option<Checked>,
}

/// The compiler's exports.
struct Exports {
    compile: Func,
    check: Func,
}

/// A source the compiler checked, and the definitions it showed.
struct Checked {
    path: String,
    source: String,
    shown: Vec<Shown>,
}

/// A definition of a checked source: its name, where its name is
/// written, and its type.
struct Shown {
    name: String,
    start: u32,
    end: u32,
    ty: String,
}

impl Compiler {
    /// Parse the compiler component `bytes` and instantiate it on
    /// `engine`.
    ///
    /// # Errors
    ///
    /// The polyfill's error when the component does not parse or
    /// instantiate, or does not export the compiler's interface.
    pub async fn new(engine: &Engine, bytes: &[u8]) -> Result<Self, Error> {
        let component = Component::new(engine, bytes).await?;
        let (store, exports) = instantiate(engine, &component).await?;
        Ok(Compiler { engine: engine.clone(), component, store, exports, checked: None })
    }

    /// Compile `request`.
    ///
    /// # Errors
    ///
    /// [`CompileError::Diagnostics`] when the compiler refused the
    /// program, and [`CompileError::Failed`] when it could not run.
    #[tracing::instrument(level = "debug", name = "conduct compile", skip_all, fields(path = request.path, world = request.world))]
    pub async fn compile(&mut self, request: &CompileRequest<'_>) -> Result<Compiled, CompileError> {
        let arguments = [Val::String(request.source.to_string()), Val::String(request.world.to_string())];
        let started = platform::now_millis();
        let results = self.call(|exports| &exports.compile, &arguments).await;
        let millis = platform::now_millis() - started;
        match first(&results?)? {
            Val::Result(Ok(Some(bytes))) => Ok(Compiled { bytes: bytes_of(bytes)?, millis }),
            Val::Result(Err(Some(problem))) => {
                let diagnostic = diagnostic_of(request.path, request.source, problem)?;
                Err(CompileError::Diagnostics {
                    text: format!(
                        "{}:{}:{}: {}",
                        diagnostic.file, diagnostic.line, diagnostic.column, diagnostic.message
                    ),
                    millis,
                })
            }
            other => Err(unexpected(other).into()),
        }
    }

    /// Check `request`: the diagnostic of the program `compile` would
    /// compile, if it does not compile. The queries below answer about
    /// the source of the last check.
    ///
    /// # Errors
    ///
    /// The polyfill's error when the compiler could not run.
    #[tracing::instrument(level = "debug", name = "conduct check", skip_all, fields(path = request.path))]
    pub async fn check(&mut self, request: &CompileRequest<'_>) -> Result<Vec<Diagnostic>, Error> {
        let arguments = [Val::String(request.source.to_string()), Val::String(request.world.to_string())];
        let results = self.call(|exports| &exports.check, &arguments).await?;
        let (shown, diagnostics) = match first(&results)? {
            Val::Result(Ok(Some(list))) => (shown_of(list)?, Vec::new()),
            Val::Result(Err(Some(problem))) => (
                // A source that does not check keeps what the last check
                // of its file showed, so a hover still answers while a
                // person types.
                self.checked
                    .take()
                    .filter(|checked| checked.path == request.path)
                    .map(|checked| checked.shown)
                    .unwrap_or_default(),
                vec![diagnostic_of(request.path, request.source, problem)?],
            ),
            other => return Err(unexpected(other)),
        };
        self.checked = Some(Checked { path: request.path.to_string(), source: request.source.to_string(), shown });
        Ok(diagnostics)
    }

    /// The type of the definition named at byte `offset` of the file
    /// `path`, in the last check.
    ///
    /// # Errors
    ///
    /// None: the answer is this host's.
    pub async fn hover(&mut self, path: &str, offset: u32) -> Result<Option<Hover>, Error> {
        Ok(self.named_at(path, offset).map(|shown| Hover {
            label: format!("{} : {}", shown.name, shown.ty),
            detail: shown.ty.clone(),
            doc: String::new(),
        }))
    }

    /// The names the last check's source defines, and the forms, that
    /// start with the word before byte `offset` of the file `path`.
    ///
    /// # Errors
    ///
    /// None: the answer is this host's.
    pub async fn complete(&mut self, path: &str, offset: u32) -> Result<Vec<Completion>, Error> {
        let Some(checked) = self.checked.as_ref().filter(|checked| checked.path == path) else {
            return Ok(Vec::new());
        };
        let (start, _) = word_at(&checked.source, offset as usize);
        let prefix = &checked.source[start..(offset as usize).min(checked.source.len())];
        let names = checked.shown.iter().map(|shown| Completion {
            label: shown.name.clone(),
            kind: if shown.ty.starts_with("((\\") { 3 } else { 6 },
            detail: shown.ty.clone(),
            doc: String::new(),
        });
        let forms = FORMS.iter().map(|form| Completion {
            label: (*form).to_string(),
            kind: 14,
            detail: String::new(),
            doc: String::new(),
        });
        Ok(names.chain(forms).filter(|completion| completion.label.starts_with(prefix)).collect())
    }

    /// Where the definition named at byte `offset` of the file `path`
    /// is written, in the last check.
    ///
    /// # Errors
    ///
    /// None: the answer is this host's.
    pub async fn definition(&mut self, path: &str, offset: u32) -> Result<Option<Location>, Error> {
        let Some(shown) = self.named_at(path, offset) else {
            return Ok(None);
        };
        let checked = self.checked.as_ref().expect("a hover answers from a check");
        let (line, column) = line_and_column(&checked.source, shown.start as usize);
        Ok(Some(Location { file: path.to_string(), start: shown.start, length: shown.end - shown.start, line, column }))
    }

    /// The definition of the last check that the word at byte `offset`
    /// of `path` names.
    fn named_at(&self, path: &str, offset: u32) -> Option<&Shown> {
        let checked = self.checked.as_ref().filter(|checked| checked.path == path)?;
        let (start, end) = word_at(&checked.source, offset as usize);
        let word = checked.source[start..end].trim_end_matches(['!', ':']);
        checked.shown.iter().find(|shown| shown.name == word)
    }

    /// Call the export `which` picks with `arguments`. A trap poisons the
    /// store, so the next call gets a new instance; if that fails too,
    /// the next call reports the poisoned store.
    async fn call(&mut self, which: impl Fn(&Exports) -> &Func, arguments: &[Val]) -> Result<Box<[Val]>, Error> {
        let results = which(&self.exports).call(&mut self.store, arguments).await;
        if results.is_err()
            && let Ok((store, exports)) = instantiate(&self.engine, &self.component).await
        {
            self.store = store;
            self.exports = exports;
        }
        results
    }
}

/// Whether `byte` ends a word, as it ends a symbol for conduct's reader.
fn ends(byte: u8) -> bool {
    byte.is_ascii_whitespace() || b"()[]{}\";,".contains(&byte)
}

/// The bytes of the word around byte `offset` of `source`.
fn word_at(source: &str, offset: usize) -> (usize, usize) {
    let bytes = source.as_bytes();
    let offset = offset.min(bytes.len());
    let mut start = offset;
    while start > 0 && !ends(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = offset;
    while end < bytes.len() && !ends(bytes[end]) {
        end += 1;
    }
    (start, end)
}

/// The line and the column of byte `offset` of `source`, each from one.
fn line_and_column(source: &str, offset: usize) -> (u32, u32) {
    let before = &source[..offset.min(source.len())];
    let line = before.matches('\n').count() + 1;
    let column = before.len() - before.rfind('\n').map_or(0, |n| n + 1) + 1;
    (line as u32, column as u32)
}

/// The first result of a call.
fn first(results: &[Val]) -> Result<&Val, Error> {
    results.first().ok_or_else(|| Error::Internal { message: "the compiler answered no result".to_string() })
}

/// Instantiate the compiler `component` in a new store, and answer its
/// exports.
async fn instantiate(engine: &Engine, component: &Component) -> Result<(Store<()>, Exports), Error> {
    let mut store = Store::new(engine, ())?;
    let instance = Linker::new(engine).instantiate(&mut store, component).await?;
    let func = |name: &str| {
        instance.exports().instance(COMPILER).and_then(|compiler| compiler.func(name)).ok_or_else(|| {
            Error::Unsupported { feature: format!("a compiler component with no `{COMPILER}#{name}` export") }
        })
    };
    let exports = Exports { compile: func("compile")?, check: func("check")? };
    Ok((store, exports))
}

/// The fields of a record, by name.
fn field<'a>(record: &'a Val, name: &str) -> Result<&'a Val, Error> {
    let Val::Record(fields) = record else {
        return Err(unexpected(record));
    };
    fields.iter().find(|field| field.name == name).map(|field| &field.value).ok_or_else(|| unexpected(record))
}

fn u32_of(value: &Val) -> Result<u32, Error> {
    match value {
        Val::U32(n) => Ok(*n),
        other => Err(unexpected(other)),
    }
}

fn string_of(value: &Val) -> Result<String, Error> {
    match value {
        Val::String(text) => Ok(text.clone()),
        other => Err(unexpected(other)),
    }
}

/// The diagnostic of a `problem` in the source `source` at `path`.
fn diagnostic_of(path: &str, source: &str, problem: &Val) -> Result<Diagnostic, Error> {
    let start = u32_of(field(problem, "start")?)?;
    let end = u32_of(field(problem, "end")?)?;
    let (line, column) = line_and_column(source, start as usize);
    Ok(Diagnostic {
        file: path.to_string(),
        start,
        length: end.saturating_sub(start),
        line,
        column,
        severity: Severity::Error,
        message: string_of(field(problem, "message")?)?,
    })
}

/// The definitions of a `list<shown>`.
fn shown_of(list: &Val) -> Result<Vec<Shown>, Error> {
    let Val::List(items) = list else {
        return Err(unexpected(list));
    };
    items
        .iter()
        .map(|item| {
            Ok(Shown {
                name: string_of(field(item, "name")?)?,
                start: u32_of(field(item, "start")?)?,
                end: u32_of(field(item, "end")?)?,
                ty: string_of(field(item, "type")?)?,
            })
        })
        .collect()
}

/// The bytes of a `list<u8>`.
fn bytes_of(list: &Val) -> Result<Vec<u8>, Error> {
    let Val::List(items) = list else {
        return Err(unexpected(list));
    };
    items
        .iter()
        .map(|item| match item {
            Val::U8(byte) => Ok(*byte),
            other => Err(unexpected(other)),
        })
        .collect()
}

/// The error for a value of the wrong shape in a result.
fn unexpected(val: &Val) -> Error {
    Error::Internal { message: format!("the compiler returned an unexpected value {val:?}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[wcmp_macros::test]
    fn it_finds_the_word_at_an_offset() {
        let source = "(define (save! x) (f x))";
        assert_eq!(word_at(source, 10), (9, 14));
        assert_eq!(word_at(source, 14), (9, 14));
        assert_eq!(line_and_column("a\nbc", 3), (2, 2));
    }
}
