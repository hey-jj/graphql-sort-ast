//! Normalize GraphQL queries for stable cache keys and signatures.
//!
//! [`sort_ast`] reorders the order-insensitive sibling lists of an executable
//! GraphQL document into a canonical order. Two queries that differ only in the
//! order of fields, arguments, definitions, or variable definitions normalize to
//! the same document. They print to the same string and hash to the
//! same signature. This helps a cache treat semantically identical queries as
//! one, and it defends against clients that emit query text in nondeterministic
//! order.
//!
//! The transform never changes meaning. It reorders only siblings whose order
//! carries no meaning. List and object literal values keep their authored order.
//!
//! # Example
//!
//! ```
//! use graphql_sort_ast::{parse_document, print_document, sort_ast};
//!
//! let doc = parse_document("query Foo { c b a }").unwrap();
//! let sorted = sort_ast(doc);
//! assert_eq!(print_document(&sorted), "query Foo {\n  a\n  b\n  c\n}\n");
//! ```
//!
//! # Scope
//!
//! The parser and AST cover executable documents: operations, fragments,
//! selection sets, fields, spreads, inline fragments, arguments, directives,
//! variable definitions, and values. Type-system (schema) definitions are out of
//! scope.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod ast;
mod parse;
mod print;
mod sort;

pub use ast::Document;
pub use parse::{parse_document, ParseError};
pub use print::print_document;
pub use sort::sort_ast;
