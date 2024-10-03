use crate::task_args::burndown::Burndown;
use crate::task_args::filter::Filters;
use crate::task_args::history::History;
use crate::task_args::modifier::Modifier;
use crate::task_args::project::Project;
use clap::builder::{IntoResettable, Resettable};
use clap::Args;
use clap::{Parser, Subcommand};
use std::env;
use std::fmt;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(about, long_about, version=None)]
pub struct Cli {
    /// Show this program's version and exit.
    /// Also shows taskwarrior's version if invoked in the long format `--version`.
    #[arg(short = 'V', long)]
    pub version: bool,

    /// Taskwarrior filter
    pub filter: Option<Filters>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand, Clone)]
pub enum Commands {
    Add {
        /// Modifiers
        mods: Vec<Modifier>,
    },
    All,
    /// Show all tasks, filtered by the project from the working dir
    Project,

    Annotate {
        /// Modifiers
        mods: Vec<Modifier>,
    },
    Append {
        /// Modifiers
        mods: Vec<Modifier>,
    },
    Blocked,
    Blocking,
    Burndown {
        burndown: Burndown,
    },
    Calc {
        /// Expression to calculate
        expression: Vec<String>,
    },
    Calendar {
        /// Extra args to pass to `task calendar`
        extra_args: Vec<String>,
    },
    Colors {
        /// Extra args to pass to `task colors`
        extra_args: Vec<String>,
    },
    Columns {
        /// Extra args to pass to `task columns`
        extra_args: Vec<String>,
    },
    Commands,
    Completed,
    Config {
        /// Extra args to pass to `task config`
        extra_args: Vec<String>,
    },
    Context {
        /// Extra args to pass to `task context`
        extra_args: Vec<String>,
    },
    Count,
    Delete {
        /// Modifiers
        mods: Vec<Modifier>,
    },
    Denotate {
        /// Extra args to pass to `task denotate`
        extra_args: Vec<String>,
    },
    Diagnostics,
    Done {
        /// Modifiers
        mods: Vec<Modifier>,
    },
    Duplicate {
        /// Modifiers
        mods: Vec<Modifier>,
    },
    Edit,
    Execute {
        /// Command to execute
        cmd: Vec<String>,
    },
    Export {
        /// Path to write report to
        report: PathBuf,
    },
    Ghistory {
        history: History,
    },
    /// Show taskwarrior's help message ('help' shows this program's help message)
    TaskHelp {
        /// Show only usage section
        #[arg(long, short)]
        usage: bool,
    },
    History {
        history: History,
    },
    Ids,
    Import {
        /// Files to import
        files: Vec<PathBuf>,
    },
    Information,
    Info,
    List,
    Log {
        /// Modifiers
        mods: Vec<Modifier>,
    },
    Logo,
    Long,
    Ls,
    Minimal,
    Modify {
        /// Modifiers
        mods: Vec<Modifier>,
    },
    Newest,
    News,
    Next,
    Oldest,
    Overdue,
    Prepend {
        /// Modifiers
        mods: Vec<Modifier>,
    },
    Projects,
    Purge,
    Ready,
    Recurring,
    Reports,
    Show {
        /// Extra args to pass to `task show`
        extra_args: Vec<String>,
    },
    Start {
        /// Modifiers
        mods: Vec<Modifier>,
    },
    Stats,
    Stop {
        /// Modifiers
        mods: Vec<Modifier>,
    },
    Summary,
    Synchronize {
        /// Extra args to pass to `task synchronize`
        extra_args: Vec<String>,
    },
    Tags,
    Timesheet,
    Udas,
    Unblocked,
    Undo,
    Uuids,
    Waiting,
    Rm {
        /// Modifiers
        mods: Vec<Modifier>,
    },
}

impl fmt::Display for Commands {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let repr = match self {
            Commands::Add { .. } => "add",
            Commands::All => "all",
            Commands::Project => "project",
            Commands::Annotate { .. } => "annotate",
            Commands::Append { .. } => "append",
            Commands::Blocked => "blocked",
            Commands::Blocking => "blocking",
            Commands::Burndown { burndown } => match burndown {
                Burndown::Daily => "burndown.daily",
                Burndown::Monthly => "burndown.monthly",
                Burndown::Weekly => "burndown.weekly",
            },
            Commands::Calc { .. } => "calc",
            Commands::Calendar { .. } => "calendar",
            Commands::Colors { .. } => "colors",
            Commands::Columns { .. } => "columns",
            Commands::Commands => "commands",
            Commands::Completed => "completed",
            Commands::Config { .. } => "config",
            Commands::Context { .. } => "context",
            Commands::Count => "count",
            Commands::Delete { .. } => "delete",
            Commands::Denotate { .. } => "denotate",
            Commands::Diagnostics => "diagnostics",
            Commands::Done { .. } => "done",
            Commands::Duplicate { .. } => "duplicate",
            Commands::Edit => "edit",
            Commands::Execute { .. } => "execute",
            Commands::Export { .. } => "export",
            Commands::Ghistory { history } => match history {
                History::Annual => "ghistory.annual",
                History::Daily => "ghistory.daily",
                History::Monthly => "ghistory.monthly",
                History::Weekly => "ghistory.weekly",
            },
            Commands::TaskHelp { .. } => "help",
            Commands::History { history } => match history {
                History::Annual => "history.annual",
                History::Daily => "history.daily",
                History::Monthly => "history.monthly",
                History::Weekly => "history.weekly",
            },
            Commands::Ids => "ids",
            Commands::Import { .. } => "import",
            Commands::Information | Commands::Info => "information",
            Commands::List => "list",
            Commands::Log { .. } => "log",
            Commands::Logo => "logo",
            Commands::Long => "long",
            Commands::Ls => "ls",
            Commands::Minimal => "minimal",
            Commands::Modify { .. } => "modify",
            Commands::Newest => "newest",
            Commands::News => "news",
            Commands::Next => "next",
            Commands::Oldest => "oldest",
            Commands::Overdue => "overdue",
            Commands::Prepend { .. } => "prepend",
            Commands::Projects => "projects",
            Commands::Purge => "purge",
            Commands::Ready => "ready",
            Commands::Recurring => "recurring",
            Commands::Reports => "reports",
            Commands::Show { .. } => "show",
            Commands::Stats => "stats",
            Commands::Stop { .. } => "stop",
            Commands::Summary => "summary",
            Commands::Synchronize { .. } => "synchronize",
            Commands::Tags => "tags",
            Commands::Timesheet => "timesheet",
            Commands::Udas => "udas",
            Commands::Unblocked => "unblocked",
            Commands::Undo => "undo",
            Commands::Uuids => "uuids",
            Commands::Waiting => "waiting",
            Commands::Start { .. } => "start",
            Commands::Rm { .. } => "rm",
        };
        write!(f, "{}", repr)
    }
}
