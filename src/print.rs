//! Canonical printer for executable GraphQL documents.
//!
//! The output uses two-space indentation, one selection per line, and inline
//! comma-separated arguments. Two documents that differ only in the order of
//! order-insensitive lists print identically once sorted, which is the point of
//! the sort transform.

use crate::ast::{
    Argument, Definition, Directive, Document, Field, FragmentDefinition, InlineFragment,
    OperationDefinition, Selection, SelectionSet, Type, Value, VariableDefinition,
};

/// Render a document to its canonical string form.
#[must_use]
pub fn print_document(document: &Document) -> String {
    let mut out = String::new();
    for (i, def) in document.definitions.iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        match def {
            Definition::Operation(op) => print_operation(op, &mut out),
            Definition::Fragment(frag) => print_fragment(frag, &mut out),
        }
    }
    out.push('\n');
    out
}

fn print_operation(op: &OperationDefinition, out: &mut String) {
    if !op.is_shorthand() {
        out.push_str(op.operation.keyword());
        // The name and variable definitions form one group with no space
        // between them. The keyword precedes the group with a single space, even
        // when the operation has no name.
        let mut group = String::new();
        if let Some(name) = &op.name {
            group.push_str(name);
        }
        print_variable_definitions(&op.variable_definitions, &mut group);
        if !group.is_empty() {
            out.push(' ');
            out.push_str(&group);
        }
        print_directives(&op.directives, out);
        out.push(' ');
    }
    print_selection_set(&op.selection_set, 0, out);
}

fn print_fragment(frag: &FragmentDefinition, out: &mut String) {
    out.push_str("fragment ");
    out.push_str(&frag.name);
    print_variable_definitions(&frag.variable_definitions, out);
    out.push_str(" on ");
    out.push_str(&frag.type_condition);
    print_directives(&frag.directives, out);
    out.push(' ');
    print_selection_set(&frag.selection_set, 0, out);
}

fn print_variable_definitions(defs: &[VariableDefinition], out: &mut String) {
    if defs.is_empty() {
        return;
    }
    out.push('(');
    for (i, def) in defs.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push('$');
        out.push_str(&def.name);
        out.push_str(": ");
        print_type(&def.ty, out);
        if let Some(default) = &def.default_value {
            out.push_str(" = ");
            print_value(default, out);
        }
    }
    out.push(')');
}

fn print_type(ty: &Type, out: &mut String) {
    match ty {
        Type::Named(name) => out.push_str(name),
        Type::List(inner) => {
            out.push('[');
            print_type(inner, out);
            out.push(']');
        }
        Type::NonNull(inner) => {
            print_type(inner, out);
            out.push('!');
        }
    }
}

fn print_selection_set(set: &SelectionSet, indent: usize, out: &mut String) {
    if set.selections.is_empty() {
        // A leaf field carries an empty selection set. Nothing to print.
        return;
    }
    out.push('{');
    for selection in &set.selections {
        out.push('\n');
        push_indent(indent + 1, out);
        print_selection(selection, indent + 1, out);
    }
    out.push('\n');
    push_indent(indent, out);
    out.push('}');
}

fn print_selection(selection: &Selection, indent: usize, out: &mut String) {
    match selection {
        Selection::Field(field) => print_field(field, indent, out),
        Selection::FragmentSpread(spread) => {
            out.push_str("...");
            out.push_str(&spread.fragment_name);
            print_directives(&spread.directives, out);
        }
        Selection::InlineFragment(inline) => print_inline_fragment(inline, indent, out),
    }
}

fn print_field(field: &Field, indent: usize, out: &mut String) {
    if let Some(alias) = &field.alias {
        out.push_str(alias);
        out.push_str(": ");
    }
    out.push_str(&field.name);
    print_arguments(&field.arguments, out);
    print_directives(&field.directives, out);
    if !field.selection_set.selections.is_empty() {
        out.push(' ');
        print_selection_set(&field.selection_set, indent, out);
    }
}

fn print_inline_fragment(inline: &InlineFragment, indent: usize, out: &mut String) {
    out.push_str("...");
    if let Some(type_condition) = &inline.type_condition {
        out.push_str(" on ");
        out.push_str(type_condition);
    }
    print_directives(&inline.directives, out);
    out.push(' ');
    print_selection_set(&inline.selection_set, indent, out);
}

fn print_arguments(args: &[Argument], out: &mut String) {
    if args.is_empty() {
        return;
    }
    out.push('(');
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&arg.name);
        out.push_str(": ");
        print_value(&arg.value, out);
    }
    out.push(')');
}

fn print_directives(directives: &[Directive], out: &mut String) {
    for directive in directives {
        out.push_str(" @");
        out.push_str(&directive.name);
        print_arguments(&directive.arguments, out);
    }
}

fn print_value(value: &Value, out: &mut String) {
    match value {
        Value::Variable(name) => {
            out.push('$');
            out.push_str(name);
        }
        Value::Int(text) => out.push_str(text),
        Value::Float(text) => out.push_str(text),
        Value::String(text) => print_string_value(text, out),
        Value::Boolean(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Null => out.push_str("null"),
        Value::Enum(name) => out.push_str(name),
        Value::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                print_value(item, out);
            }
            out.push(']');
        }
        Value::Object(fields) => {
            out.push_str("{ ");
            for (i, (name, val)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(name);
                out.push_str(": ");
                print_value(val, out);
            }
            out.push_str(" }");
        }
    }
}

fn print_string_value(text: &str, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn push_indent(level: usize, out: &mut String) {
    for _ in 0..level {
        out.push_str("  ");
    }
}
