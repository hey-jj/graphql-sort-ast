//! Conformance cases for the sort transform.
//!
//! Each case parses a GraphQL source string, sorts it, prints the result, and
//! compares the printed string to a frozen expectation. The expectations match
//! the canonical GraphQL printer. Asserting on printed output is the right parity
//! oracle because that is how a caller uses the transform: print the sorted
//! document to derive a signature.

use graphql_sort_ast::{parse_document, print_document, sort_ast};

/// Parse, sort, and print a document.
fn sorted(src: &str) -> String {
    let doc = parse_document(src).expect("valid document");
    print_document(&sort_ast(doc))
}

/// Table of (name, input, expected printed output).
const CASES: &[(&str, &str, &str)] = &[
    // C1: the documented example. Fields sort alphabetically.
    ("c1_readme", "query Foo { c b a }", "query Foo {\n  a\n  b\n  c\n}\n"),
    // C2: operations of the same kind sort by name.
    (
        "c2_ops_by_name",
        "query B {x} query A {y}",
        "query A {\n  y\n}\n\nquery B {\n  x\n}\n",
    ),
    // C3: fragments sort before operations because the kind name ranks first.
    (
        "c3_frags_before_ops",
        "query Q {...F} fragment F on T {a}",
        "fragment F on T {\n  a\n}\n\nquery Q {\n  ...F\n}\n",
    ),
    // C4: variable definitions sort by variable name.
    (
        "c4_var_defs_by_name",
        "query ($z:Int,$a:Int){f}",
        "query ($a: Int, $z: Int) {\n  f\n}\n",
    ),
    // C5: selections sort by kind. Field before FragmentSpread before
    // InlineFragment. Fields among themselves sort by name.
    (
        "c5_selection_kind_order",
        "query { z ...Spread ... on T { x } a }",
        "{\n  a\n  z\n  ...Spread\n  ... on T {\n    x\n  }\n}\n",
    ),
    // C6: two inline fragments have no name. They keep source order (stable).
    (
        "c6_inline_fragments_stable",
        "{ ... on B { b } ... on A { a } }",
        "{\n  ... on B {\n    b\n  }\n  ... on A {\n    a\n  }\n}\n",
    ),
    // C7: field arguments sort by name.
    (
        "c7_field_args",
        "{ f(z:1, a:2, m:3) }",
        "{\n  f(a: 2, m: 3, z: 1)\n}\n",
    ),
    // C8: directives on a fragment spread sort by name.
    (
        "c8_spread_directives",
        "query { ...F @z @a }",
        "{\n  ...F @a @z\n}\n",
    ),
    // C9: directives on an inline fragment sort by name.
    (
        "c9_inline_directives",
        "{ ... on T @z @a { x } }",
        "{\n  ... on T @a @z {\n    x\n  }\n}\n",
    ),
    // C10: directives on a fragment definition sort by name.
    (
        "c10_fragment_def_directives",
        "fragment F on T @z @a { x }",
        "fragment F on T @a @z {\n  x\n}\n",
    ),
    // C11: arguments of a directive sort by name.
    (
        "c11_directive_args",
        "{ f @dir(z:1, a:2) }",
        "{\n  f @dir(a: 2, z: 1)\n}\n",
    ),
    // C12: duplicate field names keep source order. No panic.
    ("c12_dup_fields_stable", "{ b a a }", "{\n  a\n  a\n  b\n}\n"),
    // C13: every nesting level sorts.
    (
        "c13_nested_recursion",
        "{ outer { c b a inner { z y x } } }",
        "{\n  outer {\n    a\n    b\n    c\n    inner {\n      x\n      y\n      z\n    }\n  }\n}\n",
    ),
    // C15: list and object literal order stays as authored. Only the argument
    // names `in` and `list` may reorder.
    (
        "c15_value_order_untouched",
        "{ f(in:{z:1,a:2}, list:[3,1,2]) }",
        "{\n  f(in: { z: 1, a: 2 }, list: [3, 1, 2])\n}\n",
    ),
    // C16: a single no-argument field prints unchanged.
    ("c16_degenerate", "{ f }", "{\n  f\n}\n"),
    // Mutations sort their selections.
    ("mutation", "mutation M { b a }", "mutation M {\n  a\n  b\n}\n"),
    // Subscriptions sort their selections.
    (
        "subscription",
        "subscription S { b a }",
        "subscription S {\n  a\n  b\n}\n",
    ),
    // Aliased fields sort by field name, not alias. Here both keys agree.
    ("alias", "{ zz: b yy: a }", "{\n  yy: a\n  zz: b\n}\n"),
    // Variable definitions with default values sort by variable name and keep
    // their defaults.
    (
        "var_defaults",
        "query ($b:Int = 3, $a:String = \"x\") { f }",
        "query ($a: String = \"x\", $b: Int = 3) {\n  f\n}\n",
    ),
    // Field directives keep source order. The directive's own arguments sort.
    (
        "field_directive_args",
        "{ f @z(b:1, a:2) @a }",
        "{\n  f @z(a: 2, b: 1) @a\n}\n",
    ),
    // Operations of different kinds share one definition kind, so they sort by
    // name, not by keyword. A mutation named A ranks before a query named Z.
    (
        "ops_sort_by_name_across_kinds",
        "mutation Z { a } query A { b }",
        "query A {\n  b\n}\n\nmutation Z {\n  a\n}\n",
    ),
    // All three operation keywords interleave by name. Order is A, B, C even
    // though the keywords are subscription, query, mutation.
    (
        "all_operation_kinds_by_name",
        "subscription C { a } query A { b } mutation B { c }",
        "query A {\n  b\n}\n\nmutation B {\n  c\n}\n\nsubscription C {\n  a\n}\n",
    ),
    // Several anonymous operations share an absent name, so they keep source
    // order. The sort is stable.
    (
        "anonymous_ops_stable",
        "{ b } { a } { c }",
        "{\n  b\n}\n\n{\n  a\n}\n\n{\n  c\n}\n",
    ),
    // Directives on an operation keep source order. Each directive's own
    // arguments still sort.
    (
        "operation_directives_keep_order",
        "query @z(b:2,a:1) @a { f }",
        "query @z(a: 1, b: 2) @a {\n  f\n}\n",
    ),
];

