# cargo-csc
A code spell-checker, written in rust. This project aims to be a cleaner and faster replacement for cspell.

# Installation
```bash
cargo install --git https://github.com/arihant2math/cargo-csc
```

## Installing Dictionaries
`cargo-csc` requires dictionaries to test words against.
Most dictionaries (assuming they aren't stored by cspell as tries) can be imported with the following command:
```
cargo-csc import-cspell
```

This command also updates the installed dictionaries if rerun.

Any text files can be imported with `cargo-csc install <URL|PATH>`.

# CLI Usage Guide

The `cargo-csc` CLI is a code spell checker that allows you to identify and manage spelling errors in your codebase. This guide provides an overview of the available commands, arguments, and options to help you effectively use the tool.

## General Usage

```bash
cargo-csc <COMMAND> [OPTIONS]
```

Run `cargo-csc --help` to see the general help menu.

---

## Commands Overview

### **Check**
Checks a directory or set of files for typos.

```bash
cargo-csc check [OPTIONS]
```

#### Options:
- `--dir <PATH>` (required): The path to the folder to scan for typos.
- `--glob <PATTERN>`: A glob pattern to filter files (default: `**/*.*`).
- `--verbose` (`-v`): Enables verbose output.
- `--progress` (`-p`): Displays progress while processing files.
- `--exclude <PATH>`: Files or folders to exclude from the search (can be repeated).
- `--extra-dictionaries <PATH>`: Paths to additional dictionaries to use (can be repeated).
- `--max-depth <DEPTH>`: Maximum directory depth to search.
- `--follow-symlinks`: Follow symbolic links during the search.
- `--max-filesize <SIZE>`: Maximum file size (in bytes) to process.
- `--jobs <NUMBER>` (`-j`): Number of threads to use (default: number of CPUs).
- `--settings <PATH>`: Path to a custom settings file.
- `--output <FORMAT>`: Output format for results (`json` or `text`).

#### Example:
```bash
cargo-csc check src **/*.rs
```

### **Cache**
Manages the cache used by the tool.

```bash
cargo-csc cache <SUBCOMMAND>
```

#### Subcommands:
- `build`: Compile the wordlists into a cache (not implemented yet).
- `clear`: Clear the cached wordlists.

### **Install**
Installs a dictionary from a local file or a URL.

```bash
cargo-csc install <URI>
```

#### Argument:
`<URI>`: Path to a local file or a URL to a dictionary file.

#### Example:
- Install from a local file:
  ```bash
  cargo-csc install ./path/to/dictionary.txt
  ```
- Install from a URL:
  ```bash
  cargo-csc install https://example.com/dictionary.txt
  ```

### **ImportCspell**
Imports dictionaries from the `cspell` tool.
Currently this doesn't support tries.

```bash
cargo-csc import-cspell
```

# Settings
## Example
```json
// code-spellcheck.json
{
  "dictionary_definitions": [
    {
      "name": "custom",
      "path": "./custom.txt"
    }
  ],
  "dictionaries": [
    "custom",
    "en_US",
    "extra",
    "rust",
    "software_terms",
    "software_tools",
    "words"
  ],
  "ignore_paths": [
    "**/target/**",
    "**/node_modules/**"
  ],
  "words": [
    "wordlist",
    "wordlists"
  ]
}
```
