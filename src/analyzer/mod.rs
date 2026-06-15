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

fn pluralize(word: &str, count: usize) -> String {
    if count == 1 {
        word.to_string()
    } else if word.ends_with('s') || word.ends_with('x') || word.ends_with("ch") || word.ends_with("sh") {
        format!("{}es", word)
    } else {
        format!("{}s", word)
    }
}

fn compute_module_name(path: &std::path::Path) -> String {
    let mut components: Vec<String> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .filter(|s| s != "." && s != "..")
        .collect();

    if components.is_empty() {
        return "root".to_string();
    }

    // Trim leading src/ or root source dirs.
    let strip_prefixes = ["src", "lib", "app", "packages", "source"];
    while !components.is_empty()
        && strip_prefixes
            .iter()
            .any(|p| components[0].eq_ignore_ascii_case(p))
    {
        components.remove(0);
    }

    if components.is_empty() {
        return "root".to_string();
    }

    // Collapse intermediate source directories (e.g. packages/ui/src/index.ts -> ui::index).
    // Keep the last component because it may be a file stem we convert to a module name.
    if components.len() > 1 {
        let last = components.pop().unwrap();
        components.retain(|c| c != "src" && c != "lib");
        components.push(last);
    }

    // Handle Rust-style mod.rs / lib.rs / main.rs and ordinary .rs files.
    if let Some(last) = components.last().cloned() {
        if last == "mod.rs" {
            components.pop();
        } else if last == "lib.rs" {
            components.pop();
            components.push("lib".to_string());
        } else if last == "main.rs" {
            components.pop();
            components.push("main".to_string());
        } else if let Some(stem) = last
            .rsplit_once('.')
            .filter(|(_, ext)| matches!(*ext, "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go"))
            .map(|(stem, _)| stem)
        {
            components.pop();
            if !stem.is_empty() {
                components.push(stem.to_string());
            }
        }
    }

    if components.is_empty() {
        "root".to_string()
    } else {
        components.join("::")
    }
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
        let module_name = compute_module_name(&file.path);

        let symbols = collect_all_symbols(&file.symbols);
        let _public_count = symbols.iter().filter(|s| s.is_public).count();

        let description = if !file.symbols.is_empty() {
            let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for sym in &file.symbols {
                *counts.entry(format!("{}", sym.kind)).or_insert(0) += 1;
            }
            let mut parts: Vec<String> = counts
                .into_iter()
                .map(|(kind, count)| format!("{} {}", count, pluralize(&kind, count)))
                .collect();
            parts.sort();
            format!("{:?} module with {}", file.language, parts.join(", "))
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
        let module_name = compute_module_name(&file.path);

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

fn parse_cargo_toml(content: &str) -> Vec<Dependency> {
    let mut deps = Vec::new();

    let Ok(manifest) = content.parse::<toml::Table>() else {
        return deps;
    };

    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let Some(table) = manifest.get(section).and_then(|v| v.as_table()) else {
            continue;
        };
        for (name, value) in table {
            let version = match value {
                toml::Value::String(v) => Some(v.clone()),
                toml::Value::Table(t) => t
                    .get("version")
                    .and_then(|v| v.as_str().map(|s| s.to_string())),
                _ => None,
            };
            deps.push(Dependency {
                name: name.clone(),
                version,
            });
        }
    }

    deps
}

fn parse_package_json(content: &str) -> Vec<Dependency> {
    let mut deps = Vec::new();

    let Ok(json) = serde_json::from_str::<serde_json::Value>(content) else {
        return deps;
    };

    for section in ["dependencies", "devDependencies", "peerDependencies"] {
        if let Some(obj) = json.get(section).and_then(|d| d.as_object()) {
            for (name, version) in obj {
                deps.push(Dependency {
                    name: name.clone(),
                    version: version.as_str().map(|s| s.to_string()),
                });
            }
        }
    }

    deps
}

fn parse_go_mod(content: &str) -> Vec<Dependency> {
    let mut deps = Vec::new();
    let mut in_require = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("require (") {
            in_require = true;
            continue;
        }
        if trimmed == ")" {
            in_require = false;
            continue;
        }

        // Single-line require: require github.com/foo/bar v1.2.3
        if trimmed.starts_with("require ") && !trimmed.contains('(') {
            let parts: Vec<&str> = trimmed.split_whitespace().skip(1).collect();
            if parts.len() >= 2 {
                deps.push(Dependency {
                    name: parts[0].to_string(),
                    version: Some(parts[1].to_string()),
                });
            }
            continue;
        }

        if in_require {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                deps.push(Dependency {
                    name: parts[0].to_string(),
                    version: Some(parts[1].to_string()),
                });
            }
        }
    }

    deps
}

