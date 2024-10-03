use super::project::{project, Project};
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
use winnow::ascii::{alphanumeric1, multispace1};
use winnow::combinator::repeat;
use winnow::combinator::{alt, eof};
use winnow::stream::Accumulate;
use winnow::stream::AsChar;
use winnow::token::one_of;
use winnow::token::take_while;
use winnow::PResult;
use winnow::Parser;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Filter {
    Project(Project),
    Other { name: String, value: String },
}

impl fmt::Display for Filter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let repr = match self {
            Filter::Project(project) => project.to_string(),
            Filter::Other { name, value } => {
                format!("{}:{}", name, value)
            }
        };
        write!(f, "{}", repr)
    }
}

fn other(s: &mut &str) -> PResult<Filter> {
    let name = word.map(|s: &str| s.to_string()).parse_next(s)?;
    let _ = ":".parse_next(s)?;
    let value = word.map(|s: &str| s.to_string()).parse_next(s)?;
    Ok(Filter::Other { name, value })
}

fn filter(s: &mut &str) -> PResult<Filter> {
    alt((project.map(|p| Filter::Project(p)), other)).parse_next(s)
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Filters {
    filters: Vec<Filter>,
}

impl Filters {
    fn with_filters(filters: Vec<Filter>) -> Self {
        Filters { filters }
    }

    pub fn filters(&self) -> &[Filter] {
        &self.filters
    }
}

impl Accumulate<Filter> for Filters {
    fn initial(capacity: Option<usize>) -> Self {
        let filters = match capacity {
            Some(c) => Vec::with_capacity(c),
            None => Vec::new(),
        };
        Filters { filters }
    }

    fn accumulate(&mut self, acc: Filter) {
        self.filters.push(acc);
    }
}

fn filter_space_or_end<'a>(s: &mut &'a str) -> PResult<Filter> {
    let f = filter.parse_next(s)?;
    let _ = alt((multispace1, eof)).parse_next(s)?;
    Ok(f)
}

fn filters(s: &mut &str) -> PResult<Filters> {
    repeat(0.., filter_space_or_end)
        .map(|filters| Filters::with_filters(filters))
        .parse_next(s)
}

impl FromStr for Filters {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        filters.parse(s).map_err(|_| ParseError::Filter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_filters_single() {
        let mut input = "project:test";
        let expected = Filters {
            filters: vec![Filter::Project(Project::with_name("test"))],
        };
        let actual = filters.parse_next(&mut input).unwrap();
        assert_eq!(expected, actual);
        assert_eq!("", input);
    }

    #[test]
    fn test_filters_multi() {
        let mut input = "project:test foo:bar";
        let expected = Filters {
            filters: vec![
                Filter::Project(Project::with_name("test")),
                Filter::Other {
                    name: String::from("foo"),
                    value: String::from("bar"),
                },
            ],
        };
        let actual = filters.parse_next(&mut input).unwrap();
        assert_eq!(expected, actual);
        assert_eq!("", input);
    }

    #[test]
    fn test_filters_from_str() {
        let input = "project:test foo:bar";
        let expected = Filters {
            filters: vec![
                Filter::Project(Project::with_name("test")),
                Filter::Other {
                    name: String::from("foo"),
                    value: String::from("bar"),
                },
            ],
        };
        let actual = Filters::from_str(input).unwrap();
        assert_eq!(expected, actual);
    }
}
