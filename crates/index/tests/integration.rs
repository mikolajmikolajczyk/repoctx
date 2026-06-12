use repoctx_index::{parse_file, Language};
use repoctx_store::SymbolRecord;

fn triples(rs: &[SymbolRecord]) -> Vec<(&str, &str, u32)> {
    rs.iter()
        .map(|r| (r.name.as_str(), r.kind.as_str(), r.start_line))
        .collect()
}

#[test]
fn rust_struct_trait_function_method() {
    let src = r#"
struct Cat { age: u32 }

trait Speak { fn speak(&self) -> String; }

impl Speak for Cat {
    fn speak(&self) -> String { "meow".into() }
}

fn main() {
    let c = Cat { age: 3 };
    println!("{}", c.speak());
}
"#;
    let got = parse_file("a.rs", Language::Rust, src).unwrap();
    assert_eq!(
        triples(&got),
        vec![
            ("Cat", "class", 1),
            ("Speak", "interface", 3),
            ("speak", "method", 6),
            ("main", "function", 9),
        ]
    );
}

#[test]
fn go_func_method_struct_interface() {
    let src = r#"package main

type Cat struct{ Age int }

type Animal interface{ Speak() string }

func (c Cat) Speak() string { return "meow" }

func main() { println("hi") }
"#;
    let got = parse_file("a.go", Language::Go, src).unwrap();
    assert_eq!(
        triples(&got),
        vec![
            ("Cat", "type", 2),
            ("Animal", "type", 4),
            ("Speak", "method", 6),
            ("main", "function", 8),
        ]
    );
}

#[test]
fn typescript_interface_method_abstract_class() {
    // Coverage from the vendored Aider tags.scm (Apache-2.0). Captures
    // interface, abstract class, and method signatures alongside the
    // plain-class / plain-function / arrow / type-alias / enum patterns
    // upstream tree-sitter-typescript misses.
    let src = r#"export interface Speak { speak(): string }

export abstract class AbstractCat { abstract meow(): string }
"#;
    let got = parse_file("a.ts", Language::TypeScript, src).unwrap();
    let names: Vec<_> = got
        .iter()
        .map(|r| (r.name.as_str(), r.kind.as_str()))
        .collect();
    assert!(names.contains(&("Speak", "interface")), "{names:?}");
    assert!(names.contains(&("speak", "method")), "{names:?}");
    assert!(names.contains(&("AbstractCat", "class")), "{names:?}");
    assert!(names.contains(&("meow", "method")), "{names:?}");
}

#[test]
fn typescript_plain_class_function_arrow_type_enum() {
    // The richer Aider-vendored tags.scm catches everything upstream
    // tree-sitter-typescript misses: plain class, concrete methods,
    // plain function, arrow function assigned to const, type alias,
    // enum.
    let src = r#"class UserCard {
  constructor() {}
  render() { return null; }
  private greet(): string { return "hi"; }
}

function plainFn() { return 1; }

const ArrowFn = (x: number) => x + 1;

type Theme = "light" | "dark";

enum Status { Active, Inactive }
"#;
    let got = parse_file("a.ts", Language::TypeScript, src).unwrap();
    let names: Vec<_> = got
        .iter()
        .map(|r| (r.name.as_str(), r.kind.as_str()))
        .collect();
    assert!(names.contains(&("UserCard", "class")), "{names:?}");
    assert!(names.contains(&("constructor", "method")), "{names:?}");
    assert!(names.contains(&("render", "method")), "{names:?}");
    assert!(names.contains(&("greet", "method")), "{names:?}");
    assert!(names.contains(&("plainFn", "function")), "{names:?}");
    assert!(names.contains(&("ArrowFn", "function")), "{names:?}");
    assert!(names.contains(&("Theme", "type")), "{names:?}");
    assert!(names.contains(&("Status", "enum")), "{names:?}");
}

