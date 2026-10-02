use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tree_sitter::Parser;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Symbol {
    pub name: String,
    pub kind: String,
    pub start_line: usize,
    pub end_line: usize,
}

pub(crate) struct ParsedSource {
    pub symbols: Vec<Symbol>,
    pub syntax_errors: bool,
}

pub(crate) fn parse(content: &str, language: &str) -> Result<ParsedSource> {
    if language != "python" {
        return Ok(ParsedSource {
            symbols: Vec::new(),
            syntax_errors: false,
        });
    }
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_python::LANGUAGE.into())?;
    let tree = parser
        .parse(content, None)
        .context("Python parsing did not complete")?;
    let mut symbols = Vec::new();
    let mut cursor = tree.walk();
    // Walk iteratively: untrusted, deeply nested source must not overflow the Rust stack.
    loop {
        let node = cursor.node();
        let kind = match node.kind() {
            "function_definition" => Some("function"),
            "class_definition" => Some("class"),
            "import_statement" | "import_from_statement" => Some("import"),
            _ => None,
        };
        if let Some(kind) = kind {
            let name_node = if kind == "import" {
                Some(node)
            } else {
                node.child_by_field_name("name")
            };
            if let Some(name_node) = name_node {
                symbols.push(Symbol {
                    name: name_node.utf8_text(content.as_bytes())?.to_owned(),
                    kind: kind.to_owned(),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + usize::from(node.end_position().column > 0),
                });
            }
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return Ok(ParsedSource {
                    symbols,
                    syntax_errors: tree.root_node().has_error(),
                });
            }
        }
    }
}
