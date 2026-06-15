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

#[derive(Debug, Clone, PartialEq)]
pub enum SymbolKind {
    Function,
    Class,
    Struct,
    Interface,
    Impl,
    Enum,
    TypeAlias,
    Field,
    Method,
    Trait,
    Macro,
    Constant,
    Module,
    Variable,
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
            SymbolKind::Field => write!(f, "field"),
            SymbolKind::Method => write!(f, "method"),
            SymbolKind::Trait => write!(f, "trait"),
            SymbolKind::Macro => write!(f, "macro"),
            SymbolKind::Constant => write!(f, "const"),
            SymbolKind::Module => write!(f, "module"),
            SymbolKind::Variable => write!(f, "variable"),
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

fn is_method_context(lang: &Language, parent_kind: &str) -> bool {
    matches!(
        (lang, parent_kind),
        (Language::Rust, "impl_item" | "trait_item")
            | (Language::TypeScript | Language::JavaScript, "class_declaration" | "class_body")
            | (Language::Python, "class_definition")
    )
}

fn is_container(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Class
            | SymbolKind::Struct
            | SymbolKind::Enum
            | SymbolKind::Impl
            | SymbolKind::Trait
            | SymbolKind::Interface
            | SymbolKind::Module
    )
}

fn extract_name(child: tree_sitter::Node, source: &[u8], lang: &Language) -> String {
    let candidates: &[&str] = match (lang, child.kind()) {
        (Language::Rust, "impl_item") => &["type", "trait", "name"],
        (Language::TypeScript | Language::JavaScript, "class_declaration" | "interface_declaration" | "type_alias_declaration" | "enum_declaration") => {
            &["name", "type_identifier"]
        }
        (Language::TypeScript | Language::JavaScript, "method_definition" | "method_signature" | "property_signature" | "property_definition") => {
            &["name", "property_identifier"]
        }
        (Language::Go, "method_declaration" | "function_declaration") => &["name", "field_identifier"],
        (Language::Python, _) => &["name"],
        _ => &["name"],
    };

    for field in candidates {
        if let Some(name) = child
            .child_by_field_name(field)
            .and_then(|n| n.utf8_text(source).ok())
        {
            return name.to_string();
        }
    }

    // Fallback: look for the first identifier-like child.
    let identifier_kinds = match lang {
        Language::TypeScript | Language::JavaScript => &["type_identifier", "property_identifier", "identifier"][..],
        Language::Go => &["field_identifier", "identifier"][..],
        _ => &["identifier"][..],
    };
    let mut cursor = child.walk();
    for grandchild in child.children(&mut cursor) {
        if identifier_kinds.contains(&grandchild.kind())
            && let Ok(text) = grandchild.utf8_text(source)
        {
            return text.to_string();
        }
    }

    "<anonymous>".to_string()
}

fn extract_symbols(
    root: tree_sitter::Node,
    source: &[u8],
    lang: &Language,
    parent_kind: &str,
) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        let kind = match (lang, child.kind()) {
            // Functions & methods
            (_, "function_item" | "function_declaration" | "function_signature_item") => {
                if is_method_context(lang, parent_kind) {
                    Some(SymbolKind::Method)
                } else {
                    Some(SymbolKind::Function)
                }
            }
            (Language::TypeScript | Language::JavaScript, "method_definition") => Some(SymbolKind::Method),
            (Language::Python, "function_definition") if parent_kind == "class_definition" => {
                Some(SymbolKind::Method)
            }
            (Language::Python, "function_definition") => Some(SymbolKind::Function),
            (Language::Go, "method_declaration") => Some(SymbolKind::Method),

            // Types & classes
            (_, "class_declaration" | "class_definition") => Some(SymbolKind::Class),
            (Language::Rust, "struct_item") => Some(SymbolKind::Struct),
            (Language::Go, "struct_type") if root.kind() == "type_declaration" || root.kind() == "type_spec" => {
                Some(SymbolKind::Struct)
            }
            (Language::Rust, "interface_item") => Some(SymbolKind::Interface),
            (Language::TypeScript | Language::JavaScript, "interface_declaration") => {
                Some(SymbolKind::Interface)
            }
            (Language::Go, "interface_type") => Some(SymbolKind::Interface),
            (Language::Rust, "impl_item") => Some(SymbolKind::Impl),
            (Language::Rust, "trait_item") => Some(SymbolKind::Trait),
            (Language::TypeScript | Language::JavaScript, "enum_declaration") => Some(SymbolKind::Enum),
            (Language::Rust, "enum_item") => Some(SymbolKind::Enum),
            (Language::TypeScript | Language::JavaScript, "type_alias_declaration") => {
                Some(SymbolKind::TypeAlias)
            }
            (Language::Rust, "type_item") => Some(SymbolKind::TypeAlias),

            // Rust-specific
            (Language::Rust, "field_declaration") => Some(SymbolKind::Field),
            (Language::Rust, "macro_rules_definition" | "macro_definition") => Some(SymbolKind::Macro),
            (Language::Rust, "const_item" | "static_item") => Some(SymbolKind::Constant),
            (Language::Rust, "mod_item") => Some(SymbolKind::Module),

            // TypeScript/JavaScript-specific
            (Language::TypeScript | Language::JavaScript, "property_signature" | "property_definition") => {
                Some(SymbolKind::Field)
            }
            (Language::TypeScript | Language::JavaScript, "method_signature") => Some(SymbolKind::Method),
            (Language::TypeScript | Language::JavaScript, "variable_declarator") => {
                Some(SymbolKind::Variable)
            }

            // Go-specific
            (Language::Go, "const_declaration" | "var_declaration") => Some(SymbolKind::Constant),

            _ => None,
        };

        if let Some(kind) = kind {
            let name = extract_name(child, source, lang);

            let is_public = match lang {
                Language::Rust => {
                    child.children(&mut child.walk()).any(|c| c.kind() == "visibility_modifier")
                }
                Language::Go => name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false),
                Language::Python => !name.starts_with('_'),
                _ => true,
            };

            let children = if is_container(&kind) {
                extract_all_symbols(child, source, lang, child.kind())
            } else {
                extract_symbols(child, source, lang, child.kind())
            };

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

