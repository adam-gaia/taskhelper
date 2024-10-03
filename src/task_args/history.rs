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

#[derive(Debug, Clone)]
pub enum History {
    Annual,
    Daily,
    Monthly,
    Weekly,
}

fn annual(s: &mut &str) -> PResult<History> {
    "annual".map(|_| History::Annual).parse_next(s)
}

fn daily(s: &mut &str) -> PResult<History> {
    "daily".map(|_| History::Daily).parse_next(s)
}

fn monthly(s: &mut &str) -> PResult<History> {
    "monthly".map(|_| History::Monthly).parse_next(s)
}

fn weekly(s: &mut &str) -> PResult<History> {
    "weekly".map(|_| History::Weekly).parse_next(s)
}

fn history(s: &mut &str) -> PResult<History> {
    alt((daily, monthly, weekly, annual)).parse_next(s)
}

impl FromStr for History {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        history.parse(s).map_err(|_| ParseError::History)
    }
}
