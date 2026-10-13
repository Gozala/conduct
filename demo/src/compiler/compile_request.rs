// Copyright 2026 The Dialog DB Project
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! One compile's request.

/// One compile: a source, the file it is shown as, and the world it
/// compiles to.
pub struct CompileRequest<'a> {
    /// The file the source is shown as, which diagnostics name.
    pub path: &'a str,
    /// The source.
    pub source: &'a str,
    /// The world: `element-component` or `route-component`.
    pub world: &'a str,
}
