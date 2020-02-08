use std::process::Command;

const EXPECTED: &str = r#"
MY_URL=https://xyzzy:xyzzy@localhost:80/xyzzy?abc=def#fragment
a=b
c=d
e=f
postarg=1
"#;

#[test]
fn test_cli() {
    let args = vec![
        "-i",
        "-f",
        "./tests",
        "-f",
        "./tests/data",
        "a=b",
        "c=d",
        "e=f",
        "env",
        "postarg=1",
    ]
    .into_iter();
    let actual = Command::new("./target/debug/enw")
        .args(args)
        .output()
        .unwrap();
    assert!(actual.status.success());
    let stdout = String::from_utf8_lossy(&actual.stdout);
    assert_eq!(stdout, EXPECTED[1..]);
}
