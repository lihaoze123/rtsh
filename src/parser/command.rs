use std::str::FromStr;

use crate::parser::cursor::{Cursor, CursorError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Quit,
    Jobs,
    Bg,
    Fg,
    External,
}

#[derive(thiserror::Error, Debug)]
pub enum ParseCommandError {
    #[error("command string is empty!")]
    EmptyCommand,

    #[error("{0}")]
    Cursor(#[from] CursorError),
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParsedCommand {
    argv: Vec<String>,
    background: bool,
    kind: CommandKind,
}

impl ParsedCommand {
    pub fn kind(&self) -> CommandKind {
        self.kind
    }
}

impl FromStr for ParsedCommand {
    type Err = ParseCommandError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let cursor = Cursor::new(s);

        let mut argv = cursor.collect::<Result<Vec<_>, CursorError>>()?;

        let background = argv
            .last()
            .is_some_and(|&last| last == "&" && !last.is_quoted());
        if background {
            argv.pop();
        }

        if argv.is_empty() {
            return Err(ParseCommandError::EmptyCommand);
        }

        let kind = match argv[0].as_str() {
            "quit" => CommandKind::Quit,
            "jobs" => CommandKind::Jobs,
            "bg" => CommandKind::Bg,
            "fg" => CommandKind::Fg,
            _ => CommandKind::External,
        };

        let argv = argv.into_iter().map(String::from).collect();
        Ok(ParsedCommand {
            argv,
            background,
            kind,
        })
    }
}

#[cfg(test)]
mod test {
    use std::{assert_eq, matches};

    use crate::parser::{CommandKind, ParseCommandError, ParsedCommand};

    fn assert_parsed(raw: &str, argv: &[&str], background: bool, kind: CommandKind) {
        let parsed_command = raw.parse::<ParsedCommand>().unwrap();
        assert_eq!(
            parsed_command,
            ParsedCommand {
                argv: argv.iter().map(|arg| (*arg).to_owned()).collect(),
                background,
                kind,
            }
        )
    }

    #[test]
    fn parse_external_background_command() {
        assert_parsed(
            " /usr/bin/ls\t-l  -d & ",
            &["/usr/bin/ls", "-l", "-d"],
            true,
            CommandKind::External,
        );
    }

    #[test]
    fn parse_quoted() {
        assert_parsed(
            "/usr/bin/echo 'Hello World'",
            &["/usr/bin/echo", "Hello World"],
            false,
            CommandKind::External,
        );
    }

    #[test]
    fn parse_builtin_command() {
        for (raw, kind) in [
            ("quit", CommandKind::Quit),
            ("jobs", CommandKind::Jobs),
            ("bg %1", CommandKind::Bg),
            ("fg %1", CommandKind::Fg),
        ] {
            let parsed_command = raw.parse::<ParsedCommand>().unwrap();
            assert_eq!(parsed_command.kind, kind);
        }
    }

    #[test]
    fn parse_empty_command() {
        let raw = "  \t\t\n";
        let parsed_cmd = raw.parse::<ParsedCommand>();
        assert!(matches!(parsed_cmd, Err(ParseCommandError::EmptyCommand)));
    }

    #[test]
    fn parse_trailing_unquoted_ampersand_as_background_marker() {
        for (raw, argv) in [
            ("echo '&'", &["echo", "&"][..]),
            ("echo & later", &["echo", "&", "later"][..]),
            ("echo argument&", &["echo", "argument&"][..]),
        ] {
            assert_parsed(raw, argv, false, CommandKind::External);
        }
        assert_parsed("echo &", &["echo"], true, CommandKind::External);
    }
}
