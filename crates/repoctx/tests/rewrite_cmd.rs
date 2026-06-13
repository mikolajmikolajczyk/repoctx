//! `repoctx rewrite <cmd>` exit-code protocol (e06f463).

use assert_cmd::Command;

fn rewrite(cmd: &str) -> (i32, String) {
    let out = Command::cargo_bin("repoctx")
        .unwrap()
        .args(["rewrite", cmd])
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
    )
}

#[test]
fn rewrite_emits_command_exit_zero() {
    assert_eq!(
        rewrite("rg foo"),
        (0, "repoctx symbols foo --json".to_string())
    );
    assert_eq!(
        rewrite(r#"rg "fn parse_config""#),
        (0, "repoctx definition parse_config --json".to_string())
    );
}

#[test]
fn passthrough_exits_one_no_stdout() {
    let (code, out) = rewrite("ls -la");
    assert_eq!(code, 1);
    assert!(out.is_empty());

    let (code, out) = rewrite(r#"rg "TODO""#);
    assert_eq!(code, 1);
    assert!(out.is_empty());
}
