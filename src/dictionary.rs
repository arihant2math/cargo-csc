use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryDefinition {
    pub name: String,
    pub paths: Vec<String>,
    #[serde(default)]
    pub globs: Vec<String>,
}
