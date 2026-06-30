//! AST types for executable GraphQL documents.
//!
//! These types cover the executable subset of the GraphQL grammar: operations,
//! fragments, selection sets, fields, spreads, inline fragments, arguments,
//! directives, variable definitions, types, and values. Type-system (schema)
//! definitions are out of scope because the sort transform targets executable
//! documents.
//!
//! Every list that the sort transform reorders is a plain `Vec`. Value literals
//! keep their authored order. `ObjectValue` fields stay in source order so that
//! sorting never disturbs an input object literal.

/// A parsed executable GraphQL document.
///
/// `definitions` holds operations and fragments in source order until sorted.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Document {
    /// Top-level definitions: operations and fragment definitions.
    pub definitions: Vec<Definition>,
}

/// A top-level definition in a document.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Definition {
    /// An operation definition (query, mutation, or subscription).
    Operation(OperationDefinition),
    /// A fragment definition.
    Fragment(FragmentDefinition),
}

/// The operation kind keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationType {
    /// `query`
    Query,
    /// `mutation`
    Mutation,
    /// `subscription`
    Subscription,
}

impl OperationType {
    /// The lower-case keyword used when printing the operation.
    #[must_use]
    pub fn keyword(self) -> &'static str {
        match self {
            OperationType::Query => "query",
            OperationType::Mutation => "mutation",
            OperationType::Subscription => "subscription",
        }
    }
}

/// An operation definition.
///
/// An operation with no name, no variable definitions, and no directives is a
/// shorthand selection set (`{ ... }`). The [`is_shorthand`](Self::is_shorthand)
/// helper detects that form so the printer can omit the leading keyword.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OperationDefinition {
    /// Operation kind. Always `Query` for the shorthand selection-set form.
    pub operation: OperationType,
    /// Operation name, absent for anonymous operations.
    pub name: Option<String>,
    /// Variable definitions, sorted by variable name.
    pub variable_definitions: Vec<VariableDefinition>,
    /// Directives applied to the operation.
    pub directives: Vec<Directive>,
    /// The operation body.
    pub selection_set: SelectionSet,
}

impl OperationDefinition {
    /// True when this prints as a bare selection set with no keyword or name.
    #[must_use]
    pub fn is_shorthand(&self) -> bool {
        self.operation == OperationType::Query
            && self.name.is_none()
            && self.variable_definitions.is_empty()
            && self.directives.is_empty()
    }
}

/// A fragment definition (`fragment Name on Type { ... }`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FragmentDefinition {
    /// Fragment name.
    pub name: String,
    /// Variable definitions. Empty in standard GraphQL.
    pub variable_definitions: Vec<VariableDefinition>,
    /// The type condition after `on`.
    pub type_condition: String,
    /// Directives applied to the fragment definition.
    pub directives: Vec<Directive>,
    /// The fragment body.
    pub selection_set: SelectionSet,
}

/// A variable definition (`$name: Type = default`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VariableDefinition {
    /// Variable name without the leading `$`.
    pub name: String,
    /// Declared type.
    pub ty: Type,
    /// Default value, if any.
    pub default_value: Option<Value>,
    /// Directives applied to the variable definition, kept in source order.
    pub directives: Vec<Directive>,
}

/// A GraphQL type reference.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    /// A named type such as `Int`.
    Named(String),
    /// A list type such as `[Int]`.
    List(Box<Type>),
    /// A non-null wrapper such as `Int!`.
    NonNull(Box<Type>),
}

/// A selection set (`{ ... }`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SelectionSet {
    /// The selections, sorted by kind then name.
    pub selections: Vec<Selection>,
}

/// One selection inside a selection set.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Selection {
    /// A field selection.
    Field(Field),
    /// A fragment spread (`...Name`).
    FragmentSpread(FragmentSpread),
    /// An inline fragment (`... on Type { ... }`).
    InlineFragment(InlineFragment),
}

/// A field selection.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Field {
    /// Optional alias.
    pub alias: Option<String>,
    /// Field name.
    pub name: String,
    /// Arguments, sorted by argument name.
    pub arguments: Vec<Argument>,
    /// Directives applied to the field.
    pub directives: Vec<Directive>,
    /// Nested selection set. `None` when the field is a leaf with no body.
    pub selection_set: Option<SelectionSet>,
}

/// A fragment spread (`...Name`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FragmentSpread {
    /// The referenced fragment name.
    pub fragment_name: String,
    /// Directives applied to the spread, kept in source order.
    pub directives: Vec<Directive>,
}

/// An inline fragment (`... on Type { ... }`).
///
/// An inline fragment carries no name. In a selection set it sorts after fields
/// and spreads because its kind ranks last.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InlineFragment {
    /// Optional type condition after `on`.
    pub type_condition: Option<String>,
    /// Directives applied to the inline fragment, kept in source order.
    pub directives: Vec<Directive>,
    /// The inline fragment body.
    pub selection_set: SelectionSet,
}

/// A directive (`@name(args)`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Directive {
    /// Directive name without the leading `@`.
    pub name: String,
    /// Directive arguments, sorted by argument name.
    pub arguments: Vec<Argument>,
}

/// A named argument (`name: value`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Argument {
    /// Argument name.
    pub name: String,
    /// Argument value.
    pub value: Value,
}

/// A GraphQL value literal.
///
/// `Object` keeps fields in source order. The sort transform never reorders
/// values, so object fields and list elements print exactly as authored.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Value {
    /// A variable reference (`$name`).
    Variable(String),
    /// An integer literal, kept as its source text.
    Int(String),
    /// A float literal, kept as its source text.
    Float(String),
    /// A string literal value (already unescaped).
    String(String),
    /// A boolean literal.
    Boolean(bool),
    /// The null literal.
    Null,
    /// An enum value.
    Enum(String),
    /// A list literal. Element order is preserved.
    List(Vec<Value>),
    /// An object literal. Field order is preserved.
    Object(Vec<(String, Value)>),
}
