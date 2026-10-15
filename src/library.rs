//! The libraries written in conduct itself, as sources a codebase adds.
//! Each wraps its definitions in a domain, so they are named there
//! (`conduct.map/put`) and found by any trailing part of that name.

/// `conduct.map`: a persistent hash map, a port of perma-map.
pub const MAP: &str = include_str!("../lib/map.cdt");

/// `conduct.parser`: parser combinators, a port of elm/parser.
pub const PARSER: &str = include_str!("../lib/parser.cdt");

/// `conduct.route`: routes that parse and format paths, after subroute.
/// Needs [`PARSER`].
pub const ROUTE: &str = include_str!("../lib/route.cdt");

/// `conduct.json`: JSON values read and written in conduct. Needs
/// [`PARSER`].
pub const JSON: &str = include_str!("../lib/json.cdt");

/// `conduct.codec`: one description of a value that encodes it to JSON
/// and decodes it back, a port of elm-codec. Needs [`PARSER`] and
/// [`JSON`].
pub const CODEC: &str = include_str!("../lib/codec.cdt");
