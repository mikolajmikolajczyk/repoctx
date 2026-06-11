/// Escape a user-supplied substring for use inside an SQL `LIKE` pattern.
///
/// Escape character is `\`. Caller must add `ESCAPE '\'` to the query.
pub fn escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '\\' | '%' | '_' => {
                out.push('\\');
                out.push(c);
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::escape;

    #[test]
    fn escapes_wildcards_and_backslash() {
        assert_eq!(escape("foo"), "foo");
        assert_eq!(escape("100%"), "100\\%");
        assert_eq!(escape("snake_case"), "snake\\_case");
        assert_eq!(escape("a\\b"), "a\\\\b");
        assert_eq!(escape("%_\\"), "\\%\\_\\\\");
    }
}
