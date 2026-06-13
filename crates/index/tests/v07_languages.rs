//! Extraction smoke tests for the v0.7.0 language batch (epic 9cf4c18).
//! Asserts each grammar parses + the tags query surfaces the expected
//! named symbols. Kinds/lines vary across grammar versions, so we assert
//! on (name, kind) presence rather than exact triples.

use repoctx_index::{parse_file, Language};
use repoctx_store::SymbolRecord;

fn has(rs: &[SymbolRecord], name: &str, kind: &str) -> bool {
    rs.iter().any(|r| r.name == name && r.kind == kind)
}

fn names(rs: &[SymbolRecord]) -> Vec<&str> {
    rs.iter().map(|r| r.name.as_str()).collect()
}

#[test]
fn ruby_module_class_method() {
    let src = "module M\n  class Animal\n    def speak\n    end\n  end\nend\n";
    let got = parse_file("a.rb", Language::Ruby, src).unwrap();
    assert!(has(&got, "Animal", "class"), "{:?}", names(&got));
    assert!(has(&got, "speak", "method"), "{:?}", names(&got));
}

#[test]
fn c_function_struct() {
    let src = "int add(int a, int b) { return a + b; }\nstruct Point { int x; };\n";
    let got = parse_file("a.c", Language::C, src).unwrap();
    assert!(has(&got, "add", "function"), "{:?}", names(&got));
    assert!(names(&got).contains(&"Point"), "{:?}", names(&got));
}

#[test]
fn cpp_class_function() {
    let src = "class Widget {\npublic:\n  void build();\n};\nint helper() { return 0; }\n";
    let got = parse_file("a.cpp", Language::Cpp, src).unwrap();
    assert!(has(&got, "Widget", "class"), "{:?}", names(&got));
    assert!(has(&got, "helper", "function"), "{:?}", names(&got));
}

#[test]
fn bash_function() {
    let src = "function deploy() {\n  echo hi\n}\nbuild() { :; }\n";
    let got = parse_file("a.sh", Language::Bash, src).unwrap();
    assert!(has(&got, "deploy", "function"), "{:?}", names(&got));
    assert!(has(&got, "build", "function"), "{:?}", names(&got));
}

#[test]
fn java_class_method() {
    let src = "public class Server {\n  void start() {}\n}\n";
    let got = parse_file("A.java", Language::Java, src).unwrap();
    assert!(has(&got, "Server", "class"), "{:?}", names(&got));
    assert!(names(&got).contains(&"start"), "{:?}", names(&got));
}

#[test]
fn csharp_class_method() {
    let src = "public class Svc {\n  public void Run() {}\n}\n";
    let got = parse_file("a.cs", Language::CSharp, src).unwrap();
    assert!(has(&got, "Svc", "class"), "{:?}", names(&got));
    assert!(names(&got).contains(&"Run"), "{:?}", names(&got));
}

#[test]
fn php_function_class() {
    let src = "<?php\nfunction handler() {}\nclass UserService {}\n";
    let got = parse_file("a.php", Language::Php, src).unwrap();
    assert!(has(&got, "handler", "function"), "{:?}", names(&got));
    assert!(has(&got, "UserService", "class"), "{:?}", names(&got));
}

#[test]
fn lua_functions() {
    let src = "function greet(name)\n  return name\nend\nlocal function helper() end\n";
    let got = parse_file("a.lua", Language::Lua, src).unwrap();
    assert!(has(&got, "greet", "function"), "{:?}", names(&got));
    assert!(has(&got, "helper", "function"), "{:?}", names(&got));
}

#[test]
fn kotlin_class_object_function() {
    let src = "class Server {\n  fun start() {}\n}\nobject Config\nfun main() {}\n";
    let got = parse_file("a.kt", Language::Kotlin, src).unwrap();
    assert!(has(&got, "Server", "class"), "{:?}", names(&got));
    assert!(has(&got, "Config", "class"), "{:?}", names(&got));
    assert!(has(&got, "main", "function"), "{:?}", names(&got));
}

#[test]
fn swift_struct_function_method() {
    // tree-sitter-swift's tags surface struct / function / method; class
    // names are not reliably captured by the community grammar.
    let src = "struct Point {}\nfunc run() {}\nclass Widget {\n  func build() {}\n}\n";
    let got = parse_file("a.swift", Language::Swift, src).unwrap();
    assert!(names(&got).contains(&"Point"), "{:?}", names(&got));
    assert!(names(&got).contains(&"run"), "{:?}", names(&got));
    assert!(names(&got).contains(&"build"), "{:?}", names(&got));
}

#[test]
fn extension_mapping_covers_batch() {
    use std::path::Path;
    let cases = [
        ("a.rb", Language::Ruby),
        ("a.c", Language::C),
        ("a.h", Language::C),
        ("a.cpp", Language::Cpp),
        ("a.hpp", Language::Cpp),
        ("a.sh", Language::Bash),
        ("A.java", Language::Java),
        ("a.cs", Language::CSharp),
        ("a.php", Language::Php),
        ("a.lua", Language::Lua),
        ("a.kt", Language::Kotlin),
        ("a.swift", Language::Swift),
    ];
    for (path, want) in cases {
        assert_eq!(Language::from_path(Path::new(path)), Some(want), "{path}");
    }
}
