use isolated_lab_the_gang_uses_servo_too_much::select;

const SRC: &str = r#"
fn spawn() {
    std::process::Command::new("tmux");
}

#[cfg(test)]
mod tests {
    fn t() {
        foo();
        Command::new("git");
    }
}
"#;

fn run(css: &str) -> Vec<(&'static str, String)> {
    select(SRC, css)
        .unwrap()
        .into_iter()
        .map(|h| (if h.kind == "call_expression" { "call" } else { "ident" }, h.text))
        .collect()
}

#[test]
fn selectors() {
    let got = [
        "call_expression",
        r#"attribute_item:text(*= "cfg(test)") + mod_item call_expression"#,
        r#"call_expression:not(attribute_item:text(*= "cfg(test)") + mod_item *)"#,
        r#"call_expression:has(> scoped_identifier:text($= "Command::new"))"#,
        "call_expression > identifier[field=function]",
    ]
    .map(run);
    let tmux = ("call", r#"std::process::Command::new("tmux")"#.to_string());
    let foo = ("call", "foo()".to_string());
    let git = ("call", r#"Command::new("git")"#.to_string());
    assert_eq!(
        got,
        [
            vec![tmux.clone(), foo.clone(), git.clone()],
            vec![foo, git.clone()],
            vec![tmux.clone()],
            vec![tmux, git],
            vec![("ident", "foo".to_string())],
        ]
    );
}
