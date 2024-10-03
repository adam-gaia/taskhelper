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
pub enum Burndown {
    Daily,
    Monthly,
    Weekly,
}

fn daily(s: &mut &str) -> PResult<Burndown> {
    "daily".map(|_| Burndown::Daily).parse_next(s)
}

fn monthly(s: &mut &str) -> PResult<Burndown> {
    "monthly".map(|_| Burndown::Monthly).parse_next(s)
}

fn weekly(s: &mut &str) -> PResult<Burndown> {
    "weekly".map(|_| Burndown::Weekly).parse_next(s)
}

fn burndown(s: &mut &str) -> PResult<Burndown> {
    alt((daily, monthly, weekly)).parse_next(s)
}

impl FromStr for Burndown {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        burndown.parse(s).map_err(|_| ParseError::Burndown)
    }
}
