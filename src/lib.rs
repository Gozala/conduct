//! Conduct: a prototype language with Lisp syntax, hash-addressed
//! definitions, fully inferred lattice types, and Wasm components as
//! its compile target.
//!
//! The pipeline:
//!
//! 1. [`reader`] reads s-expressions.
//! 2. [`syntax`] parses them into named expressions.
//! 3. [`codebase`] resolves names to de Bruijn indices and hashes,
//!    groups recursion into cycles, hashes every definition, and checks
//!    its type once.
//! 4. [`infer`] and [`types`] infer types with subtyping (Simple-sub),
//!    with unions, intersections, variants, and optional attributes.
//! 5. [`codegen`] compiles the definitions an entry point reaches to a
//!    core Wasm module, and [`element`] and [`route`] wrap one as a
//!    component of the TodoMVC demo's element or route world.

pub mod claims;
pub mod codebase;
pub mod codegen;
pub mod element;
mod fetch;
pub mod infer;
pub mod library;
pub mod macros;
pub mod reader;
pub mod route;
pub mod service;
pub mod syntax;
pub mod term;
pub mod transform;
pub mod types;

pub use codebase::{Codebase, Definition};
pub use term::{Hash, Term};
