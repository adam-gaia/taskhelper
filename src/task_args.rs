pub mod burndown;
pub mod filter;
pub mod modifier;

use filter::Filter;
use filter::Filters;
use libc::PR_GET_PDEATHSIG;
use thiserror::Error;

pub mod history;
pub mod project;

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
use winnow::combinator::Repeat;
use winnow::stream::Accumulate;
use winnow::stream::AsChar;
use winnow::token::one_of;
use winnow::token::take_while;
use winnow::PResult;
use winnow::Parser;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Unable to parse project name")]
    Project,
    #[error("Unable to parse filter")]
    Filter,
    #[error("Unable to parse modifier")]
    Modifier,
    #[error("Unable to parse burndown")]
    Burndown,
    #[error("Unable to parse (g)history")]
    History,
}

// TODO: parse things in quotes?

fn word<'a>(s: &mut &'a str) -> PResult<&'a str> {
    take_while(1.., |c: char| {
        c.is_alphanum() || c == '_' || c == '-' || c == '.'
    })
    .parse_next(s)
}

fn word_space_or_end<'a>(s: &mut &'a str) -> PResult<&'a str> {
    let w = word.parse_next(s)?;
    let _ = alt((multispace1, eof)).parse_next(s)?;
    Ok(w)
}

fn multi_word(s: &mut &str) -> PResult<Vec<String>> {
    repeat(0.., word_space_or_end.map(|s: &str| s.to_string())).parse_next(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use s_string::s;

    #[test]
    fn test_words() {
        let mut input = "this is a lot of words";
        let expected = vec![
            s!("this"),
            s!("is"),
            s!("a"),
            s!("lot"),
            s!("of"),
            s!("words"),
        ];
        let actual = multi_word.parse_next(&mut input).unwrap();
        assert_eq!(expected, actual);
        assert_eq!("", input);
    }

    #[test]
    fn test_words_till_colon() {
        let mut input = "this is a lot: of words";
        let expected = vec![s!("this"), s!("is"), s!("a")];
        let actual = multi_word.parse_next(&mut input).unwrap();
        assert_eq!(expected, actual);
        assert_eq!("lot: of words", input);
    }
}
