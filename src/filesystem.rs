use anyhow::Context;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{Settings, Trie, TrieHashStore, MultiTrie, store_path};

fn get_wordlist<P: AsRef<Path>>(name: &str, dir: P) -> anyhow::Result<Option<Trie>> {
    let path = dir.as_ref().join(format!("{}.txt", name));
    let dir_option = dir.as_ref().join(name);
    if path.exists() {
        Trie::from_wordlist(&path).context(format!("Failed to load wordlist: {}", name))?;
    } else if path.with_extension("dic").exists() {
        Trie::from_wordlist(&path.with_extension("dic"))
            .context(format!("Failed to load wordlist: {}", name))?;
    } else if dir_option.exists() && dir_option.is_dir() {
        Trie::from_directory(&dir_option).context(format!("Failed to load wordlist: {}", name))?;
    }
    Ok(None)
}

pub fn compile_wordlist<P: AsRef<Path>>(path: P, output: P) -> anyhow::Result<()> {
    let trie = Trie::from_wordlist(&path)?;
    let data = trie.dump()?;
    std::fs::write(&output, data)?;
    let hash_store_path = store_path().join("wordlist_hashes.json");
    let mut hash_store =
        TrieHashStore::load_from_file(&hash_store_path).unwrap_or_else(|_| TrieHashStore::new());
    let hash = hash_file(path.as_ref())?;

    hash_store
        .0
        .insert(path.as_ref().display().to_string(), hash);
    hash_store.dump_to_file(&hash_store_path)?;
    Ok(())
}

fn hash_file<P: AsRef<Path>>(path: P) -> anyhow::Result<String> {
    let text = crate::fs::read(&path)
        .with_context(|| format!("Failed to read file: {}", path.as_ref().display()))?;
    Ok(blake3::hash(&text).to_hex().to_string())
}

fn get_or_compile_wordlist(
    name: &str,
    definitions: &[crate::settings::DictionaryDefinition],
) -> anyhow::Result<Trie> {
    let definition = definitions
        .iter()
        .find(|def| def.name == name)
        .cloned()
        .unwrap_or(crate::settings::DictionaryDefinition {
            name: name.to_string(),
            path: store_path()
                .join(format!("{}.txt", name))
                .to_string_lossy()
                .to_string(),
            globs: vec![],
            compile: true,
        });
    let aliases = HashMap::from([("en_US", "en-US"), ("softwareTerms", "software_terms")]);
    if let Some(alias) = aliases.get(name) {
        if !definitions.iter().any(|def| &def.name == alias) {
            return get_or_compile_wordlist(alias, definitions);
        }
    }
    if definition.compile {
        let parent = Path::new(&definition.path)
            .parent()
            .unwrap_or_else(|| Path::new("."));
        let text_path = PathBuf::from(&definition.path);
        let bin_path = parent.join(format!("{}.bin", name));

        let hash_store = TrieHashStore::load_from_file(store_path().join("wordlist_hashes.json"))
            .unwrap_or_else(|_| TrieHashStore::new());
        let hash = hash_file(store_path().join(format!("{}.txt", name)))
            .context(format!("Failed to hash wordlist: {}", name))?;
        if !bin_path.exists() {
            compile_wordlist(&text_path, &bin_path)
                .context(format!("Failed to compile wordlist to trie: {}", name))?;
        }
        if let Some(stored_hash) = hash_store.0.get(name) {
            if stored_hash != &hash {
                compile_wordlist(&text_path, &bin_path)
                    .context(format!("Failed to compile wordlist to trie: {}", name))?;
            }
        } else {
            compile_wordlist(&text_path, &bin_path)
                .context(format!("Failed to compile wordlist to trie: {}", name))?;
        }
        let mut result = Trie::load_from_file(&bin_path);
        if result.is_err() {
            println!("Failed to load binary trie, recompiling");
            std::fs::remove_file(bin_path)?;
            result = get_or_compile_wordlist(name, definitions);
        }
        Ok(result
            .context(format!("Failed to load trie binary: {}", name))?)
    } else {
        Ok(Trie::from_wordlist(&definition.path)
            .context(format!("Failed to load wordlist: {}", name))?)
    }
}

pub fn get_trie(file: &PathBuf, settings: &Settings) -> anyhow::Result<MultiTrie> {
    let mut trie = MultiTrie::new();
    let mut tries = settings.dictionaries.clone();
    match crate::get_file_extension(file).unwrap().as_str() {
        "rs" => {
            tries.push("rust".to_string());
        }
        e => {
            eprintln!("Unsupported file type: {}", e);
        }
    }
    for name in tries {
        let trie_instance = get_or_compile_wordlist(&name, &settings.dictionary_definitions)
            .context(format!("Failed to load wordlist: {}", &name))?;
        trie.inner.push(trie_instance);
    }
    let custom_trie = Trie::from_iterator(settings.words.iter().map(|s| s.to_string()));
    trie.inner.push(custom_trie);
    Ok(trie)
}
