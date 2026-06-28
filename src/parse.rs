//! A recursive-descent parser for executable GraphQL documents.
//!
//! The parser covers the executable subset of the GraphQL grammar. It rejects
//! type-system definitions with a clear error. Integer and float literals keep
//! their source text so that printing round-trips them unchanged. String
//! literals are unescaped during parsing, including block strings.

use crate::ast::{
    Argument, Definition, Directive, Document, Field, FragmentDefinition, FragmentSpread,
    InlineFragment, OperationDefinition, OperationType, Selection, SelectionSet, Type, Value,
    VariableDefinition,
};
use std::fmt;

/// A parse failure with a human-readable message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    message: String,
}

impl ParseError {
    fn new(message: impl Into<String>) -> Self {
        ParseError {
            message: message.into(),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parse an executable GraphQL document from source text.
///
/// # Errors
///
/// Returns a [`ParseError`] when the input is not a valid executable document.
/// Type-system definitions and trailing garbage are rejected.
pub fn parse_document(source: &str) -> Result<Document, ParseError> {
    let mut parser = Parser::new(source);
    let document = parser.parse_document()?;
    Ok(document)
}

/// A single lexical token.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Name(String),
    Int(String),
    Float(String),
    String(String),
    Punct(char),
    Spread,
    Eof,
}

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        // A leading byte-order mark (U+FEFF, encoded as EF BB BF) is the only
        // place these bytes are ignorable. Strip it once here so the token loop
        // never treats a stray continuation byte as whitespace.
        let pos = if src.as_bytes().starts_with(&[0xef, 0xbb, 0xbf]) {
            3
        } else {
            0
        };
        Lexer {
            src,
            bytes: src.as_bytes(),
            pos,
        }
    }

