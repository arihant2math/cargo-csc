use std::{
    fmt::{Debug, Display, Formatter},
    path::PathBuf,
    sync::Arc,
};

use miette::{Diagnostic, NamedSource, SourceOffset, SourceSpan};
use tokio::{fs::File, io, io::AsyncReadExt};
use tree_sitter::Node;

pub async fn get_code(path: &PathBuf) -> anyhow::Result<(String, Option<tree_sitter::Parser>)> {
    let file = File::open(path).await?;
    let mut reader = io::BufReader::new(file);
    let mut source_code = String::new();
    reader.read_to_string(&mut source_code).await?;
    let mut parser = tree_sitter::Parser::new();
    let mut found = true;
    match crate::filesystem::get_file_extension(path)
        .unwrap_or_default()
        .as_str()
    {
        "c" => {
            parser.set_language(&tree_sitter_c::LANGUAGE.into())?;
        }
        "cpp" | "c++" => {
            parser.set_language(&tree_sitter_cpp::LANGUAGE.into())?;
        }
        "go" => {
            parser.set_language(&tree_sitter_go::LANGUAGE.into())?;
        }
        "html" => {
            parser.set_language(&tree_sitter_html::LANGUAGE.into())?;
        }
        "js" => {
            parser.set_language(&tree_sitter_javascript::LANGUAGE.into())?;
        }
        "py" => {
            parser.set_language(&tree_sitter_python::LANGUAGE.into())?;
        }
        "md" => {
            parser.set_language(&tree_sitter_md::LANGUAGE.into())?;
        }
        "rb" => {
            parser.set_language(&tree_sitter_ruby::LANGUAGE.into())?;
        }
        "rs" => {
            parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;
        }
        "toml" => {
            parser.set_language(&tree_sitter_toml_ng::LANGUAGE.into())?;
        }
        "ts" => {
            parser.set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())?;
        }
        "tsx" => {
            parser.set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())?;
        }
        _ => {
            found = false;
        }
    }
    if !found {
        Ok((source_code, None))
    } else {
        Ok((source_code, Some(parser)))
    }
}

pub fn handle_node(words: &crate::MultiTrie, node: &Node, source_code: &Arc<str>) -> Vec<Typo> {
    let start_byte = node.start_byte();
    let end_byte = node.end_byte();
    let text = &source_code[start_byte..end_byte];
    let mut typos = Vec::new();
    if node.is_named() && node.child_count() == 0 {
        for word in text.split_whitespace() {
            if word.len() > 1 {
                if let Some(typo) = words.handle_identifier(word) {
                    // TODO: Fix
                    // let suggestion = words.suggestion(&typo);
                    let typo = Typo::from_node(typo, *node, source_code.clone(), None);
                    typos.push(typo);
                }
            }
        }
    }
    for child in node.children(&mut node.walk()) {
        typos.append(&mut handle_node(words, &child, source_code));
    }
    // De-duplicate typos
    typos.dedup_by(|a, b| a.word == b.word && a.line == b.line && a.column == b.column);
    typos
}

pub fn handle_text(words: &crate::MultiTrie, source_code: &Arc<str>) -> Vec<Typo> {
    let mut typos = Vec::new();
    for (line_count, line) in source_code.lines().enumerate() {
        for word in line.split_whitespace() {
            if word.len() > 1 {
                if let Some(typo) = words.handle_identifier(word) {
                    typos.push(Typo {
                        line: line_count + 1,
                        column: line.find(word).unwrap_or(0) + 1,
                        length: word.len(),
                        word: typo,
                        suggestion: None,
                        source: source_code.clone(),
                    });
                }
            }
        }
    }
    // De-duplicate typos
    typos.dedup_by(|a, b| a.word == b.word && a.line == b.line && a.column == b.column);
    typos
}

#[derive(Clone, Debug)]
pub struct Typo {
    pub line: usize,
    pub column: usize,
    pub length: usize,
    pub word: String,
    pub suggestion: Option<String>,
    pub source: Arc<str>,
}

impl Typo {
    fn from_node(
        word: String,
        node: Node,
        source_code: Arc<str>,
        suggestion: Option<String>,
    ) -> Self {
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();
        let line = node.start_position().row + 1;
        let column = node.start_position().column + 1;
        let length = end_byte - start_byte;
        Self {
            line,
            column,
            length,
            word,
            source: source_code,
            suggestion,
        }
    }

    pub fn new_with_suggestion(
        word: String,
        node: Node,
        source_code: Arc<str>,
        suggestion: String,
    ) -> Self {
        Self::from_node(word, node, source_code, Some(suggestion))
    }

    pub fn new_without_suggestion(word: String, node: Node, source_code: Arc<str>) -> Self {
        Self::from_node(word, node, source_code, None)
    }

    pub fn to_diagnostic(&self, file: &str) -> TypoDiagnostic {
        let offset = SourceOffset::from_location(self.source.clone(), self.line, self.column);
        let span = SourceSpan::new(offset, self.length);
        let suggestion_text = match self.suggestion {
            Some(ref suggestion) => format!(" Did you mean `{}`?", suggestion),
            None => String::new(),
        };
        TypoDiagnostic {
            src: NamedSource::new(file, self.source.clone()),
            typo_span: span,
            advice: format!("Unknown word `{}`.{}", self.word, suggestion_text),
        }
    }
}

#[derive(Clone, Diagnostic)]
pub struct TypoDiagnostic {
    #[source_code]
    src: NamedSource<Arc<str>>,
    #[label = "Typo here"]
    typo_span: SourceSpan,
    #[help]
    advice: String,
}

impl Debug for TypoDiagnostic {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "TypoDiagnostic",)
    }
}

impl Display for TypoDiagnostic {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for TypoDiagnostic {}
