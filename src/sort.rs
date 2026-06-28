//! The sort transform.
//!
//! [`sort_ast`] returns a new document whose order-insensitive sibling lists are
//! sorted into a canonical order. Two documents that differ only in the order of
//! definitions, selections, arguments, directives, or variable definitions
//! normalize to the same sorted document and print to the same string.
//!
//! # Sort keys
//!
//! Each list sorts by a fixed key, stably:
//!
//! - Document definitions: by kind then name. Fragments rank before operations.
//! - Selection set selections: by kind then name. Fields rank before fragment
//!   spreads, which rank before inline fragments.
//! - Field and directive arguments: by argument name.
//! - Directives on spreads, inline fragments, and fragment definitions: by name.
//! - Operation and fragment variable definitions: by variable name.
//!
//! # Ordering rules
//!
//! Names compare by UTF-16 code unit, case-sensitive, with no locale collation.
//! A missing name sorts after every present name. Equal keys keep their source
//! order because the sort is stable.

use crate::ast::{
    Definition, Directive, Document, Field, FragmentDefinition, FragmentSpread, InlineFragment,
    OperationDefinition, Selection, SelectionSet,
};
use std::cmp::Ordering;

/// Sort the order-insensitive sibling lists of a document into canonical order.
///
/// The input is consumed and a new sorted document is returned. The transform
/// reorders only lists whose order carries no meaning. It never changes field
/// names, argument values, or the order of values inside list and object
/// literals.
///
/// # Examples
///
/// ```
/// use graphql_sort_ast::{parse_document, print_document, sort_ast};
///
/// let doc = parse_document("query Foo { c b a }").unwrap();
/// let sorted = sort_ast(doc);
/// assert_eq!(print_document(&sorted), "query Foo {\n  a\n  b\n  c\n}\n");
/// ```
pub fn sort_ast(mut document: Document) -> Document {
    document
        .definitions
        .iter_mut()
        .for_each(sort_definition_children);
    stable_sort_by(&mut document.definitions, |a, b| {
        let by_kind = definition_kind_rank(a).cmp(&definition_kind_rank(b));
        by_kind.then_with(|| compare_optional_name(definition_name(a), definition_name(b)))
    });
    document
}

fn sort_definition_children(def: &mut Definition) {
    match def {
        Definition::Operation(op) => sort_operation(op),
        Definition::Fragment(frag) => sort_fragment(frag),
    }
}

fn sort_operation(op: &mut OperationDefinition) {
    sort_variable_definitions(&mut op.variable_definitions);
    // The operation directive list keeps source order. Each directive's
    // arguments still sort.
    op.directives.iter_mut().for_each(sort_directive_arguments);
    sort_selection_set(&mut op.selection_set);
}

fn sort_fragment(frag: &mut FragmentDefinition) {
    sort_variable_definitions(&mut frag.variable_definitions);
    sort_directives(&mut frag.directives);
    sort_selection_set(&mut frag.selection_set);
}

fn sort_selection_set(set: &mut SelectionSet) {
    set.selections.iter_mut().for_each(sort_selection_children);
    stable_sort_by(&mut set.selections, |a, b| {
        let by_kind = selection_kind_rank(a).cmp(&selection_kind_rank(b));
        by_kind.then_with(|| compare_optional_name(selection_name(a), selection_name(b)))
    });
}

fn sort_selection_children(selection: &mut Selection) {
    match selection {
        Selection::Field(field) => sort_field(field),
        Selection::FragmentSpread(spread) => sort_fragment_spread(spread),
        Selection::InlineFragment(inline) => sort_inline_fragment(inline),
    }
}

fn sort_field(field: &mut Field) {
    stable_sort_by(&mut field.arguments, |a, b| compare_str(&a.name, &b.name));
    // The field directive list keeps source order. Each directive's arguments
    // still sort.
    field
        .directives
        .iter_mut()
        .for_each(sort_directive_arguments);
    sort_selection_set(&mut field.selection_set);
}

fn sort_fragment_spread(spread: &mut FragmentSpread) {
    sort_directives(&mut spread.directives);
}

fn sort_inline_fragment(inline: &mut InlineFragment) {
    sort_directives(&mut inline.directives);
    sort_selection_set(&mut inline.selection_set);
}

/// Sort a directive list by name, and sort each directive's arguments.
fn sort_directives(directives: &mut [Directive]) {
    directives.iter_mut().for_each(sort_directive_arguments);
    stable_sort_by(directives, |a, b| compare_str(&a.name, &b.name));
}

/// Sort one directive's arguments by name. The directive itself stays in place.
fn sort_directive_arguments(directive: &mut Directive) {
    stable_sort_by(&mut directive.arguments, |a, b| {
        compare_str(&a.name, &b.name)
    });
}

fn sort_variable_definitions(defs: &mut [crate::ast::VariableDefinition]) {
    stable_sort_by(defs, |a, b| compare_str(&a.variable, &b.variable));
}

/// Kind rank for document definitions. Fragments rank before operations to match
/// the lexicographic order of the kind names `FragmentDefinition` and
/// `OperationDefinition`.
fn definition_kind_rank(def: &Definition) -> u8 {
    match def {
        Definition::Fragment(_) => 0,
        Definition::Operation(_) => 1,
    }
}

fn definition_name(def: &Definition) -> Option<&str> {
    match def {
        Definition::Operation(op) => op.name.as_deref(),
        Definition::Fragment(frag) => Some(&frag.name),
    }
}

/// Kind rank for selections. Fields rank before fragment spreads, which rank
/// before inline fragments, matching the lexicographic order of the kind names
/// `Field`, `FragmentSpread`, and `InlineFragment`.
fn selection_kind_rank(selection: &Selection) -> u8 {
    match selection {
        Selection::Field(_) => 0,
        Selection::FragmentSpread(_) => 1,
        Selection::InlineFragment(_) => 2,
    }
}

fn selection_name(selection: &Selection) -> Option<&str> {
    match selection {
        Selection::Field(field) => Some(&field.name),
        Selection::FragmentSpread(spread) => Some(&spread.fragment_name),
        // Inline fragments carry no name. The key is absent and sorts last.
        Selection::InlineFragment(_) => None,
    }
}

/// Compare two name keys where a missing name sorts after any present name.
fn compare_optional_name(a: Option<&str>, b: Option<&str>) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => compare_str(a, b),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Compare two strings by UTF-16 code unit.
///
/// JavaScript string comparison ranks by UTF-16 code unit. Rust `str` ordering
/// ranks by UTF-8 byte, which equals Unicode scalar value order. The two agree
/// for every character in the Basic Multilingual Plane and disagree only for
/// characters at U+10000 and above. GraphQL names are ASCII, so this distinction
/// never affects real keys, but comparing by code unit keeps parity exact for
/// any string value.
fn compare_str(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// Stable sort wrapper. The standard library `sort_by` is stable, so equal keys
/// keep their source order.
fn stable_sort_by<T, F>(items: &mut [T], compare: F)
where
    F: FnMut(&T, &T) -> Ordering,
{
    items.sort_by(compare);
}
