use super::multi_word;
use super::project::project;
use super::project::Project;
use super::word;
use super::word_space_or_end;
use super::ParseError;
use color_eyre::eyre::bail;
use color_eyre::Result;
use core::fmt::Error;
use log::debug;
use log::info;
use log::trace;
use serde::Serialize;
use std::env;
use std::fmt;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::process::Command;
use std::str;
use std::str::FromStr;
use winnow::ascii::alphanumeric1;
use winnow::ascii::multispace1;
use winnow::combinator::alt;
use winnow::combinator::eof;
use winnow::combinator::repeat;
use winnow::stream::Accumulate;
use winnow::stream::AsChar;
use winnow::token::one_of;
use winnow::token::take_while;
use winnow::PResult;
use winnow::Parser;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Modifier {
    Description(String),
    Project(Project),
    Other { name: String, value: String },
}

impl FromStr for Modifier {
    type Err = ParseError;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        modifier.parse(s).map_err(|_| ParseError::Modifier)
    }
}

impl fmt::Display for Modifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let repr = match self {
            Modifier::Description(desc) => desc.to_string(),
            Modifier::Project(project) => project.to_string(),
            Modifier::Other { name, value } => format!("{}:{}", name, value),
        };
        write!(f, "{}", repr)
    }
}

fn other(s: &mut &str) -> PResult<Modifier> {
    let name = word.map(|s: &str| s.to_string()).parse_next(s)?;
    let _ = ":".parse_next(s)?;
    let value = word.map(|s: &str| s.to_string()).parse_next(s)?;
    Ok(Modifier::Other { name, value })
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Modifiers {
    modifiers: Vec<Modifier>,
}

impl Modifiers {
    pub fn modifiers(&self) -> &[Modifier] {
        &self.modifiers
    }
}

impl FromStr for Modifiers {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        modifiers.parse(s).map_err(|_| ParseError::Modifier)
    }
}

impl Accumulate<Modifier> for Modifiers {
    fn initial(capacity: Option<usize>) -> Self {
        let modifiers = match capacity {
            Some(c) => Vec::with_capacity(c),
            None => Vec::new(),
        };
        Modifiers { modifiers }
    }

    fn accumulate(&mut self, acc: Modifier) {
        self.modifiers.push(acc);
    }
}

fn description(s: &mut &str) -> PResult<Modifier> {
    word.map(|word| Modifier::Description(word.to_string()))
        .parse_next(s)
}

fn standard_modifier(s: &mut &str) -> PResult<Modifier> {
    alt((project.map(|m| Modifier::Project(m)), other)).parse_next(s)
}

fn modifier(s: &mut &str) -> PResult<Modifier> {
    alt((standard_modifier, description)).parse_next(s)
}

fn modifier_space_or_end<'a>(s: &mut &'a str) -> PResult<Modifier> {
    let m = modifier.parse_next(s)?;
    let _ = alt((multispace1, eof)).parse_next(s)?;
    Ok(m)
}

fn modifiers(s: &mut &str) -> PResult<Modifiers> {
    repeat(0.., modifier_space_or_end)
        .map(|modifiers| Modifiers { modifiers })
        .parse_next(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_space_at_end() {
        let mut input = "test ";
        let expected = vec![String::from("test")];
        let actual = multi_word.parse_next(&mut input).unwrap();
        assert_eq!(expected, actual);
        assert_eq!("", input);
    }

    #[test]
    fn test_multi_word() {
        let mut input = "foo bar test project:test foo:bar";
        let expected = vec![
            String::from("foo"),
            String::from("bar"),
            String::from("test"),
        ];
        let actual = multi_word.parse_next(&mut input).unwrap();
        assert_eq!(expected, actual);
        assert_eq!("project:test foo:bar", input);
    }

    #[test]
    fn test_description() {
        let mut input = "foo bar test project:test foo:bar";
        let expected = Modifier::Description(String::from("foo"));
        let actual = description.parse_next(&mut input).unwrap();
        assert_eq!(expected, actual);
        assert_eq!(" bar test project:test foo:bar", input);
    }

    #[test]
    fn test_modifiers_1() {
        let mods = Modifiers {
            modifiers: vec![
                Modifier::Description(String::from("foo")),
                Modifier::Description(String::from("bar")),
            ],
        };

        let mut input = "foo bar";

        let actual = modifiers.parse_next(&mut input).unwrap();
        assert_eq!(mods, actual);
        assert_eq!("", input);
    }

    #[test]
    fn test_modifiers_2() {
        let mods = Modifiers {
            modifiers: vec![
                Modifier::Description(String::from("foo")),
                Modifier::Description(String::from("bar")),
                Modifier::Description(String::from("test")),
            ],
        };

        let mut input = "foo bar test";

        let actual = modifiers.parse_next(&mut input).unwrap();
        assert_eq!(mods, actual);
        assert_eq!("", input);
    }

    #[test]
    fn test_modifiers_3() {
        let mods = Modifiers {
            modifiers: vec![
                Modifier::Description(String::from("foo")),
                Modifier::Description(String::from("bar")),
                Modifier::Description(String::from("test")),
                Modifier::Project(Project::with_name("test")),
            ],
        };

        let mut input = "foo bar test project:test";

        let actual = modifiers.parse_next(&mut input).unwrap();
        assert_eq!(mods, actual);
        assert_eq!("", input);
    }

    #[test]
    fn test_multicall_modifier_4() {
        let mods = Modifiers {
            modifiers: vec![
                Modifier::Description(String::from("foo")),
                Modifier::Description(String::from("bar")),
                Modifier::Description(String::from("test")),
                Modifier::Project(Project::with_name("test")),
                Modifier::Other {
                    name: String::from("foo"),
                    value: String::from("bar"),
                },
            ],
        };

        let mut input = "foo bar test project:test foo:bar";

        let actual = modifiers.parse_next(&mut input).unwrap();
        assert_eq!(mods, actual);
        assert_eq!("", input);
    }
}
