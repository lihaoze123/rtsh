use std::str::FromStr;

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
        let raw = s.trim().to_owned();
        let mut argv: Vec<String> = raw.split_ascii_whitespace().map(str::to_owned).collect();

        if argv.is_empty() {
            return Err(ParseCommandError::EmptyCommand);
        }

        let background = argv.last().is_some_and(|last| last == "&");
        if background {
            argv.pop();
        }

        let kind = match argv[0].as_str() {
            "quit" => CommandKind::Quit,
            "jobs" => CommandKind::Jobs,
            "bg" => CommandKind::Bg,
            "fg" => CommandKind::Fg,
            _ => CommandKind::External,
        };

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

    #[test]
    fn parse_ls_command_works() {
        let raw = " /usr/bin/env ls\t-l  -d & ";
        let parsed_cmd: ParsedCommand = raw.parse().unwrap();
        assert_eq!(
            parsed_cmd,
            ParsedCommand {
                argv: vec!["/usr/bin/env", "ls", "-l", "-d"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                background: true,
                kind: CommandKind::External,
            }
        )
    }

    #[test]
    fn parse_builtin_command_works() {
        let raw = "quit";
        let parsed_cmd: ParsedCommand = raw.parse().unwrap();
        assert!(matches!(parsed_cmd.kind(), CommandKind::Quit));
    }

    #[test]
    fn parse_empty_command_works() {
        let raw = "  \t\t\n";
        let parsed_cmd = raw.parse::<ParsedCommand>();
        assert!(matches!(parsed_cmd, Err(ParseCommandError::EmptyCommand)));
    }
}
