//! Property tests for the sort transform.
//!
//! These generate random executable documents and check invariants that must
//! hold for every input: idempotence, sortedness, stability, and semantic
//! preservation.

use graphql_sort_ast::ast::{
    Argument, Definition, Directive, Document, Field, FragmentDefinition, FragmentSpread,
    InlineFragment, OperationDefinition, OperationType, Selection, SelectionSet, Value,
};
use graphql_sort_ast::{parse_document, print_document, sort_ast};
use proptest::prelude::*;
use std::collections::BTreeMap;

/// Lowercase ASCII names so generated documents always parse and print.
fn name_strategy() -> impl Strategy<Value = String> {
    proptest::string::string_regex("[a-e]{1,3}").expect("valid regex")
}

fn value_strategy() -> impl Strategy<Value = Value> {
    prop_oneof![
        (1u32..100).prop_map(|n| Value::Int(n.to_string())),
        any::<bool>().prop_map(Value::Boolean),
        name_strategy().prop_map(Value::Enum),
    ]
}

fn argument_strategy() -> impl Strategy<Value = Argument> {
    (name_strategy(), value_strategy()).prop_map(|(name, value)| Argument { name, value })
}

fn directive_strategy() -> impl Strategy<Value = Directive> {
    (
        name_strategy(),
        prop::collection::vec(argument_strategy(), 0..3),
    )
        .prop_map(|(name, arguments)| Directive { name, arguments })
}

fn selection_strategy() -> impl Strategy<Value = Selection> {
    let leaf = (
        name_strategy(),
        prop::collection::vec(argument_strategy(), 0..3),
        prop::collection::vec(directive_strategy(), 0..2),
    )
        .prop_map(|(name, arguments, directives)| {
            Selection::Field(Field {
                alias: None,
                name,
                arguments,
                directives,
                selection_set: SelectionSet {
                    selections: Vec::new(),
                },
            })
        });

    leaf.prop_recursive(3, 16, 4, |inner| {
        prop_oneof![
            // Field with a nested selection set.
            (
                name_strategy(),
                prop::collection::vec(argument_strategy(), 0..3),
                prop::collection::vec(directive_strategy(), 0..2),
                prop::collection::vec(inner.clone(), 1..4),
            )
                .prop_map(|(name, arguments, directives, selections)| {
                    Selection::Field(Field {
                        alias: None,
                        name,
                        arguments,
                        directives,
                        selection_set: SelectionSet { selections },
                    })
                }),
            // Fragment spread.
            (
                name_strategy(),
                prop::collection::vec(directive_strategy(), 0..2),
            )
                .prop_map(|(fragment_name, directives)| {
                    Selection::FragmentSpread(FragmentSpread {
                        fragment_name,
                        directives,
                    })
                }),
            // Inline fragment with a nested selection set.
            (
                prop::option::of(name_strategy()),
                prop::collection::vec(directive_strategy(), 0..2),
                prop::collection::vec(inner, 1..4),
            )
                .prop_map(|(type_condition, directives, selections)| {
                    Selection::InlineFragment(InlineFragment {
                        type_condition,
                        directives,
                        selection_set: SelectionSet { selections },
                    })
                }),
        ]
    })
}

fn selection_set_strategy() -> impl Strategy<Value = SelectionSet> {
    prop::collection::vec(selection_strategy(), 1..4)
        .prop_map(|selections| SelectionSet { selections })
}

fn operation_type_strategy() -> impl Strategy<Value = OperationType> {
    prop_oneof![
        Just(OperationType::Query),
        Just(OperationType::Mutation),
        Just(OperationType::Subscription),
    ]
}