#[test]
fn tsx_react_component_patterns() {
    // TSX shares the TypeScript tags.scm. React components written as
    // plain function, arrow function, or class all surface.
    let src = r#"interface Props { name: string }

class Card extends React.Component<Props> {
  render() { return <div />; }
}

function FunctionalCard({ name }: Props) {
  return <div>{name}</div>;
}

const ArrowCard = ({ name }: Props) => <div>{name}</div>;
"#;
    let got = parse_file("a.tsx", Language::Tsx, src).unwrap();
    let names: Vec<_> = got
        .iter()
        .map(|r| (r.name.as_str(), r.kind.as_str()))
        .collect();
    assert!(names.contains(&("Props", "interface")), "{names:?}");
    assert!(names.contains(&("Card", "class")), "{names:?}");
    assert!(names.contains(&("render", "method")), "{names:?}");
    assert!(names.contains(&("FunctionalCard", "function")), "{names:?}");
    assert!(names.contains(&("ArrowCard", "function")), "{names:?}");
}

#[test]
fn javascript_class_function() {
    let src = r#"class Cat { meow() { return "meow"; } }
function hello() { return "hi"; }
"#;
    let got = parse_file("a.js", Language::JavaScript, src).unwrap();
    let names: Vec<_> = got
        .iter()
        .map(|r| (r.name.as_str(), r.kind.as_str()))
        .collect();
    assert!(names.contains(&("Cat", "class")), "{names:?}");
    assert!(names.contains(&("hello", "function")), "{names:?}");
}

#[test]
fn python_def_class() {
    let src = r#"
class Cat:
    def speak(self):
        return "meow"

def hello():
    return "hi"
"#;
    let got = parse_file("a.py", Language::Python, src).unwrap();
    let names: Vec<_> = got
        .iter()
        .map(|r| (r.name.as_str(), r.kind.as_str()))
        .collect();
    assert!(names.contains(&("Cat", "class")), "{names:?}");
    assert!(
        names.iter().any(|(n, k)| *n == "hello" && *k == "function"),
        "{names:?}"
    );
}

#[test]
fn json_top_level_keys() {
    let src = r#"{
  "name": "repoctx",
  "version": 1,
  "nested": { "ignored": true }
}
"#;
    let got = parse_file("p.json", Language::Json, src).unwrap();
    let names: Vec<_> = got.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["name", "version", "nested"]);
    assert!(got.iter().all(|r| r.kind == "key"));
}

#[test]
fn yaml_top_level_keys_multi_doc() {
    let src = "
name: repoctx
version: 1
---
name: other
ignored:
  sub: true
";
    let got = parse_file("p.yaml", Language::Yaml, src).unwrap();
    let names: Vec<_> = got.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["name", "version", "name", "ignored"]);
    assert!(got.iter().all(|r| r.kind == "key"));
}

#[test]
fn yaml_non_mapping_root_is_empty() {
    let src = "- one\n- two\n";
    let got = parse_file("p.yaml", Language::Yaml, src).unwrap();
    assert!(got.is_empty(), "{got:?}");
}

#[test]
fn toml_root_pair_table_and_array() {
    let src = r#"
name = "repoctx"

[package]
version = "1"

[[bin]]
name = "x"
"#;
    let got = parse_file("p.toml", Language::Toml, src).unwrap();
    let names: Vec<_> = got.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["name", "package", "bin"]);
    assert!(got.iter().all(|r| r.kind == "key"));
}

#[test]
fn markdown_atx_and_setext() {
    let src = "# Title

Some text.

## Subsection

Underline H1
===========

Underline H2
-----------
";
    let got = parse_file("p.md", Language::Markdown, src).unwrap();
    let names: Vec<_> = got
        .iter()
        .map(|r| (r.name.as_str(), r.kind.as_str()))
        .collect();
    assert_eq!(
        names,
        vec![
            ("Title", "section"),
            ("Subsection", "section"),
            ("Underline H1", "section"),
            ("Underline H2", "section"),
        ]
    );
}

#[test]
fn unknown_extension_is_none() {
    use std::path::Path;
    assert!(Language::from_path(Path::new("foo.unknown")).is_none());
    assert_eq!(Language::from_path(Path::new("a.rs")), Some(Language::Rust));
    assert_eq!(Language::from_path(Path::new("a.tsx")), Some(Language::Tsx));
}