fn parse_pyproject_toml(content: &str) -> Vec<Dependency> {
    let mut deps = Vec::new();

    let Ok(manifest) = content.parse::<toml::Table>() else {
        return deps;
    };

    // [project] dependencies = ["requests>=2.28", "numpy"]
    if let Some(arr) = manifest
        .get("project")
        .and_then(|v| v.as_table())
        .and_then(|t| t.get("dependencies"))
        .and_then(|v| v.as_array())
    {
        for item in arr {
            if let Some(spec) = item.as_str() {
                let (name, version) = split_python_spec(spec);
                deps.push(Dependency {
                    name: name.to_string(),
                    version: version.map(|s| s.to_string()),
                });
            }
        }
    }

    // [tool.poetry.dependencies]
    if let Some(table) = manifest
        .get("tool")
        .and_then(|v| v.as_table())
        .and_then(|t| t.get("poetry"))
        .and_then(|v| v.as_table())
        .and_then(|t| t.get("dependencies"))
        .and_then(|v| v.as_table())
    {
        for (name, value) in table {
            if name == "python" {
                continue;
            }
            let version = match value {
                toml::Value::String(v) => Some(v.clone()),
                toml::Value::Table(t) => t
                    .get("version")
                    .and_then(|v| v.as_str().map(|s| s.to_string())),
                _ => None,
            };
            deps.push(Dependency {
                name: name.clone(),
                version,
            });
        }
    }

    deps
}

fn split_python_spec(spec: &str) -> (&str, Option<&str>) {
    let delimiters = ['=', '<', '>', '~', '!', ';'];
    if let Some(pos) = spec.find(&delimiters[..]) {
        let name = spec[..pos].trim();
        let version = spec[pos..].trim();
        let version = version
            .strip_prefix('=')
            .map(|s| s.trim())
            .unwrap_or(version);
        (name, Some(version).filter(|s| !s.is_empty()))
    } else {
        (spec.trim(), None)
    }
}

fn parse_requirements_txt(content: &str) -> Vec<Dependency> {
    let mut deps = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (name, version) = split_python_spec(trimmed);
        deps.push(Dependency {
            name: name.to_string(),
            version: version.map(|s| s.to_string()),
        });
    }

    deps
}

