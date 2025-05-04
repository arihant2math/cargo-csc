use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictionaryDefinition {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub globs: Vec<String>,
    #[serde(default)]
    pub compile: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub dictionaries: Vec<String>,
    #[serde(default, alias = "dictionaryDefinitions")]
    pub dictionary_definitions: Vec<DictionaryDefinition>,
    #[serde(default, alias = "ignorePaths")]
    pub ignore_paths: Vec<String>,
    #[serde(default)]
    pub words: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            dictionaries: vec![
                "extra".to_string(),
                "en-US".to_string(),
                "software_terms".to_string(),
                "software_tools".to_string(),
                "words".to_string(),
            ],
            dictionary_definitions: vec![],
            ignore_paths: vec![],
            words: vec![],
        }
    }
}

impl Settings {
    pub fn new() -> Self {
        Settings::default()
    }

    pub fn load_from_file<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Self> {
        let data = std::fs::read_to_string(path)?;
        let settings: Settings = serde_hjson::from_str(&data)?;
        Ok(settings)
    }

    pub fn save_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> anyhow::Result<()> {
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn load(override_: Option<String>) -> Self {
        let path = override_.unwrap_or_else(|| "code-spellcheck.json".to_string());
        if std::path::Path::new(&path).exists() {
            match Settings::load_from_file(&path) {
                Ok(settings) => settings,
                Err(e) => {
                    eprintln!("Error loading settings from {}: {}", path, e);
                    Settings::default()
                }
            }
        } else {
            Settings::default()
        }
    }
}
