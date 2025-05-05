use anyhow::bail;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io;
use tokio::io::AsyncReadExt;

pub async fn get_code(path: &PathBuf) -> anyhow::Result<(String, tree_sitter::Parser)> {
    let file = File::open(path).await?;
    let mut reader = io::BufReader::new(file);
    let mut source_code = String::new();
    reader.read_to_string(&mut source_code).await?;
    let mut parser = tree_sitter::Parser::new();
    match crate::filesystem::get_file_extension(path)
        .unwrap()
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
        "rs" => {
            parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;
        }
        "toml" => {
            parser.set_language(&tree_sitter_toml_ng::LANGUAGE.into())?;
        }
        e => {
            bail!("Unsupported file type: {}", e);
        }
    }
    Ok((source_code, parser))
}

pub fn handle_node(
    words: &crate::MultiTrie,
    node: &tree_sitter::Node,
    source_code: &str,
) -> Vec<Typo> {
    let start_byte = node.start_byte();
    let end_byte = node.end_byte();
    let text = &source_code[start_byte as usize..end_byte as usize];
    let mut typos = Vec::new();
    if node.is_named() {
        for word in text.split_whitespace() {
            if word.len() > 1 {
                if let Some(typo) = words.handle_identifier(word) {
                    let line = node.start_position().row + 1;
                    let column = node.start_position().column + 1;
                    let typo = Typo {
                        line,
                        column,
                        word: typo,
                    };
                    typos.push(typo);
                }
            }
        }
    }
    for child in node.children(&mut node.walk()) {
        typos.append(&mut handle_node(words, &child, source_code));
    }
    typos
}

#[derive(Debug)]
pub struct Typo {
    pub line: usize,
    pub column: usize,
    pub word: String,
}
