// Copyright 2026 The Dialog DB Project
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! An element's or a route's compile.
//!
//! An author writes conduct and never writes WIT or glue: the compiler
//! knows the element world and the route world, and writes the exports
//! of each itself. So a compile is the author's source and its world.

use crate::compiler::CompileRequest;

/// The world every element compiles to.
pub const ELEMENT_WORLD: &str = "element-component";

/// The world every route compiles to.
pub const ROUTE_WORLD: &str = "route-component";

/// An element's or a route's compile: the author's source, the file it
/// is shown as, and the world.
pub struct Compile {
    /// The file the source is shown as.
    pub path: String,
    /// The author's source.
    pub source: String,
    /// The world.
    pub world: &'static str,
}

impl Compile {
    /// The compile of the element `tag` from `source`.
    ///
    /// # Errors
    ///
    /// None: the compiler finds what an element lacks. The result keeps
    /// the shape of a helper that could refuse a source before it.
    pub fn element(tag: &str, source: &str) -> Result<Self, String> {
        Ok(Compile { path: element_file(tag), source: source.to_string(), world: ELEMENT_WORLD })
    }

    /// The compile of the route `pattern` from `source`.
    pub fn route(pattern: &str, source: &str) -> Self {
        Compile { path: route_file(pattern), source: source.to_string(), world: ROUTE_WORLD }
    }

    /// The request to hand the compiler.
    pub fn request(&self) -> CompileRequest<'_> {
        CompileRequest { path: &self.path, source: &self.source, world: self.world }
    }
}

/// The file name a tag's source is shown as, such as `todo-item.cdt`.
pub fn element_file(tag: &str) -> String {
    format!("{tag}.cdt")
}

/// The file name a route's source is shown as: the pattern's segments
/// joined with `-`, such as `api-todos-id.cdt` for `/api/todos/:id`.
pub fn route_file(pattern: &str) -> String {
    let name: Vec<String> = pattern
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.trim_start_matches(':').to_string())
        .collect();
    format!("{}.cdt", name.join("-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[wcmp_macros::test]
    fn it_names_a_route_file_after_its_pattern() {
        assert_eq!(route_file("/api/todos/:id"), "api-todos-id.cdt");
        assert_eq!(route_file("/api/todos"), "api-todos.cdt");
    }
}
