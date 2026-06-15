use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::scanner::{SourceFile, Symbol};

pub struct ProjectAnalysis {
    pub name: String,
    pub modules: Vec<Module>,
    pub interfaces: Vec<Interface>,
    pub file_stats: FileStats,
    pub language_stats: HashMap<String, usize>,
    pub dependencies: Vec<Dependency>,
}

pub struct Module {
    pub name: String,
    pub path: PathBuf,
    pub description: String,
    pub symbols: Vec<SymbolInfo>,
    pub line_count: usize,
}

pub struct SymbolInfo {
    pub kind: String,
    pub name: String,
    pub line: usize,
    pub is_public: bool,
}

pub struct Interface {
    pub name: String,
    pub module: String,
    pub kind: String,
    pub line: usize,
}

pub struct FileStats {
    pub total_files: usize,
    pub total_lines: usize,
    pub avg_lines_per_file: usize,
}

pub struct Dependency {
    pub name: String,
    pub version: Option<String>,
}

fn symbol_to_info(sym: &Symbol) -> SymbolInfo {
    SymbolInfo {
        kind: format!("{}", sym.kind),
        name: sym.name.clone(),
        line: sym.line,
        is_public: sym.is_public,
    }
}

fn collect_all_symbols(symbols: &[Symbol]) -> Vec<SymbolInfo> {
    let mut result = Vec::new();
    for sym in symbols {
        result.push(symbol_to_info(sym));
        result.extend(collect_all_symbols(&sym.children));
    }
    result
}

fn group_into_modules(files: &[SourceFile]) -> Vec<Module> {
    let mut modules = Vec::new();

    for file in files {
        let module_name = file
            .path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "root".to_string());

        let symbols = collect_all_symbols(&file.symbols);
        let _public_count = symbols.iter().filter(|s| s.is_public).count();

        let description = if !file.symbols.is_empty() {
            let kinds: Vec<String> = file
                .symbols
                .iter()
                .map(|s| format!("{} {}", s.kind, s.name))
                .collect();
            format!("Contains: {}", kinds.join(", "))
        } else {
            format!("{} source file", format!("{:?}", file.language).to_lowercase())
        };

        modules.push(Module {
            name: module_name,
            path: file.path.clone(),
            description,
            symbols,
            line_count: file.line_count,
        });
    }

    modules
}

fn collect_interfaces(files: &[SourceFile]) -> Vec<Interface> {
    let mut interfaces = Vec::new();

    for file in files {
        let module_name = file
            .path
            .file_stem()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        for sym in &file.symbols {
            if sym.is_public {
                interfaces.push(Interface {
                    name: sym.name.clone(),
                    module: module_name.clone(),
                    kind: format!("{}", sym.kind),
                    line: sym.line,
                });
            }
        }
    }

    interfaces
}

fn parse_manifest(root: &str) -> Vec<Dependency> {
    let mut deps = Vec::new();

    // Cargo.toml
    if let Ok(content) = std::fs::read_to_string(format!("{}/Cargo.toml", root)) {
        let mut in_deps = false;
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_deps = line == "[dependencies]" || line == "[dev-dependencies]" || line == "[build-dependencies]";
                continue;
            }
            if !in_deps || line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((name, rest)) = line.split_once('=') {
                let name = name.trim().to_string();
                let rest = rest.trim();
                let version = if rest.starts_with('{') {
                    // Inline table: clap = { version = "4", features = ["derive"] }
                    rest.split("version")
                        .nth(1)
                        .and_then(|s| s.split_once('='))
                        .and_then(|(_, v)| v.split(',').next())
                        .map(|v| v.trim().trim_matches('"').to_string())
                } else {
                    // Simple: walkdir = "2"
                    Some(rest.trim_matches('"').to_string())
                };
                deps.push(Dependency { name, version });
            }
        }
    }

    // package.json
    if let Ok(content) = std::fs::read_to_string(format!("{}/package.json", root)) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(obj) = json.get("dependencies").and_then(|d| d.as_object()) {
                for (name, version) in obj {
                    deps.push(Dependency {
                        name: name.clone(),
                        version: version.as_str().map(|s| s.to_string()),
                    });
                }
            }
        }
    }

    // go.mod
    if let Ok(content) = std::fs::read_to_string(format!("{}/go.mod", root)) {
        let mut in_require = false;
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("require") {
                in_require = true;
                continue;
            }
            if in_require {
                if line == ")" {
                    in_require = false;
                    continue;
                }
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    deps.push(Dependency {
                        name: parts[0].to_string(),
                        version: Some(parts[1].to_string()),
                    });
                }
            }
        }
    }

    deps
}

pub fn analyze(files: &[SourceFile]) -> Result<ProjectAnalysis> {
    let name = std::env::current_dir()?
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());

    let total_lines = files.iter().map(|f| f.line_count).sum();
    let avg_lines = if files.is_empty() { 0 } else { total_lines / files.len() };

    let mut language_stats: HashMap<String, usize> = HashMap::new();
    for file in files {
        let lang = format!("{:?}", file.language).to_lowercase();
        *language_stats.entry(lang).or_insert(0) += file.line_count;
    }

    let modules = group_into_modules(files);
    let interfaces = collect_interfaces(files);
    let dependencies = parse_manifest(".");

    Ok(ProjectAnalysis {
        name,
        modules,
        interfaces,
        file_stats: FileStats {
            total_files: files.len(),
            total_lines,
            avg_lines_per_file: avg_lines,
        },
        language_stats,
        dependencies,
    })
}