fn extract_all_symbols(
    root: tree_sitter::Node,
    source: &[u8],
    lang: &Language,
    parent_kind: &str,
) -> Vec<Symbol> {
    let mut symbols = extract_symbols(root, source, lang, parent_kind);
    let mut cursor = root.walk();

    // Also recurse into non-symbol container children (e.g. field_declaration_list,
    // class_body, declaration_list) to find nested fields/methods.
    // Keep `parent_kind` so that methods inside an impl block know they belong to it.
    for child in root.children(&mut cursor) {
        let is_symbol_container = matches!(
            child.kind(),
            "field_declaration_list"
                | "class_body"
                | "declaration_list"
                | "enum_variant_list"
                | "interface_body"
                | "block"
                | "export_statement"
                | "decorated_definition"
                | "statement_block"
        );
        if is_symbol_container {
            symbols.extend(extract_all_symbols(child, source, lang, parent_kind));
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
                extract_all_symbols(tree.root_node(), content.as_bytes(), &lang, "source_file")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_rust(source: &str) -> Vec<Symbol> {
        let mut parser = create_parser(&Language::Rust).expect("rust parser");
        let tree = parser.parse(source, None).expect("parse");
        extract_all_symbols(tree.root_node(), source.as_bytes(), &Language::Rust, "source_file")
    }

    fn parse_ts(source: &str) -> Vec<Symbol> {
        let mut parser = create_parser(&Language::TypeScript).expect("typescript parser");
        let tree = parser.parse(source, None).expect("parse");
        extract_all_symbols(tree.root_node(), source.as_bytes(), &Language::TypeScript, "source_file")
    }

    fn parse_python(source: &str) -> Vec<Symbol> {
        let mut parser = create_parser(&Language::Python).expect("python parser");
        let tree = parser.parse(source, None).expect("parse");
        extract_all_symbols(tree.root_node(), source.as_bytes(), &Language::Python, "source_file")
    }

    fn parse_go(source: &str) -> Vec<Symbol> {
        let mut parser = create_parser(&Language::Go).expect("go parser");
        let tree = parser.parse(source, None).expect("parse");
        extract_all_symbols(tree.root_node(), source.as_bytes(), &Language::Go, "source_file")
    }

    fn find_symbol<'a>(symbols: &'a [Symbol], name: &str) -> Option<&'a Symbol> {
        symbols.iter().find(|s| s.name == name)
    }

    #[test]
    fn rust_extracts_struct_function_and_impl_methods() {
        let source = r#"
pub struct User {
    name: String,
}

impl User {
    pub fn new(name: &str) -> Self {
        User { name: name.to_string() }
    }

    fn secret(&self) {}
}

fn helper() {}
"#;
        let symbols = parse_rust(source);
        let user = find_symbol(&symbols, "User").expect("User struct");
        assert_eq!(user.kind, SymbolKind::Struct);
        assert!(user.is_public);
        assert_eq!(user.children.len(), 1); // field

        let user_impl = find_symbol(&symbols, "User").and_then(|_| {
            symbols.iter().find(|s| s.kind == SymbolKind::Impl && s.name == "User")
        });
        assert!(user_impl.is_some(), "impl User should be extracted");
        let impl_methods = &user_impl.unwrap().children;
        assert_eq!(impl_methods.len(), 2);
        assert!(impl_methods.iter().any(|m| m.name == "new" && m.kind == SymbolKind::Method));
        assert!(impl_methods.iter().any(|m| m.name == "secret" && m.kind == SymbolKind::Method));

        let helper = find_symbol(&symbols, "helper").expect("helper function");
        assert_eq!(helper.kind, SymbolKind::Function);
        assert!(!helper.is_public);
    }

    #[test]
    fn rust_extracts_trait_and_enum() {
        let source = r#"
pub trait Greeter {
    fn greet(&self);
}

pub enum Color {
    Red,
    Green,
}
"#;
        let symbols = parse_rust(source);
        let greeter = find_symbol(&symbols, "Greeter").expect("Greeter trait");
        assert_eq!(greeter.kind, SymbolKind::Trait);
        assert_eq!(greeter.children.len(), 1);
        assert_eq!(greeter.children[0].name, "greet");
        assert_eq!(greeter.children[0].kind, SymbolKind::Method);

        let color = find_symbol(&symbols, "Color").expect("Color enum");
        assert_eq!(color.kind, SymbolKind::Enum);
    }

    #[test]
    fn rust_extracts_macro_and_module() {
        let source = r#"
macro_rules! say_hello {
    () => { println!("hello") };
}

mod inner {
    pub fn work() {}
}
"#;
        let symbols = parse_rust(source);
        assert!(find_symbol(&symbols, "say_hello").is_some());
        assert_eq!(find_symbol(&symbols, "say_hello").unwrap().kind, SymbolKind::Macro);

        let inner = find_symbol(&symbols, "inner").expect("inner module");
        assert_eq!(inner.kind, SymbolKind::Module);
        assert_eq!(inner.children.len(), 1);
        assert_eq!(inner.children[0].name, "work");
    }

    #[test]
    fn typescript_extracts_class_interface_and_methods() {
        let source = r#"
export class User {
    constructor(public name: string) {}

    greet(): string {
        return "hi";
    }
}

interface Named {
    name: string;
}
"#;
        let symbols = parse_ts(source);
        let user = find_symbol(&symbols, "User").expect("User class");
        assert_eq!(user.kind, SymbolKind::Class);
        assert!(user.children.iter().any(|c| c.name == "greet" && c.kind == SymbolKind::Method));

        let named = find_symbol(&symbols, "Named").expect("Named interface");
        assert_eq!(named.kind, SymbolKind::Interface);
        assert!(named.children.iter().any(|c| c.name == "name" && c.kind == SymbolKind::Field));
    }

    #[test]
    fn python_extracts_class_and_methods() {
        let source = r#"
class User:
    def __init__(self, name):
        self.name = name

    def greet(self):
        return f"hello {self.name}"

def helper():
    pass
"#;
        let symbols = parse_python(source);
        let user = find_symbol(&symbols, "User").expect("User class");
        assert_eq!(user.kind, SymbolKind::Class);
        assert!(user.children.iter().any(|c| c.name == "__init__" && c.kind == SymbolKind::Method));
        assert!(user.children.iter().any(|c| c.name == "greet" && c.kind == SymbolKind::Method));

        let helper = find_symbol(&symbols, "helper").expect("helper function");
        assert_eq!(helper.kind, SymbolKind::Function);
    }

    #[test]
    fn go_extracts_functions_structs_and_methods() {
        let source = r#"
package main

type User struct {
    Name string
}

func NewUser(name string) *User {
    return &User{Name: name}
}

func (u *User) Greet() string {
    return "hello"
}
"#;
        let symbols = parse_go(source);
        let new_user = find_symbol(&symbols, "NewUser").expect("NewUser function");
        assert_eq!(new_user.kind, SymbolKind::Function);
        assert!(new_user.is_public);

        let greet = find_symbol(&symbols, "Greet").expect("Greet method");
        assert_eq!(greet.kind, SymbolKind::Method);
        assert!(greet.is_public);
    }

    #[test]
    fn language_detection_works() {
        use std::path::Path;
        assert_eq!(detect_language(Path::new("foo.rs")), Language::Rust);
        assert_eq!(detect_language(Path::new("foo.ts")), Language::TypeScript);
        assert_eq!(detect_language(Path::new("foo.tsx")), Language::TypeScript);
        assert_eq!(detect_language(Path::new("foo.py")), Language::Python);
        assert_eq!(detect_language(Path::new("foo.go")), Language::Go);
        assert_eq!(detect_language(Path::new("foo.jsx")), Language::JavaScript);
        assert_eq!(detect_language(Path::new("foo.txt")), Language::Unknown);
    }
}
