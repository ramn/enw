use std::{env, ffi::OsString, fs, path::PathBuf, process::Command};

use clap::{App, Arg};

pub type BoxError = Box<dyn std::error::Error>;

const ABOUT: &str =
    "Similar to the GNU env command, but will automatically load an .env file, if found.";
const USAGE: &str = "enw [OPTION]... [-] [NAME=VALUE] [COMMAND [ARGS]...]";

#[derive(Debug, Default)]
struct OptionsBuilder<'a> {
    env_files: Vec<PathBuf>,
    vars: Vec<(&'a str, &'a str)>,
    command: &'a str,
    args: Vec<&'a str>,
    ignore_env: bool,
}

pub fn run(args: impl Iterator<Item = impl Into<OsString> + Clone>) -> Result<(), BoxError> {
    let matches = App::new("enw")
        .about(ABOUT)
        .usage(USAGE)
        .arg(
            Arg::with_name("env_file")
                .short("f")
                .long("file")
                .value_name("FILE")
                .help(".env file")
                .takes_value(true)
                .multiple(true)
                .number_of_values(1),
        )
        .arg(
            Arg::with_name("ignore_env")
                .short("i")
                .long("ignore_env")
                .help("start with an empty environment"),
        )
        .arg(
            Arg::with_name("rest")
                .value_name("REST")
                .takes_value(true)
                .hidden(true)
                .multiple(true),
        )
        .get_matches_from(args);

    let mut opt_builder = OptionsBuilder::default();
    opt_builder.ignore_env = matches.is_present("ignore_env");
    opt_builder.env_files.push(env::current_dir()?.join(".env"));
    opt_builder.env_files = matches
        .values_of_lossy("env_file")
        .unwrap_or_else(|| vec![])
        .iter()
        .map(|fname| fname.into())
        .collect();
    let rest = matches.values_of_lossy("rest").unwrap_or_else(|| vec![]);
    opt_builder.vars = rest
        .iter()
        .take_while(|x| x.contains('='))
        .map(|line| parse_env_line(&line))
        .collect::<Result<_, _>>()?;
    opt_builder.command = rest
        .get(opt_builder.vars.len())
        .ok_or_else(|| "No COMMAND supplied")?;
    opt_builder.args = rest
        .iter()
        .skip(opt_builder.vars.len() + 1)
        .map(|x| &**x)
        .collect();

    let env_files: Vec<_> = opt_builder
        .env_files
        .into_iter()
        .filter(|p| p.exists())
        .map(|path| {
            if path.is_dir() {
                path.join(".env")
            } else {
                path
            }
        })
        .filter(|p| p.is_file())
        .map(fs::read_to_string)
        .collect::<Result<_, _>>()?;
    let mut env_vars: Vec<_> = env_files
        .iter()
        .flat_map(|text| parse_env_file(&text))
        .collect::<Result<_, _>>()?;
    env_vars.extend(opt_builder.vars.into_iter());

    let mut cmd = Command::new(opt_builder.command);
    if opt_builder.ignore_env {
        cmd.env_clear();
    }
    cmd.envs(env_vars).args(opt_builder.args);
    let mut child = cmd.spawn()?;
    child.wait()?;
    Ok(())
}

fn parse_env_file(text: &str) -> impl Iterator<Item = Result<(&str, &str), BoxError>> {
    text.lines()
        .map(|line| line.trim())
        .filter(|line| line.contains('=') && !line.starts_with('#'))
        .map(parse_env_line)
}

fn parse_env_line(line: &str) -> Result<(&str, &str), BoxError> {
    let mut parts = line.splitn(2, '=').map(str::trim);
    let key = parts.next().ok_or("KEY missing")?;
    let mut value = parts.next().ok_or("VALUE missing")?;
    if let (_, Some(hash_ix)) =
        value
            .chars()
            .enumerate()
            .fold((false, None), |(in_quote, hash_ix), (i, c)| match c {
                '"' => (!in_quote, hash_ix),
                '#' => (in_quote, if in_quote { hash_ix } else { Some(i) }),
                _ => (in_quote, hash_ix),
            })
    {
        value = &value[0..hash_ix].trim();
    }
    Ok((key, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_env_line() {
        let actual = parse_env_line(
            r#" MY_URL = "https://xyzzy:xyzzy@localhost:80/xyzzy?abc=def#fragment" # comment"#,
        )
        .unwrap();
        let expetced = (
            "MY_URL",
            "\"https://xyzzy:xyzzy@localhost:80/xyzzy?abc=def#fragment\"",
        );
        assert_eq!(actual, expetced);
    }
}
