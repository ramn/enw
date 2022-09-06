use std::{
    collections::HashMap, env, ffi::OsString, fs, os::unix::process::CommandExt, path::PathBuf,
    process::Command,
};

use clap::{App, AppSettings, Arg, ArgMatches};

pub type BoxError = Box<dyn std::error::Error>;

const ABOUT: &str =
    "Similar to the GNU env command, but will automatically load an .env file, if found.";
const USAGE: &str = "enw [OPTION]... [-] [NAME=VALUE] [COMMAND [ARGS]...]";
const DEFAULT_ENV_FILE_NAME: &str = ".env";

#[derive(Debug)]
struct EnvFile {
    path: PathBuf,
    is_default: bool,
}

#[derive(Debug, Default)]
struct OptionsBuilder {
    env_files: Vec<EnvFile>,
    vars: Vec<(String, String)>,
    command: Option<String>,
    args: Vec<String>,
    ignore_env: bool,
    load_implicit_env_file: bool,
    print_warnings: bool,
}

pub fn run(args: impl Iterator<Item = impl Into<OsString> + Clone>) -> Result<(), BoxError> {
    let matches = parse_arguments(args);
    let opt_builder = OptionsBuilder::with_arg_matches(matches)?;
    let mut warnings = Vec::new();
    let env_files: Vec<_> = opt_builder
        .env_files
        .into_iter()
        .filter_map(|env_file| {
            let EnvFile { path, is_default } = env_file;
            if path.is_dir() {
                let file_path = path.join(DEFAULT_ENV_FILE_NAME);
                if file_path.is_file() {
                    Some(file_path)
                } else {
                    if !is_default {
                        warnings.push(format!(
                            "no {DEFAULT_ENV_FILE_NAME} file found in {}",
                            path.to_string_lossy()
                        ));
                    }
                    None
                }
            } else if path.is_file() {
                Some(path)
            } else {
                if !is_default {
                    warnings.push(format!("{} does not exist", path.to_string_lossy()));
                }
                None
            }
        })
        .map(fs::read_to_string)
        .collect::<Result<_, _>>()?;
    let mut env_vars: HashMap<_, _> = env_files
        .iter()
        .flat_map(|text| parse_env_doc(text))
        .collect::<Result<_, _>>()?;
    env_vars.extend(opt_builder.vars.into_iter());
    let mut env_vars: Vec<_> = env_vars.into_iter().collect();
    env_vars.sort();
    if opt_builder.print_warnings {
        for warning in warnings {
            eprintln!("warning: {warning}");
        }
    }
    if let Some(command) = opt_builder.command {
        let mut cmd = Command::new(command);
        if opt_builder.ignore_env {
            cmd.env_clear();
        }
        cmd.envs(env_vars).args(opt_builder.args);
        Err(cmd.exec().into())
    } else {
        for (key, value) in env_vars {
            println!("{}={}", key, value);
        }
        Ok(())
    }
}

fn parse_arguments(args: impl Iterator<Item = impl Into<OsString> + Clone>) -> ArgMatches<'static> {
    App::new("enw")
        .about(ABOUT)
        .version(env!("CARGO_PKG_VERSION"))
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
        .arg(
            Arg::with_name("quiet")
                .short("q")
                .long("quiet")
                .help("don't print any warnings"),
        )
        .get_matches_from(args)
}

fn parse_env_doc(text: &str) -> Vec<Result<(String, String), BoxError>> {
    text.lines()
        .map(|line| line.trim_start())
        .filter(|line| line.contains('=') && !line.starts_with('#'))
        .map(parse_env_line)
        .collect()
}

fn parse_env_line(line: &str) -> Result<(String, String), BoxError> {
    let mut parts = line.splitn(2, '=').map(str::trim);
    let key = parts.next().ok_or("KEY missing")?;
    if !key_is_valid(key) {
        return Err(format!("KEY contains invalid characters: {}", key).into());
    }
    let value = parse_value(parts.next().unwrap_or(""))?;
    Ok((key.to_owned(), value))
}