fn parse_manifest(root: &str) -> Vec<Dependency> {
    let mut deps = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let try_parse = |path: &str, parser: fn(&str) -> Vec<Dependency>| -> Vec<Dependency> {
        std::fs::read_to_string(path)
            .map(|content| parser(&content))
            .unwrap_or_default()
    };

    for dep in try_parse(&format!("{}/Cargo.toml", root), parse_cargo_toml) {
        if seen.insert(format!("{}@{:?}", dep.name, dep.version)) {
            deps.push(dep);
        }
    }

    for dep in try_parse(&format!("{}/package.json", root), parse_package_json) {
        if seen.insert(format!("{}@{:?}", dep.name, dep.version)) {
            deps.push(dep);
        }
    }

    for dep in try_parse(&format!("{}/go.mod", root), parse_go_mod) {
        if seen.insert(format!("{}@{:?}", dep.name, dep.version)) {
            deps.push(dep);
        }
    }

    for dep in try_parse(&format!("{}/pyproject.toml", root), parse_pyproject_toml) {
        if seen.insert(format!("{}@{:?}", dep.name, dep.version)) {
            deps.push(dep);
        }
    }

    for dep in try_parse(&format!("{}/requirements.txt", root), parse_requirements_txt) {
        if seen.insert(format!("{}@{:?}", dep.name, dep.version)) {
            deps.push(dep);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn module_names_for_rust_layout() {
        assert_eq!(compute_module_name(Path::new("./src/main.rs")), "main");
        assert_eq!(compute_module_name(Path::new("src/lib.rs")), "lib");
        assert_eq!(compute_module_name(Path::new("src/analyzer/mod.rs")), "analyzer");
        assert_eq!(compute_module_name(Path::new("src/foo/bar.rs")), "foo::bar");
        assert_eq!(compute_module_name(Path::new("packages/ui/src/index.ts")), "ui::index");
    }

    #[test]
    fn cargo_toml_parsing_handles_inline_tables() {
        let content = r#"
[package]
name = "demo"
version = "0.1.0"

[dependencies]
clap = { version = "4", features = ["derive"] }
serde = "1"
tokio = { version = "1.35", features = ["full"] }

[dev-dependencies]
pretty_assertions = "1"
"#;
        let deps = parse_cargo_toml(content);
        let names: Vec<&str> = deps.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"clap"));
        assert!(names.contains(&"serde"));
        assert!(names.contains(&"tokio"));
        assert!(names.contains(&"pretty_assertions"));

        let clap = deps.iter().find(|d| d.name == "clap").unwrap();
        assert_eq!(clap.version.as_deref(), Some("4"));

        let serde = deps.iter().find(|d| d.name == "serde").unwrap();
        assert_eq!(serde.version.as_deref(), Some("1"));
    }

    #[test]
    fn package_json_parsing_includes_dev_deps() {
        let content = r#"
{
  "dependencies": {
    "react": "^18.0.0"
  },
  "devDependencies": {
    "jest": "^29.0.0"
  }
}
"#;
        let deps = parse_package_json(content);
        assert!(deps.iter().any(|d| d.name == "react" && d.version.as_deref() == Some("^18.0.0")));
        assert!(deps.iter().any(|d| d.name == "jest" && d.version.as_deref() == Some("^29.0.0")));
    }

    #[test]
    fn go_mod_parsing_handles_single_and_block_requires() {
        let content = r#"
module github.com/example/app

go 1.21

require github.com/foo/bar v1.2.3

require (
    github.com/baz/qux v2.0.0
    github.com/some/thing v0.5.0
)
"#;
        let deps = parse_go_mod(content);
        assert!(deps.iter().any(|d| d.name == "github.com/foo/bar"));
        assert!(deps.iter().any(|d| d.name == "github.com/baz/qux"));
        assert!(deps.iter().any(|d| d.name == "github.com/some/thing"));
    }

    #[test]
    fn pyproject_toml_parsing_handles_pep621_and_poetry() {
        let content = r#"
[project]
name = "myapp"
dependencies = [
    "requests>=2.28",
    "numpy",
]

[tool.poetry.dependencies]
python = "^3.11"
pendulum = "^3.0"
httpx = { version = ">=0.25", extras = ["http2"] }
"#;
        let deps = parse_pyproject_toml(content);
        assert!(deps.iter().any(|d| d.name == "requests"));
        assert!(deps.iter().any(|d| d.name == "numpy"));
        assert!(deps.iter().any(|d| d.name == "pendulum"));
        assert!(deps.iter().any(|d| d.name == "httpx"));
        assert!(!deps.iter().any(|d| d.name == "python"));
    }

    #[test]
    fn requirements_txt_parsing_handles_versions() {
        let content = r#"
# comment
requests>=2.28
numpy<2.0; python_version < "3.12"
pytest
"#;
        let deps = parse_requirements_txt(content);
        assert!(deps.iter().any(|d| d.name == "requests"));
        assert!(deps.iter().any(|d| d.name == "numpy"));
        assert!(deps.iter().any(|d| d.name == "pytest"));
    }
}
