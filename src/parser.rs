use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Quit,
    Jobs,
    Bg,
    Fg,
    Executable,
}

#[derive(thiserror::Error, Debug)]
pub enum ParseCommandError {
    #[error("command string is empty!")]
    EmptyCommand,

    #[error("unknown command")]
    UnknownCommand,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParsedCommand {
    argv: Vec<String>,
    background: bool,
    kind: CommandKind,
}

impl ParsedCommand {
    pub fn new(argv: Vec<String>, background: bool, raw: String) -> Self {
        let kind = match argv[0].as_str() {
            "quit" => CommandKind::Quit,
            "jobs" => CommandKind::Jobs,
            "bg" => CommandKind::Bg,
            "fg" => CommandKind::Fg,
            _ => CommandKind::Executable,
        };
        Self {
            argv,
            background,
            kind,
        }
    }

    pub fn kind(&self) -> CommandKind {
        self.kind
    }
}

impl FromStr for ParsedCommand {
    type Err = ParseCommandError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let raw = s.trim().to_owned();
        let mut split = raw.split_ascii_whitespace();

        let mut argv: Vec<String> = Vec::new();
        let mut background = false;
        while let Some(arg) = split.next() {
            if arg == "&" {
                background = true;
                break;
            }
            argv.push(arg.to_string());
        }

        if argv.is_empty() {
            return Err(ParseCommandError::EmptyCommand);
        }

        Ok(ParsedCommand::new(argv, background, raw))
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
                argv: vec![
                    "/usr/bin/env".to_owned(),
                    "ls".to_owned(),
                    "-l".to_owned(),
                    "-d".to_owned()
                ],
                background: true,
                kind: CommandKind::Executable,
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
