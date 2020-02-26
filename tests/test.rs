use std::{env, path::Path, process::Command};

const EXPECTED: &str = include_str!("data/expected_with_command.txt");
const EXPECTED_NO_COMMAND: &str = include_str!("data/expected_no_command.txt");

type BoxError = Box<dyn std::error::Error>;

#[test]
fn test_cli() -> Result<(), BoxError> {
    in_directory(&env::current_dir()?.join("tests"), || {
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
            "postarg=1"
        ].into_iter();
        let actual = Command::new("../target/debug/enw")
            .args(args)
            .output()?;
        assert!(actual.status.success());
        let stdout = String::from_utf8_lossy(&actual.stdout);
        assert_eq!(stdout, EXPECTED);
        Ok(())
    })?;

    in_directory(&env::current_dir()?.join("tests"), || {
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
        ].into_iter();
        let actual = Command::new("../target/debug/enw")
            .args(args)
            .output()?;
        assert!(actual.status.success());
        let stdout = String::from_utf8_lossy(&actual.stdout);
        assert_eq!(stdout, EXPECTED_NO_COMMAND);
        Ok(())
    })
}

fn in_directory<F>(path: &Path, thunk: F) -> Result<(), BoxError>
    where F: FnOnce() -> Result<(), BoxError>,
{
    let current_dir = env::current_dir()?;
    env::set_current_dir(path)?;
    let result = thunk();
    env::set_current_dir(current_dir)?;
    result
}