fn definition_strategy() -> impl Strategy<Value = Definition> {
    prop_oneof![
        (
            operation_type_strategy(),
            prop::option::of(name_strategy()),
            selection_set_strategy(),
        )
            .prop_map(|(operation, name, selection_set)| {
                Definition::Operation(OperationDefinition {
                    operation,
                    name,
                    variable_definitions: Vec::new(),
                    directives: Vec::new(),
                    selection_set,
                })
            }),
        (
            name_strategy(),
            name_strategy(),
            prop::collection::vec(directive_strategy(), 0..2),
            selection_set_strategy(),
        )
            .prop_map(|(name, type_condition, directives, selection_set)| {
                Definition::Fragment(FragmentDefinition {
                    name,
                    variable_definitions: Vec::new(),
                    type_condition,
                    directives,
                    selection_set,
                })
            }),
    ]
}

prop_compose! {
    fn document_strategy()(definitions in prop::collection::vec(definition_strategy(), 1..4))
        -> Document {
        Document { definitions }
    }
}

/// Compare strings the same way the transform does: by UTF-16 code unit.
fn name_key(a: &str) -> Vec<u16> {
    a.encode_utf16().collect()
}

/// Check that every sorted list in a document is non-decreasing under the
/// transform's keys.
fn assert_sorted(doc: &Document) {
    assert_non_decreasing(&doc.definitions, |d| {
        (definition_rank(d), definition_name_key(d))
    });
    for def in &doc.definitions {
        match def {
            Definition::Operation(op) => assert_selection_set_sorted(&op.selection_set),
            Definition::Fragment(frag) => {
                assert_directives_sorted(&frag.directives);
                assert_selection_set_sorted(&frag.selection_set);
            }
        }
    }
}

fn assert_selection_set_sorted(set: &SelectionSet) {
    assert_non_decreasing(&set.selections, |s| {
        (selection_rank(s), selection_name_key(s))
    });
    for sel in &set.selections {
        match sel {
            Selection::Field(field) => {
                assert_arguments_sorted(&field.arguments);
                assert_directive_args_sorted(&field.directives);
                assert_selection_set_sorted(&field.selection_set);
            }
            Selection::FragmentSpread(spread) => assert_directives_sorted(&spread.directives),
            Selection::InlineFragment(inline) => {
                assert_directives_sorted(&inline.directives);
                assert_selection_set_sorted(&inline.selection_set);
            }
        }
    }
}

fn assert_directives_sorted(directives: &[Directive]) {
    assert_non_decreasing(directives, |d| name_key(&d.name));
    assert_directive_args_sorted(directives);
}

fn assert_directive_args_sorted(directives: &[Directive]) {
    for directive in directives {
        assert_arguments_sorted(&directive.arguments);
    }
}

fn assert_arguments_sorted(arguments: &[Argument]) {
    assert_non_decreasing(arguments, |a| name_key(&a.name));
}

fn assert_non_decreasing<T, K, F>(items: &[T], key: F)
where
    K: Ord,
    F: Fn(&T) -> K,
{
    for pair in items.windows(2) {
        assert!(key(&pair[0]) <= key(&pair[1]), "list is not sorted");
    }
}

fn definition_rank(def: &Definition) -> u8 {
    match def {
        Definition::Fragment(_) => 0,
        Definition::Operation(_) => 1,
    }
}

/// Name key ordered so a missing name sorts after every present name. The
/// leading flag is `true` when the name is missing, so present names rank first.
fn definition_name_key(def: &Definition) -> (bool, Vec<u16>) {
    match def {
        Definition::Operation(op) => optional_name_key(op.name.as_deref()),
        Definition::Fragment(frag) => optional_name_key(Some(&frag.name)),
    }
}

fn optional_name_key(name: Option<&str>) -> (bool, Vec<u16>) {
    match name {
        Some(n) => (false, name_key(n)),
        None => (true, Vec::new()),
    }
}

fn selection_rank(sel: &Selection) -> u8 {
    match sel {
        Selection::Field(_) => 0,
        Selection::FragmentSpread(_) => 1,
        Selection::InlineFragment(_) => 2,
    }
}

fn selection_name_key(sel: &Selection) -> (bool, Vec<u16>) {
    match sel {
        Selection::Field(field) => optional_name_key(Some(&field.name)),
        Selection::FragmentSpread(spread) => optional_name_key(Some(&spread.fragment_name)),
        Selection::InlineFragment(_) => optional_name_key(None),
    }
}

