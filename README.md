# graphql-sort-ast

Normalize GraphQL queries into a canonical, sorted form for stable cache keys
and signatures.

`sort_ast` reorders the order-insensitive sibling lists of an executable GraphQL
document into a fixed order. Two queries that differ only in the order of
fields, arguments, directives, definitions, or variable definitions normalize to
the same document. They print to the same string and hash to the same signature.
A cache can then treat semantically identical queries as one, and a client that
emits query text in nondeterministic order still produces a stable key.

The transform never changes meaning. It reorders only siblings whose order
carries no meaning. List and object literal values keep their authored order.

## Installation

```toml
[dependencies]
graphql-sort-ast = "0.1"
```

## Usage

```rust
use graphql_sort_ast::{parse_document, print_document, sort_ast};

let doc = parse_document("query Foo { c b a }").unwrap();
let sorted = sort_ast(doc);
assert_eq!(print_document(&sorted), "query Foo {\n  a\n  b\n  c\n}\n");
```

## What gets sorted

| Node | List | Sort key |
|---|---|---|
| Document | definitions | kind, then name |
| Operation | variable definitions | variable name |
| Selection set | selections | kind, then name |
| Field | arguments | argument name |
| Fragment spread | directives | directive name |
| Inline fragment | directives | directive name |
| Fragment definition | variable definitions, then directives | variable name, directive name |
| Directive | arguments | argument name |

Definitions sort fragments before operations. Selections sort fields first,
then fragment spreads, then inline fragments. Names compare by UTF-16 code unit,
case-sensitive, with no locale collation. A missing name sorts after every
present name. Equal keys keep their source order because the sort is stable.

Field and operation directive lists keep their source order. Each directive's
own arguments still sort.

## What does not get sorted

- Elements inside a list literal.
- Fields inside an object literal.
- The order of values anywhere.

## Scope

The parser and AST cover executable documents: operations, fragments, selection
sets, fields, spreads, inline fragments, arguments, directives, variable
definitions, and values. Type-system (schema) definitions are rejected.

## License

Licensed under the [MIT license](LICENSE).
