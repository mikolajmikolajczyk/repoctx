//! Call-site extraction tests (static call graph, epic af42572 / ADR-0010).

use repoctx_index::{parse_calls_with, parse_file, Language};

/// Extract (caller_name, callee_name) edges for a source snippet.
fn edges(path: &str, lang: Language, src: &str) -> Vec<(String, String)> {
    let symbols = parse_file(path, lang, src).unwrap();
    let calls = parse_calls_with(path, lang, src, &symbols).unwrap();
    let mut out: Vec<(String, String)> = calls
        .into_iter()
        .map(|c| (c.caller_name, c.callee_name))
        .collect();
    out.sort();
    out
}

fn has(edges: &[(String, String)], caller: &str, callee: &str) -> bool {
    edges.iter().any(|(a, b)| a == caller && b == callee)
}

#[test]
fn rust_function_method_and_macro_calls() {
    let src = r#"
fn main() {
    helper();
    thing.run();
    println!("hi");
}

fn helper() {
    nested();
}
"#;
    let e = edges("a.rs", Language::Rust, src);
    assert!(has(&e, "main", "helper"), "{e:?}");
    assert!(has(&e, "main", "run"), "method call: {e:?}");
    assert!(has(&e, "main", "println"), "macro call: {e:?}");
    assert!(has(&e, "helper", "nested"), "{e:?}");
    // No edge attributed to a non-existent caller.
    assert!(!has(&e, "helper", "helper"));
}

#[test]
fn python_function_and_attribute_calls() {
    let src = "
def main():
    helper()
    obj.method()

def helper():
    pass
";
    let e = edges("a.py", Language::Python, src);
    assert!(has(&e, "main", "helper"), "{e:?}");
    assert!(has(&e, "main", "method"), "{e:?}");
}

#[test]
fn go_function_and_selector_calls() {
    let src = r#"
package main

func main() {
    helper()
    pkg.Do()
}

func helper() {}
"#;
    let e = edges("a.go", Language::Go, src);
    assert!(has(&e, "main", "helper"), "{e:?}");
    assert!(has(&e, "main", "Do"), "{e:?}");
}

#[test]
fn java_method_invocations() {
    let src = r#"
class App {
    void main() {
        helper();
        obj.run();
    }
    void helper() {}
}
"#;
    let e = edges("App.java", Language::Java, src);
    assert!(has(&e, "main", "helper"), "{e:?}");
    assert!(has(&e, "main", "run"), "{e:?}");
}

#[test]
fn c_function_calls() {
    let src = r#"
int helper(void) { return 0; }

int main(void) {
    helper();
    return 0;
}
"#;
    let e = edges("a.c", Language::C, src);
    assert!(has(&e, "main", "helper"), "{e:?}");
}

#[test]
fn cpp_function_and_qualified_calls() {
    let src = r#"
int helper() { return 0; }

int main() {
    helper();
    ns::run();
    return 0;
}
"#;
    let e = edges("a.cpp", Language::Cpp, src);
    assert!(has(&e, "main", "helper"), "{e:?}");
    assert!(has(&e, "main", "run"), "qualified call: {e:?}");
}

#[test]
fn typescript_function_and_member_calls() {
    let src = r#"
function helper(): void {}

function main(): void {
    helper();
    obj.run();
}
"#;
    let e = edges("a.ts", Language::TypeScript, src);
    assert!(has(&e, "main", "helper"), "{e:?}");
    assert!(has(&e, "main", "run"), "{e:?}");
}

#[test]
fn javascript_function_and_member_calls() {
    let src = r#"
function helper() {}

function main() {
    helper();
    obj.run();
}
"#;
    let e = edges("a.js", Language::JavaScript, src);
    assert!(has(&e, "main", "helper"), "{e:?}");
    assert!(has(&e, "main", "run"), "{e:?}");
}

#[test]
fn top_level_calls_have_no_caller() {
    // A call outside any function/method is dropped (no caller to attribute).
    let src = "helper()\n";
    let e = edges("a.py", Language::Python, src);
    assert!(e.is_empty(), "top-level call should be dropped: {e:?}");
}

#[test]
fn non_core_language_yields_no_edges() {
    // Ruby has no call query yet (follow-up child) -> empty, no error.
    let src = "def main\n  helper\nend\n";
    let symbols = parse_file("a.rb", Language::Ruby, src).unwrap();
    let calls = parse_calls_with("a.rb", Language::Ruby, src, &symbols).unwrap();
    assert!(calls.is_empty());
}
