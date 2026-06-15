use anyhow::Result;
use ignore::WalkBuilder;
use std::path::PathBuf;
use tree_sitter::Parser;

pub struct SourceFile {
    pub path: PathBuf,
    pub line_count: usize,
    pub language: Language,
    pub symbols: Vec<Symbol>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Language {
    TypeScript,
    JavaScript,
    Python,
    Rust,
    Go,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Symbol {
    pub kind: SymbolKind,
    pub name: String,
    pub line: usize,
    pub is_public: bool,
    pub children: Vec<Symbol>,
}

#[derive(Debug, Clone)]
pub enum SymbolKind {
    Function,
    Class,
    Struct,
    Interface,
    Impl,
    Enum,
    TypeAlias,
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SymbolKind::Function => write!(f, "function"),
            SymbolKind::Class => write!(f, "class"),
            SymbolKind::Struct => write!(f, "struct"),
            SymbolKind::Interface => write!(f, "interface"),
            SymbolKind::Impl => write!(f, "impl"),
            SymbolKind::Enum => write!(f, "enum"),
            SymbolKind::TypeAlias => write!(f, "type"),
        }
    }
}

fn detect_language(path: &std::path::Path) -> Language {
    match path.extension().and_then(|e| e.to_str()) {
        Some("ts") | Some("tsx") => Language::TypeScript,
        Some("js") | Some("jsx") | Some("mjs") => Language::JavaScript,
        Some("py") => Language::Python,
        Some("rs") => Language::Rust,
        Some("go") => Language::Go,
        _ => Language::Unknown,
    }
}

fn create_parser(lang: &Language) -> Option<Parser> {
    let mut parser = Parser::new();
    let language = match lang {
        Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
        Language::JavaScript => tree_sitter_javascript::LANGUAGE,
        Language::Python => tree_sitter_python::LANGUAGE,
        Language::Rust => tree_sitter_rust::LANGUAGE,
        Language::Go => tree_sitter_go::LANGUAGE,
        Language::Unknown => return None,
    };
    parser.set_language(&language.into()).ok()?;
    Some(parser)
}

fn extract_symbols(root: tree_sitter::Node, source: &[u8], lang: &Language) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        let kind = match (lang, child.kind()) {
            (_, "function_item" | "function_declaration") => Some(SymbolKind::Function),
            (_, "class_declaration" | "class_definition") => Some(SymbolKind::Class),
            (_, "struct_item") => Some(SymbolKind::Struct),
            (_, "interface_declaration") => Some(SymbolKind::Interface),
            (_, "impl_item") => Some(SymbolKind::Impl),
            (_, "enum_item") => Some(SymbolKind::Enum),
            (_, "type_item" | "type_alias_declaration") => Some(SymbolKind::TypeAlias),
            _ => None,
        };

        if let Some(kind) = kind {
            let name = child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("<anonymous>")
                .to_string();

            let is_public = match lang {
                Language::Rust => {
                    child.children(&mut child.walk()).any(|c| c.kind() == "visibility_modifier")
                }
                Language::Go => {
                    name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
                }
                Language::Python => !name.starts_with('_'),
                _ => true,
            };

            let children = extract_symbols(child, source, lang);

            symbols.push(Symbol {
                kind,
                name,
                line: child.start_position().row + 1,
                is_public,
                children,
            });
        }
    }

    symbols
}

pub fn scan(root: &str, lang_filter: Option<&str>) -> Result<Vec<SourceFile>> {
    let mut files = Vec::new();
    let allowed_langs: Option<Vec<&str>> = lang_filter.map(|l| l.split(',').collect());

    for entry in WalkBuilder::new(root).hidden(false).build() {
        let entry = entry?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let lang = detect_language(path);

        if lang == Language::Unknown {
            continue;
        }

        if let Some(ref langs) = allowed_langs {
            let lang_name = format!("{:?}", lang).to_lowercase();
            if !langs.iter().any(|l| l.trim().to_lowercase() == lang_name) {
                continue;
            }
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let line_count = content.lines().count();

        let symbols = if let Some(mut parser) = create_parser(&lang) {
            if let Some(tree) = parser.parse(&content, None) {
                extract_symbols(tree.root_node(), content.as_bytes(), &lang)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        files.push(SourceFile {
            path: path.to_path_buf(),
            line_count,
            language: lang,
            symbols,
        });
    }

    Ok(files)
}
