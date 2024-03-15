use std::{env, path::Path, process::Command};

use pretty_assertions::assert_eq;

const EXPECTED: &str = include_str!("data/expected_with_command.txt");
const EXPECTED_NO_COMMAND: &str = include_str!("data/expected_no_command.txt");

const EXPECTED_STDERR_WITH_FILE_NOT_FOUND: &str =
    include_str!("data/stderr_with_file_not_found.txt");
const EXPECTED_STDERR_WITH_NO_DOT_ENV_IN_DIRECTORY: &str =
    include_str!("data/stderr_with_no_dot_env_in_directory.txt");

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
            "postarg=1",
        ]
        .into_iter();
        let actual = Command::new("../target/debug/enw").args(args).output()?;
        assert!(actual.status.success());
        let stdout = String::from_utf8_lossy(&actual.stdout);
        assert_eq!(stdout, EXPECTED);
        Ok(())
    })?;

    in_directory(&env::current_dir()?.join("tests"), || {
        let args = vec![
            "-i", "-f", "../src", "-f", "./data", "-n", "a=b", "c=d", "e=f",
        ]
        .into_iter();
        let actual = Command::new("../target/debug/enw").args(args).output()?;
        assert!(actual.status.success());
        let stdout = String::from_utf8_lossy(&actual.stdout);
        assert_eq!(stdout, EXPECTED_NO_COMMAND);
        Ok(())
    })?;

    // Test roundtrip of a file that should produce identical output
    in_directory(&env::current_dir()?.join("tests"), || {
        let args = vec![
            "-i", "-f", "./data/expected_no_command.txt", "-n",
        ]
        .into_iter();
        let actual = Command::new("../target/debug/enw").args(args).output()?;
        assert!(actual.status.success());
        let stdout = String::from_utf8_lossy(&actual.stdout);
        assert_eq!(stdout, EXPECTED_NO_COMMAND);
        Ok(())
    })?;


    in_directory(&env::current_dir()?.join("tests"), || {
        let args = vec!["-f", "not_found.env"];
        let actual = Command::new("../target/debug/enw").args(args).output()?;
        assert!(actual.status.success());
        let stderr = String::from_utf8_lossy(&actual.stderr);
        assert_eq!(
            stderr, EXPECTED_STDERR_WITH_FILE_NOT_FOUND,
            "When specified file can not be found"
        );
        Ok(())
    })?;

    in_directory(&env::current_dir()?.join("tests"), || {
        let args = vec!["-f", "./data/not_found"];
        let actual = Command::new("../target/debug/enw").args(args).output()?;
        assert!(actual.status.success());
        let stderr = String::from_utf8_lossy(&actual.stderr);
        assert_eq!(
            stderr, EXPECTED_STDERR_WITH_NO_DOT_ENV_IN_DIRECTORY,
            "When default env file is not found in directory"
        );
        Ok(())
    })?;

    in_directory(&env::current_dir()?.join("tests/data/not_found"), || {
        let actual = Command::new("../../../target/debug/enw").output()?;
        assert!(actual.status.success());
        let stderr = String::from_utf8_lossy(&actual.stderr);
        assert_eq!(stderr, "", "When the file not found is the default file");
        Ok(())
    })?;

    // Test escaping of output
    in_directory(&env::current_dir()?.join("tests"), || {
        let args = vec!["-i", "-n", "-f", "./data/input_01.txt"].into_iter();
        let actual = Command::new("../target/debug/enw").args(args).output()?;
        assert!(actual.status.success());
        let stdout = String::from_utf8_lossy(&actual.stdout);
        assert_eq!(stdout, std::fs::read_to_string("data/roundtrip_expected_output_for_input_01.txt")?);
        Ok(())
    })?;

    Ok(())
}

fn in_directory<F>(path: &Path, thunk: F) -> Result<(), BoxError>
where
    F: FnOnce() -> Result<(), BoxError>,
{
    let current_dir = env::current_dir()?;
    env::set_current_dir(path)?;
    let result = thunk();
    env::set_current_dir(current_dir)?;
    result
}
