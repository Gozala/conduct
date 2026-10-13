// Copyright 2026 The Dialog DB Project
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! What the language service tells the shelf's editor about a source:
//! diagnostics, hover, completions, and the location of a definition.

mod completion;
mod diagnostic;
mod hover;
mod location;
mod severity;

pub use completion::Completion;
pub use diagnostic::Diagnostic;
pub use hover::Hover;
pub use location::Location;
pub use severity::Severity;
