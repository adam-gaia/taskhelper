use super::filter::Filter;
use super::filter::Filters;
use super::word;
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
use winnow::combinator::alt;
use winnow::combinator::repeat;
use winnow::stream::Accumulate;
use winnow::stream::AsChar;
use winnow::token::one_of;
use winnow::token::take_while;
use winnow::PResult;

use winnow::Parser;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Project {
    name: String,
}

impl Project {
    pub fn with_name(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
    pub fn name(&self) -> &String {
        &self.name
    }
}

impl fmt::Display for Project {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "project:{}", self.name)
    }
}

fn project_name(s: &mut &str) -> PResult<String> {
    word.map(|s: &str| s.to_string()).parse_next(s)
}

pub fn project(s: &mut &str) -> PResult<Project> {
    let _ = alt(("project", "proj")).parse_next(s)?;
    let _ = ":".parse_next(s)?;
    project_name.map(|name| Project { name }).parse_next(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_project() {
        let mut input = "project:test";
        let expected = Project::with_name("test");
        let actual = project.parse_next(&mut input).unwrap();
        assert_eq!(expected, actual);
        assert_eq!("", input);
    }
}