    fn skip_ignored(&mut self) -> Result<(), ParseError> {
        loop {
            let Some(&b) = self.bytes.get(self.pos) else {
                return Ok(());
            };
            match b {
                b' ' | b'\t' | b'\r' | b'\n' | b',' => {
                    // Whitespace, line terminators, and commas are insignificant.
                    self.pos += 1;
                }
                b'#' => {
                    self.pos += 1;
                    while let Some(&c) = self.bytes.get(self.pos) {
                        if c == b'\n' || c == b'\r' {
                            break;
                        }
                        self.pos += 1;
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn next_token(&mut self) -> Result<Token, ParseError> {
        self.skip_ignored()?;
        let Some(&b) = self.bytes.get(self.pos) else {
            return Ok(Token::Eof);
        };
        match b {
            b'!' | b'$' | b'(' | b')' | b':' | b'=' | b'@' | b'[' | b']' | b'{' | b'}' | b'|'
            | b'&' => {
                self.pos += 1;
                Ok(Token::Punct(b as char))
            }
            b'.' => self.lex_spread(),
            b'"' => self.lex_string(),
            b if b == b'_' || b.is_ascii_alphabetic() => self.lex_name(),
            b'-' | b'0'..=b'9' => self.lex_number(),
            _ => Err(ParseError::new(format!(
                "unexpected character {:?}",
                b as char
            ))),
        }
    }

    fn lex_spread(&mut self) -> Result<Token, ParseError> {
        if self.bytes[self.pos..].starts_with(b"...") {
            self.pos += 3;
            Ok(Token::Spread)
        } else {
            Err(ParseError::new("expected '...'"))
        }
    }

    fn lex_name(&mut self) -> Result<Token, ParseError> {
        let start = self.pos;
        while let Some(&c) = self.bytes.get(self.pos) {
            if c == b'_' || c.is_ascii_alphanumeric() {
                self.pos += 1;
            } else {
                break;
            }
        }
        Ok(Token::Name(self.src[start..self.pos].to_string()))
    }

    fn lex_number(&mut self) -> Result<Token, ParseError> {
        let start = self.pos;
        let mut is_float = false;
        if self.bytes.get(self.pos) == Some(&b'-') {
            self.pos += 1;
        }
        while let Some(&c) = self.bytes.get(self.pos) {
            if c.is_ascii_digit() {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.bytes.get(self.pos) == Some(&b'.') {
            is_float = true;
            self.pos += 1;
            while let Some(&c) = self.bytes.get(self.pos) {
                if c.is_ascii_digit() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
        }
        if matches!(self.bytes.get(self.pos), Some(&b'e') | Some(&b'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.bytes.get(self.pos), Some(&b'+') | Some(&b'-')) {
                self.pos += 1;
            }
            while let Some(&c) = self.bytes.get(self.pos) {
                if c.is_ascii_digit() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
        }
        let text = self.src[start..self.pos].to_string();
        if is_float {
            Ok(Token::Float(text))
        } else {
            Ok(Token::Int(text))
        }
    }

    fn lex_string(&mut self) -> Result<Token, ParseError> {
        if self.bytes[self.pos..].starts_with(b"\"\"\"") {
            return self.lex_block_string();
        }
        self.pos += 1; // opening quote
        let mut out = String::new();
        loop {
            let Some(&c) = self.bytes.get(self.pos) else {
                return Err(ParseError::new("unterminated string"));
            };
            match c {
                b'"' => {
                    self.pos += 1;
                    return Ok(Token::String(out));
                }
                b'\\' => {
                    self.pos += 1;
                    self.lex_escape(&mut out)?;
                }
                b'\n' | b'\r' => return Err(ParseError::new("unterminated string")),
                _ => {
                    let ch = self.next_char();
                    out.push(ch);
                }
            }
        }
    }

    fn next_char(&mut self) -> char {
        let rest = &self.src[self.pos..];
        let ch = rest.chars().next().expect("non-empty rest");
        self.pos += ch.len_utf8();
        ch
    }

    fn lex_escape(&mut self, out: &mut String) -> Result<(), ParseError> {
        let Some(&c) = self.bytes.get(self.pos) else {
            return Err(ParseError::new("unterminated escape"));
        };
        self.pos += 1;
        match c {
            b'"' => out.push('"'),
            b'\\' => out.push('\\'),
            b'/' => out.push('/'),
            b'b' => out.push('\u{0008}'),
            b'f' => out.push('\u{000C}'),
            b'n' => out.push('\n'),
            b'r' => out.push('\r'),
            b't' => out.push('\t'),
            b'u' => {
                let hex = self
                    .src
                    .get(self.pos..self.pos + 4)
                    .ok_or_else(|| ParseError::new("incomplete unicode escape"))?;
                let code = u32::from_str_radix(hex, 16)
                    .map_err(|_| ParseError::new("invalid unicode escape"))?;
                let ch = char::from_u32(code)
                    .ok_or_else(|| ParseError::new("invalid unicode code point"))?;
                out.push(ch);
                self.pos += 4;
            }
            other => {
                return Err(ParseError::new(format!(
                    "invalid escape sequence \\{}",
                    other as char
                )))
            }
        }
        Ok(())
    }

    fn lex_block_string(&mut self) -> Result<Token, ParseError> {
        self.pos += 3; // opening triple quote
        let mut raw = String::new();
        loop {
            if self.bytes[self.pos..].starts_with(b"\"\"\"") {
                self.pos += 3;
                break;
            }
            let Some(&c) = self.bytes.get(self.pos) else {
                return Err(ParseError::new("unterminated block string"));
            };
            if c == b'\\' && self.bytes[self.pos..].starts_with(b"\\\"\"\"") {
                raw.push_str("\"\"\"");
                self.pos += 4;
            } else {
                let ch = self.next_char();
                raw.push(ch);
            }
        }
        Ok(Token::String(dedent_block_string(&raw)))
    }
}

/// Apply the GraphQL block-string dedent algorithm.
pub(crate) fn dedent_block_string(raw: &str) -> String {
    let lines: Vec<&str> = raw.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    let mut common_indent: Option<usize> = None;
    for line in lines.iter().skip(1) {
        let indent = line.len() - line.trim_start().len();
        if indent < line.len() {
            common_indent = Some(match common_indent {
                Some(c) => c.min(indent),
                None => indent,
            });
        }
    }
    let mut result: Vec<String> = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            result.push((*line).to_string());
        } else {
            let indent = common_indent.unwrap_or(0);
            let trimmed = if line.len() >= indent {
                &line[indent..]
            } else {
                ""
            };
            result.push(trimmed.to_string());
        }
    }
    while result.first().map(|l| l.trim().is_empty()).unwrap_or(false) {
        result.remove(0);
    }
    while result.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        result.pop();
    }
    result.join("\n")
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    current: Token,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        let mut lexer = Lexer::new(src);
        let current = lexer.next_token().unwrap_or(Token::Eof);
        Parser { lexer, current }
    }

    fn bump(&mut self) -> Result<Token, ParseError> {
        let next = self.lexer.next_token()?;
        Ok(std::mem::replace(&mut self.current, next))
    }

    fn expect_punct(&mut self, c: char) -> Result<(), ParseError> {
        if self.current == Token::Punct(c) {
            self.bump()?;
            Ok(())
        } else {
            Err(ParseError::new(format!(
                "expected {:?}, found {:?}",
                c, self.current
            )))
        }
    }

    fn peek_punct(&self, c: char) -> bool {
        self.current == Token::Punct(c)
    }

    fn expect_name(&mut self) -> Result<String, ParseError> {
        match self.bump()? {
            Token::Name(n) => Ok(n),
            other => Err(ParseError::new(format!("expected name, found {:?}", other))),
        }
    }

    fn expect_keyword(&mut self, kw: &str) -> Result<(), ParseError> {
        match &self.current {
            Token::Name(n) if n == kw => {
                self.bump()?;
                Ok(())
            }
            other => Err(ParseError::new(format!(
                "expected '{}', found {:?}",
                kw, other
            ))),
        }
    }

    fn parse_document(&mut self) -> Result<Document, ParseError> {
        let mut definitions = Vec::new();
        while self.current != Token::Eof {
            definitions.push(self.parse_definition()?);
        }
        if definitions.is_empty() {
            return Err(ParseError::new("document has no definitions"));
        }
        Ok(Document { definitions })
    }

    fn parse_definition(&mut self) -> Result<Definition, ParseError> {
        match &self.current {
            Token::Punct('{') => Ok(Definition::Operation(self.parse_shorthand_operation()?)),
            Token::Name(n) => match n.as_str() {
                "query" => Ok(Definition::Operation(
                    self.parse_operation(OperationType::Query)?,
                )),
                "mutation" => Ok(Definition::Operation(
                    self.parse_operation(OperationType::Mutation)?,
                )),
                "subscription" => Ok(Definition::Operation(
                    self.parse_operation(OperationType::Subscription)?,
                )),
                "fragment" => Ok(Definition::Fragment(self.parse_fragment_definition()?)),
                other => Err(ParseError::new(format!(
                    "unexpected keyword {:?}; only executable definitions are supported",
                    other
                ))),
            },
            other => Err(ParseError::new(format!(
                "unexpected token {:?} at definition",
                other
            ))),
        }
    }

    fn parse_shorthand_operation(&mut self) -> Result<OperationDefinition, ParseError> {
        let selection_set = self.parse_selection_set()?;
        Ok(OperationDefinition {
            operation: OperationType::Query,
            name: None,
            variable_definitions: Vec::new(),
            directives: Vec::new(),
            selection_set,
        })
    }

    fn parse_operation(
        &mut self,
        operation: OperationType,
    ) -> Result<OperationDefinition, ParseError> {
        self.bump()?; // operation keyword
        let name = if let Token::Name(_) = &self.current {
            Some(self.expect_name()?)
        } else {
            None
        };
        let variable_definitions = self.parse_variable_definitions()?;
        let directives = self.parse_directives()?;
        let selection_set = self.parse_selection_set()?;
        Ok(OperationDefinition {
            operation,
            name,
            variable_definitions,
            directives,
            selection_set,
        })
    }

    fn parse_fragment_definition(&mut self) -> Result<FragmentDefinition, ParseError> {
        self.bump()?; // `fragment`
        let name = self.expect_name()?;
        let variable_definitions = self.parse_variable_definitions()?;
        self.expect_keyword("on")?;
        let type_condition = self.expect_name()?;
        let directives = self.parse_directives()?;
        let selection_set = self.parse_selection_set()?;
        Ok(FragmentDefinition {
            name,
            variable_definitions,
            type_condition,
            directives,
            selection_set,
        })
    }

    fn parse_variable_definitions(&mut self) -> Result<Vec<VariableDefinition>, ParseError> {
        if !self.peek_punct('(') {
            return Ok(Vec::new());
        }
        self.expect_punct('(')?;
        let mut defs = Vec::new();
        while !self.peek_punct(')') {
            self.expect_punct('$')?;
            let name = self.expect_name()?;
            self.expect_punct(':')?;
            let ty = self.parse_type()?;
            let default_value = if self.peek_punct('=') {
                self.expect_punct('=')?;
                Some(self.parse_value()?)
            } else {
                None
            };
            // Directives on a variable definition are consumed but not stored.
            // The sort transform never reorders them and the printer omits them.
            self.parse_directives()?;
            defs.push(VariableDefinition {
                name,
                ty,
                default_value,
            });
        }
        self.expect_punct(')')?;
        Ok(defs)
    }

    fn parse_type(&mut self) -> Result<Type, ParseError> {
        let mut ty = if self.peek_punct('[') {
            self.expect_punct('[')?;
            let inner = self.parse_type()?;
            self.expect_punct(']')?;
            Type::List(Box::new(inner))
        } else {
            Type::Named(self.expect_name()?)
        };
        if self.peek_punct('!') {
            self.expect_punct('!')?;
            ty = Type::NonNull(Box::new(ty));
        }
        Ok(ty)
    }

    fn parse_directives(&mut self) -> Result<Vec<Directive>, ParseError> {
        let mut directives = Vec::new();
        while self.peek_punct('@') {
            self.expect_punct('@')?;
            let name = self.expect_name()?;
            let arguments = self.parse_arguments()?;
            directives.push(Directive { name, arguments });
        }
        Ok(directives)
    }

    fn parse_arguments(&mut self) -> Result<Vec<Argument>, ParseError> {
        if !self.peek_punct('(') {
            return Ok(Vec::new());
        }
        self.expect_punct('(')?;
        let mut args = Vec::new();
        while !self.peek_punct(')') {
            let name = self.expect_name()?;
            self.expect_punct(':')?;
            let value = self.parse_value()?;
            args.push(Argument { name, value });
        }
        self.expect_punct(')')?;
        Ok(args)
    }

    fn parse_selection_set(&mut self) -> Result<SelectionSet, ParseError> {
        self.expect_punct('{')?;
        let mut selections = Vec::new();
        while !self.peek_punct('}') {
            selections.push(self.parse_selection()?);
        }
        self.expect_punct('}')?;
        Ok(SelectionSet { selections })
    }

    fn parse_selection(&mut self) -> Result<Selection, ParseError> {
        if self.current == Token::Spread {
            self.bump()?;
            // A spread is either `...Name`, `...on Type`, or `...` followed by
            // directives and a selection set (an inline fragment with no type).
            if let Token::Name(n) = &self.current {
                if n == "on" {
                    self.bump()?;
                    let type_condition = Some(self.expect_name()?);
                    let directives = self.parse_directives()?;
                    let selection_set = self.parse_selection_set()?;
                    return Ok(Selection::InlineFragment(InlineFragment {
                        type_condition,
                        directives,
                        selection_set,
                    }));
                }
                let fragment_name = self.expect_name()?;
                let directives = self.parse_directives()?;
                return Ok(Selection::FragmentSpread(FragmentSpread {
                    fragment_name,
                    directives,
                }));
            }
            let directives = self.parse_directives()?;
            let selection_set = self.parse_selection_set()?;
            return Ok(Selection::InlineFragment(InlineFragment {
                type_condition: None,
                directives,
                selection_set,
            }));
        }

        let first = self.expect_name()?;
        let (alias, name) = if self.peek_punct(':') {
            self.expect_punct(':')?;
            (Some(first), self.expect_name()?)
        } else {
            (None, first)
        };
        let arguments = self.parse_arguments()?;
        let directives = self.parse_directives()?;
        let selection_set = if self.peek_punct('{') {
            self.parse_selection_set()?
        } else {
            SelectionSet {
                selections: Vec::new(),
            }
        };
        Ok(Selection::Field(Field {
            alias,
            name,
            arguments,
            directives,
            selection_set,
        }))
    }

    fn parse_value(&mut self) -> Result<Value, ParseError> {
        match self.current.clone() {
            Token::Punct('$') => {
                self.bump()?;
                Ok(Value::Variable(self.expect_name()?))
            }
            Token::Int(text) => {
                self.bump()?;
                Ok(Value::Int(text))
            }
            Token::Float(text) => {
                self.bump()?;
                Ok(Value::Float(text))
            }
            Token::String(text) => {
                self.bump()?;
                Ok(Value::String(text))
            }
            Token::Name(n) => {
                self.bump()?;
                match n.as_str() {
                    "true" => Ok(Value::Boolean(true)),
                    "false" => Ok(Value::Boolean(false)),
                    "null" => Ok(Value::Null),
                    _ => Ok(Value::Enum(n)),
                }
            }
            Token::Punct('[') => {
                self.bump()?;
                let mut items = Vec::new();
                while !self.peek_punct(']') {
                    items.push(self.parse_value()?);
                }
                self.expect_punct(']')?;
                Ok(Value::List(items))
            }
            Token::Punct('{') => {
                self.bump()?;
                let mut fields = Vec::new();
                while !self.peek_punct('}') {
                    let name = self.expect_name()?;
                    self.expect_punct(':')?;
                    let value = self.parse_value()?;
                    fields.push((name, value));
                }
                self.expect_punct('}')?;
                Ok(Value::Object(fields))
            }
            other => Err(ParseError::new(format!(
                "unexpected value token {:?}",
                other
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::print::print_document;

    fn roundtrip(src: &str) -> String {
        print_document(&parse_document(src).unwrap())
    }

    #[test]
    fn comments_and_commas_are_ignored() {
        let out = roundtrip("# leading comment\n{ a, b # trailing\n c }");
        assert_eq!(out, "{\n  a\n  b\n  c\n}\n");
    }

    #[test]
    fn string_escapes_round_trip() {
        let doc = parse_document(r#"{ f(x: "a\tb\n\"c\"") }"#).unwrap();
        let Definition::Operation(op) = &doc.definitions[0] else {
            panic!("expected operation");
        };
        let Selection::Field(field) = &op.selection_set.selections[0] else {
            panic!("expected field");
        };
        assert_eq!(
            field.arguments[0].value,
            Value::String("a\tb\n\"c\"".into())
        );
    }

    #[test]
    fn unicode_escape_decodes() {
        let doc = parse_document(r#"{ f(x: "A") }"#).unwrap();
        let Definition::Operation(op) = &doc.definitions[0] else {
            panic!("expected operation");
        };
        let Selection::Field(field) = &op.selection_set.selections[0] else {
            panic!("expected field");
        };
        assert_eq!(field.arguments[0].value, Value::String("A".into()));
    }

    #[test]
    fn block_string_dedents() {
        let src = "{ f(x: \"\"\"\n    hello\n    world\n  \"\"\") }";
        let doc = parse_document(src).unwrap();
        let Definition::Operation(op) = &doc.definitions[0] else {
            panic!("expected operation");
        };
        let Selection::Field(field) = &op.selection_set.selections[0] else {
            panic!("expected field");
        };
        assert_eq!(
            field.arguments[0].value,
            Value::String("hello\nworld".into())
        );
    }

    #[test]
    fn integer_and_float_text_preserved() {
        let doc = parse_document("{ f(a: 007, b: 1.50e3, c: -2) }").unwrap();
        let Definition::Operation(op) = &doc.definitions[0] else {
            panic!("expected operation");
        };
        let Selection::Field(field) = &op.selection_set.selections[0] else {
            panic!("expected field");
        };
        assert_eq!(field.arguments[0].value, Value::Int("007".into()));
        assert_eq!(field.arguments[1].value, Value::Float("1.50e3".into()));
        assert_eq!(field.arguments[2].value, Value::Int("-2".into()));
    }

    #[test]
    fn type_system_definition_is_rejected() {
        let err = parse_document("type Query { a: Int }").unwrap_err();
        assert!(err.to_string().contains("only executable"));
    }

    #[test]
    fn empty_document_is_rejected() {
        assert!(parse_document("   ").is_err());
    }

    #[test]
    fn nested_non_null_list_type_round_trips() {
        let out = roundtrip("query ($v: [Int!]!) { f }");
        assert_eq!(out, "query ($v: [Int!]!) {\n  f\n}\n");
    }
}
