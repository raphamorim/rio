#[cfg(target_os = "macos")]
pub mod macos;

/// Escape shell-sensitive characters in a string by prefixing each
/// with a backslash. Suitable for inserting paths into a live
/// terminal buffer, e.g. on file drag and drop.
pub fn shell_escape(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(
            c,
            '\\' | ' '
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '<'
                | '>'
                | '"'
                | '\''
                | '`'
                | '!'
                | '#'
                | '$'
                | '&'
                | ';'
                | '|'
                | '*'
                | '?'
                | '\t'
        ) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::shell_escape;

    #[test]
    fn plain_path_is_unchanged() {
        assert_eq!(shell_escape("/usr/local/bin/rio"), "/usr/local/bin/rio");
    }

    #[test]
    fn spaces_and_parens_are_escaped() {
        assert_eq!(
            shell_escape("/Users/me/My File (1).txt"),
            "/Users/me/My\\ File\\ \\(1\\).txt"
        );
    }

    #[test]
    fn shell_metacharacters_are_escaped() {
        assert_eq!(
            shell_escape("a$b`c\"d'e;f&g|h*i?j!k#l"),
            "a\\$b\\`c\\\"d\\'e\\;f\\&g\\|h\\*i\\?j\\!k\\#l"
        );
    }

    #[test]
    fn backslash_is_escaped_without_double_escaping() {
        assert_eq!(shell_escape("a\\ b"), "a\\\\\\ b");
    }

    #[test]
    fn brackets_braces_and_redirects_are_escaped() {
        assert_eq!(
            shell_escape("x[1]{2}<3>4\t5"),
            "x\\[1\\]\\{2\\}\\<3\\>4\\\t5"
        );
    }

    #[test]
    fn unicode_passes_through() {
        assert_eq!(shell_escape("/tmp/café 図.png"), "/tmp/café\\ 図.png");
    }
}