/// A multiset summary of a document that ignores sibling order. Two documents
/// with the same summary differ only in order.
fn summarize(doc: &Document) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for def in &doc.definitions {
        match def {
            Definition::Operation(op) => {
                let label = format!("op:{}:{:?}", op.operation.keyword(), op.name);
                *counts.entry(label).or_insert(0) += 1;
                summarize_selection_set(&op.selection_set, &mut counts);
            }
            Definition::Fragment(frag) => {
                *counts
                    .entry(format!("frag:{}:{}", frag.name, frag.type_condition))
                    .or_insert(0) += 1;
                summarize_directives(&frag.directives, &mut counts);
                summarize_selection_set(&frag.selection_set, &mut counts);
            }
        }
    }
    counts
}

fn summarize_selection_set(set: &SelectionSet, counts: &mut BTreeMap<String, usize>) {
    for sel in &set.selections {
        match sel {
            Selection::Field(field) => {
                *counts.entry(format!("field:{}", field.name)).or_insert(0) += 1;
                for arg in &field.arguments {
                    *counts.entry(format!("arg:{}", arg.name)).or_insert(0) += 1;
                }
                summarize_directives(&field.directives, counts);
                summarize_selection_set(&field.selection_set, counts);
            }
            Selection::FragmentSpread(spread) => {
                *counts
                    .entry(format!("spread:{}", spread.fragment_name))
                    .or_insert(0) += 1;
                summarize_directives(&spread.directives, counts);
            }
            Selection::InlineFragment(inline) => {
                *counts
                    .entry(format!("inline:{:?}", inline.type_condition))
                    .or_insert(0) += 1;
                summarize_directives(&inline.directives, counts);
                summarize_selection_set(&inline.selection_set, counts);
            }
        }
    }
}

fn summarize_directives(directives: &[Directive], counts: &mut BTreeMap<String, usize>) {
    for directive in directives {
        *counts.entry(format!("dir:{}", directive.name)).or_insert(0) += 1;
        for arg in &directive.arguments {
            *counts.entry(format!("dirarg:{}", arg.name)).or_insert(0) += 1;
        }
    }
}

proptest! {
    // P1: sorting is idempotent.
    #[test]
    fn idempotence(doc in document_strategy()) {
        let once = sort_ast(doc);
        let printed_once = print_document(&once);
        let twice = sort_ast(once);
        prop_assert_eq!(printed_once, print_document(&twice));
    }

    // P3: every sorted list is non-decreasing under the transform's keys.
    #[test]
    fn sortedness(doc in document_strategy()) {
        let sorted = sort_ast(doc);
        assert_sorted(&sorted);
    }

    // P2: sorting changes only order. The order-insensitive summary is preserved.
    #[test]
    fn semantic_preservation(doc in document_strategy()) {
        let before = summarize(&doc);
        let sorted = sort_ast(doc);
        prop_assert_eq!(before, summarize(&sorted));
    }

    // Round-trip: a sorted document parses back to the same printed string.
    #[test]
    fn print_parse_roundtrip(doc in document_strategy()) {
        let sorted = sort_ast(doc);
        let printed = print_document(&sorted);
        let reparsed = parse_document(&printed).expect("sorted output parses");
        prop_assert_eq!(printed, print_document(&reparsed));
    }
}

// P4: stability. Equal-key siblings keep source order. Duplicate inline
// fragments have no name and equal kind, so they must stay put.
#[test]
fn stability_inline_fragments() {
    let doc = parse_document("{ ... on Z { z } ... on A { a } ... on M { m } }").unwrap();
    let sorted = print_document(&sort_ast(doc));
    let z = sorted.find("on Z").unwrap();
    let a = sorted.find("on A").unwrap();
    let m = sorted.find("on M").unwrap();
    assert!(z < a && a < m, "inline fragments must keep source order");
}
