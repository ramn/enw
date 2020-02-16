use std::{env, ffi::OsString, fs, path::PathBuf, process::Command};

use clap::{App, AppSettings, Arg, ArgMatches};

pub type BoxError = Box<dyn std::error::Error>;

const ABOUT: &str =
    "Similar to the GNU env command, but will automatically load an .env file, if found.";
const USAGE: &str = "enw [OPTION]... [-] [NAME=VALUE] [COMMAND [ARGS]...]";
const DEFAULT_ENV_FILE_NAME: &str = ".env";

#[derive(Debug, Default)]
struct OptionsBuilder {
    env_files: Vec<PathBuf>,
    vars: Vec<(String, String)>,
    command: String,
    args: Vec<String>,
    ignore_env: bool,
    load_implicit_env_file: bool,
}

pub fn run(args: impl Iterator<Item = impl Into<OsString> + Clone>) -> Result<(), BoxError> {
    let matches = parse_arguments(args);
    let opt_builder = OptionsBuilder::with_arg_matches(matches)?;
    let env_files: Vec<_> = opt_builder
        .env_files
        .into_iter()
        .filter(|p| p.exists())
        .map(|path| {
            if path.is_dir() {
                path.join(DEFAULT_ENV_FILE_NAME)
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

fn parse_arguments(args: impl Iterator<Item = impl Into<OsString> + Clone>) -> ArgMatches<'static> {
    App::new("enw")
        .about(ABOUT)
        .usage(USAGE)
        .setting(AppSettings::TrailingVarArg)
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
                .long("ignore-env")
                .help("start with an empty environment"),
        )
        .arg(
            Arg::with_name("no_implicit_env_file")
                .short("n")
                .long("no-env-file")
                .help("don't implicitly load the .env file from current dir"),
        )
        .arg(
            Arg::with_name("rest")
                .value_name("REST")
                .takes_value(true)
                .hidden(true)
                .multiple(true),
        )
        .get_matches_from(args)
}

fn parse_env_file(text: &str) -> Vec<Result<(String, String), BoxError>> {
    text.lines()
        // TODO: don't trim the end of the string
        .map(|line| line.trim())
        .filter(|line| line.contains('=') && !line.starts_with('#'))
        .map(parse_env_line)
        .collect()
}

// TODO: Add non-parsing mode? Don't trim or expand quotes in the value. Don't support comment at
// end of line. We should be compatible with different shells, that might not do automatic
// expansion (dequoting).
fn parse_env_line(line: &str) -> Result<(String, String), BoxError> {
    let mut parts = line.splitn(2, '=').map(str::trim);
    let key = parts.next().ok_or("KEY missing")?;
    let value = parts.next().ok_or("VALUE missing")?;
    let value = parse_value(value);
    Ok((key.to_owned(), value))
}

fn parse_value(v: &str) -> String {
    enum S {
        Escape,
        Start,
        Quote,
    };
    let mut out = String::with_capacity(v.len());
    let mut state = vec![S::Start];
    let mut chars = v.chars();
    'outer: while let Some(c) = chars.next() {
        match state.last().unwrap() {
            S::Escape => {
                match c {
                    '"' | '\'' => {
                        out.push(c);
                    }
                    _ => {
                        out.push('\\');
                        out.push(c);
                    }
                };
                state.pop();
            }
            S::Quote => match c {
                '"' => {
                    state.pop();
                    state.push(S::Start);
                }
                '\\' => {
                    state.push(S::Escape);
                }
                _ => {
                    out.push(c);
                }
            },
            S::Start => match c {
                '"' => {
                    state.pop();
                    state.push(S::Quote);
                }
                '\\' => state.push(S::Escape),
                '#' => {
                    break 'outer;
                }
                _ => {
                    out.push(c);
                }
            },
        }
    }
    trim_end_whitespace(&mut out);
    out
}

/// Trim ending whitespace without reallocating
fn trim_end_whitespace(s: &mut String) {
    let trailing_whitespace = s
        .chars()
        .rev()
        .take_while(|&c| char::is_whitespace(c))
        .count();
    s.truncate(s.len() - trailing_whitespace);
}

impl OptionsBuilder {
    fn with_arg_matches(matches: ArgMatches<'static>) -> Result<Self, BoxError> {
        const DEFAULT_VEC: Vec<String> = Vec::new();
        let mut opt_builder = OptionsBuilder::default();
        opt_builder.ignore_env = matches.is_present("ignore_env");
        opt_builder.load_implicit_env_file = !matches.is_present("no_implicit_env_file");

        if opt_builder.load_implicit_env_file {
            // .env file from current dir automatically loaded, overridden by explicitly passed in .env
            // files
            opt_builder
                .env_files
                .push(env::current_dir()?.join(DEFAULT_ENV_FILE_NAME));
        }
        opt_builder.env_files.extend(
            matches
                .values_of_lossy("env_file")
                .unwrap_or(DEFAULT_VEC)
                .iter()
                .map(|fname| fname.into()),
        );
        let rest = matches.values_of_lossy("rest").unwrap_or_else(|| vec![]);
        opt_builder.vars = rest
            .iter()
            .take_while(|x| x.contains('='))
            .map(|line| parse_env_line(&line))
            .collect::<Result<Vec<_>, _>>()?;
        opt_builder.command = rest
            .get(opt_builder.vars.len())
            .cloned()
            .ok_or_else(|| "No COMMAND supplied")?;
        opt_builder.args = rest
            .iter()
            .skip(opt_builder.vars.len() + 1)
            .cloned()
            .collect();
        Ok(opt_builder)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_parse_env_line() {
        assert_eq!(
            p(r#" MY_URL = "https://xyzzy:xyzzy@localhost:80/xyzzy?abc=def#fragment" # comment"#),
            owned(
                "MY_URL",
                "https://xyzzy:xyzzy@localhost:80/xyzzy?abc=def#fragment",
            ),
        );
        assert_eq!(
            p(r##"key="https://xyzzy:xyzzy@localhost:80/xyzzy?abc=\"def#\"#fragment" # comment"##),
            owned(
                "key",
                r##"https://xyzzy:xyzzy@localhost:80/xyzzy?abc="def#"#fragment"##,
            ),
        );
        assert_eq!(
            p(r##"key="https://xyzzy:xyzzy@localhost:80/xyzzy?abc=\'def#\'#fragment" # comment"##),
            owned(
                "key",
                r##"https://xyzzy:xyzzy@localhost:80/xyzzy?abc='def#'#fragment"##,
            ),
        );
        assert_eq!(
            p(r##"key="https://xyzzy:xyzzy@localhost:80/xyzzy?abc=\\"def\\"#fragment" # comment"##),
            owned(
                "key",
                r##"https://xyzzy:xyzzy@localhost:80/xyzzy?abc=\\def\\#fragment"##,
            ),
        );
        assert_eq!(
            p(
                r##"key="https://xyzzy:xyzzy@localhost:80/xyzzy?abc=\\"def#\\"#fragment" # comment"##
            ),
            owned(
                "key",
                r##"https://xyzzy:xyzzy@localhost:80/xyzzy?abc=\\def"##,
            ),
        );

        assert_eq!(
            p(r##"key="my multiline\nstring" # comment"##),
            owned("key", r##"my multiline\nstring"##,),
        );
    }

    fn p(input: &str) -> (String, String) {
        parse_env_line(input).unwrap()
    }

    fn owned(k: &str, v: &str) -> (String, String) {
        (k.into(), v.into())
    }
}
