use std::{env, process::Command};

const EXPECTED: &str = include_str!("data/expected_01.txt");

#[test]
fn test_cli() {
    env::set_current_dir(env::current_dir().unwrap().join("tests")).unwrap();
    let args = vec![
        "-i",
        "-f",
        "../src",
        "-f",
        "./data",
        "-n",
        "a=b",
        "c=d",
        "e=f",
        "env",
        "postarg=1",
    ]
    .into_iter();
    let actual = Command::new("../target/debug/enw")
        .args(args)
        .output()
        .unwrap();
    assert!(actual.status.success());
    let stdout = String::from_utf8_lossy(&actual.stdout);
    assert_eq!(stdout, EXPECTED);
}