#[test]
fn conformance_table() {
    for (name, input, expected) in CASES {
        let got = sorted(input);
        assert_eq!(&got, expected, "case {name}: input {input:?}");
    }
}

#[test]
fn idempotence() {
    // C14: sorting an already-sorted document is a no-op.
    for (name, input, _) in CASES {
        let once = sorted(input);
        let twice = print_document(&sort_ast(parse_document(&once).expect("reparse")));
        assert_eq!(once, twice, "case {name} is not idempotent");
    }
}

#[test]
fn ascii_case_sensitive_order() {
    // SPEC ASCII ordering: digits, then uppercase, then underscore, then
    // lowercase. Field names cannot start with a digit, so use them only as
    // first characters that are valid name starts. Use single-character names
    // that exercise the ordering across the allowed ranges.
    let got = sorted("{ b B a A _x }");
    // Order: A, B, _x, a, b by UTF-16 code unit.
    assert_eq!(got, "{\n  A\n  B\n  _x\n  a\n  b\n}\n");
}

#[test]
fn anonymous_operation_name_sorts_last() {
    // An anonymous operation has no name. Among same-kind definitions a missing
    // name sorts after a present name.
    let got = sorted("{ a } query Named { b }");
    assert_eq!(got, "query Named {\n  b\n}\n\n{\n  a\n}\n");
}

#[test]
fn empty_object_and_list_literals() {
    let got = sorted("{ f(obj: {}, list: []) }");
    assert_eq!(got, "{\n  f(list: [], obj: {  })\n}\n");
}
