//! The libraries written in conduct itself, as sources a codebase adds.
//! Each declares its own domain, so its definitions are named there
//! (`conduct.map/put`) and found by the bare name where that is unique.

/// `conduct.map`: a persistent hash map, a port of perma-map.
pub const MAP: &str = include_str!("../lib/map.cdt");

/// `conduct.parser`: parser combinators, a port of elm/parser.
pub const PARSER: &str = include_str!("../lib/parser.cdt");