fn key_is_valid(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .take(1)
            .all(|c| c.is_ascii_alphabetic() || c == '_')
        && !key.chars().any(|c| c.is_whitespace())
}

fn parse_value(v: &str) -> Result<String, BoxError> {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum S {
        DoubleQuote,
        Escape,
        SingleQuote,
        Start,
    }
    let mut out = String::with_capacity(v.len());
    let mut state = vec![S::Start];
    'outer: for c in v.chars() {
        let s = *state.last().unwrap();
        match s {
            S::Escape => {
                state.pop();
                match (state.last().unwrap(), c) {
                    (S::DoubleQuote, '"')
                    | (S::SingleQuote, '\'')
                    | (S::DoubleQuote, '\\')
                    | (S::SingleQuote, '\\') => {
                        out.push(c);
                    }
                    (S::DoubleQuote, _) | (S::SingleQuote, _) => {
                        out.push('\\');
                        out.push(c);
                    }
                    (S::Start, _) => match c {
                        '"' | '\'' | ' ' | '$' | '\\' => out.push(c),
                        _ => {
                            return Err(
                                format!("error parsing value, invalid escape: {}", c).into()
                            );
                        }
                    },
                    (S::Escape, _) => unreachable!(),
                }
            }
            S::DoubleQuote | S::SingleQuote => match (s, c) {
                (S::DoubleQuote, '"') | (S::SingleQuote, '\'') => {
                    state.pop();
                    state.push(S::Start);
                }
                (_, '\\') => {
                    state.push(S::Escape);
                }
                _ => {
                    out.push(c);
                }
            },
            S::Start => match c {
                '"' => {
                    state.pop();
                    state.push(S::DoubleQuote);
                }
                '\'' => {
                    state.pop();
                    state.push(S::SingleQuote);
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
    if !(state.len() == 1 && state[0] == S::Start) {
        return match state.last() {
            Some(S::DoubleQuote) | Some(S::SingleQuote) => {
                Err("error parsing value: unmatched quotes.".into())
            }
            _ => Err("error parsing value".into()),
        };
    }
    trim_end_whitespace(&mut out);
    Ok(out)
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
        let mut opt_builder = OptionsBuilder {
            ignore_env: matches.is_present("ignore_env"),
            load_implicit_env_file: !matches.is_present("no_implicit_env_file"),
            print_warnings: !matches.is_present("quiet"),
            ..Default::default()
        };
        if opt_builder.load_implicit_env_file {
            // .env file from current dir automatically loaded, overridden by explicitly passed in .env
            // files
            opt_builder.env_files.push(EnvFile {
                path: env::current_dir()?.join(DEFAULT_ENV_FILE_NAME),
                is_default: true,
            });
        }
        opt_builder.env_files.extend(
            matches
                .values_of_lossy("env_file")
                .unwrap_or(DEFAULT_VEC)
                .iter()
                .map(|fname| EnvFile {
                    path: fname.into(),
                    is_default: false,
                }),
        );
        let rest = matches.values_of_lossy("rest").unwrap_or_default();
        opt_builder.vars = rest
            .iter()
            .take_while(|x| x.contains('='))
            .map(|line| parse_env_line(line))
            .collect::<Result<Vec<_>, _>>()?;
        opt_builder.command = rest.get(opt_builder.vars.len()).cloned();
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
                r##"https://xyzzy:xyzzy@localhost:80/xyzzy?abc=\'def#\'#fragment"##,
            ),
        );
        assert_eq!(
            p(
                r##"key='https://xyzzy:xyzzy@localhost:80/"xyzzy?\"abc\"=\'def#\'#fragment"' # comment"##
            ),
            owned(
                "key",
                r##"https://xyzzy:xyzzy@localhost:80/"xyzzy?\"abc\"='def#'#fragment""##,
            ),
        );
        assert_eq!(
            p(r##"key="https://xyzzy:xyzzy@localhost:80/xyzzy?abc=\\"def\\"#fragment" # comment"##),
            owned(
                "key",
                r##"https://xyzzy:xyzzy@localhost:80/xyzzy?abc=\def\#fragment"##,
            ),
        );
        assert_eq!(
            p(
                r##"key="https://xyzzy:xyzzy@localhost:80/xyzzy?abc=\\"def#\\"#fragment" # comment"##
            ),
            owned(
                "key",
                r##"https://xyzzy:xyzzy@localhost:80/xyzzy?abc=\def"##,
            ),
        );

        assert_eq!(
            p(r##"key="my multiline\nstring" # comment"##),
            owned("key", r##"my multiline\nstring"##,),
        );

        assert_eq!(
            p(r##"key="my multiline\nstring" # comment"##),
            owned("key", r##"my multiline\nstring"##,),
        );
    }

    // Test cases borrowed from dotenv

    #[test]
    fn test_parse_line_env() {
        let inputs = include_str!("../tests/data/input_01.txt");
        let expected_iter = vec![
            ("KEY", "1"),
            ("KEY2", "2"),
            ("KEY3", "3"),
            ("KEY4", "fo ur"),
            ("KEY5", "fi ve"),
            ("KEY6", "s ix"),
            ("KEY7", ""),
            ("KEY8", ""),
            ("KEY9", ""),
            ("KEY10", "whitespace before ="),
            ("KEY11", "whitespace after ="),
        ]
        .into_iter()
        .map(|(k, v)| owned(k, v));
        for (actual, expected) in inputs.lines().map(parse_env_line).zip(expected_iter) {
            assert_eq!(actual.unwrap(), expected);
        }
    }

    #[test]
    fn test_parse_line_comment() {
        let actual = parse_env_doc(
            r"\
            # foo=bar\
            #    ",
        );
        assert!(actual.is_empty());
    }

    #[test]
    fn test_parse_line_invalid() {
        // Note 4 spaces after 'invalid' below
        let actual = parse_env_doc(
            "  invalid    \n\
            bad key = no work\n\
            =lacks key
            1abc=starts_with_digit",
        );

        assert_eq!(actual.len(), 3);
        for actual in actual {
            assert!(actual.is_err(), "unexpectedly ok: {:?}", actual.unwrap());
        }
    }

    #[test]
    fn test_parse_value_escapes() {
        let actual = parse_env_doc(
            r#"
            KEY1=foo\ bar\ baz
            KEY2=\$foo
            KEY3="foo bar \"baz\""
            KEY4='foo $\bar'\''baz'
            KEY5="'\"foo\\"\ "bar"
            KEY6="foo" #end of line comment
            KEY7="line 1\nline 2"
            "#,
        );

        let expected = vec![
            ("KEY1", r#"foo bar baz"#),
            ("KEY2", r#"$foo"#),
            ("KEY3", r#"foo bar "baz""#),
            ("KEY4", r#"foo $\bar'baz"#),
            ("KEY5", r#"'"foo\ bar"#),
            ("KEY6", "foo"),
            ("KEY7", "line 1\\nline 2"),
        ]
        .into_iter()
        .map(|(k, v)| owned(k, v));

        for (actual, expected) in actual.into_iter().zip(expected) {
            let actual = actual.unwrap();
            assert_eq!(actual, expected, "got: {:#?}", actual);
        }
    }

    #[test]
    fn test_parse_value_escapes_invalid() {
        let actuals = parse_env_doc(
            r#"
            KEY1="foo
            KEY2='foo bar''
            KEY3=foo\8bar
            "#,
        );

        for actual in actuals {
            assert!(actual.is_err(), "expected err: {:?}", actual);
        }
    }

    #[test]
    fn test_parse_keys_with_non_standard_chars() {
        let actuals = parse_env_doc(
            r#"
            key.1=value
            KEY/2=value
            KEY:3=value
            "#,
        );

        let actuals = actuals
            .into_iter()
            .map(|r| r.map(|(k, _v)| k))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(actuals, vec!["key.1", "KEY/2", "KEY:3"]);
    }

    fn p(input: &str) -> (String, String) {
        parse_env_line(input).unwrap()
    }

    fn owned(k: &str, v: &str) -> (String, String) {
        (k.into(), v.into())
    }
}
