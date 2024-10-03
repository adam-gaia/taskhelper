use clap::{Parser as ClapParser, Subcommand};
use color_eyre::eyre::bail;
use color_eyre::Result;
use core::fmt::Error;
use futures_util::pin_mut;
use libc::winsize;
use log::debug;
use log::info;
use log::trace;
use nix::pty::openpty;
use nix::pty::OpenptyResult;
use nix::pty::Winsize;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::close;
use nix::unistd::dup2;
use nix::unistd::fork;
use nix::unistd::ForkResult;
use serde::Serialize;
use std::env;
use std::fmt;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::fd::FromRawFd;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::str;
use terminal_size::{terminal_size, Height, Width};

mod args;
use args::{Cli, Commands};
mod task_args;
use log::warn;
use task_args::filter::{Filter, Filters};
use task_args::modifier::Modifier;
use task_args::project::Project;

const TASK_BIN: &'static str = "task";
const DEFAULT_TERM_SIZE: (u16, u16) = (80, 24);
const SUPPORTED_TASKWARRIOR_VERSION: &'static str = "3.1.0";
const VERSION: &'static str = env!("CARGO_PKG_VERSION");
const NAME: &'static str = env!("CARGO_BIN_NAME");
const DESCRIPTION: &'static str = env!("CARGO_PKG_DESCRIPTION");

// TODO: stdin
// TODO: add 'open' subcommand that runs 'taskopen'. Add taskopen to flake deps
// TODO: page long outputs (maybe make this a config option to enable/disable and set pager?)

fn has_git_dir(path: &Path) -> bool {
    let git_dir = path.join(".git");
    git_dir.is_dir()
}

fn project_name_from_path(path: &Path) -> String {
    path.file_name().unwrap().to_str().unwrap().to_string()
}

fn find_project() -> Result<Option<Project>> {
    let mut cwd = env::current_dir()?;
    loop {
        if has_git_dir(&cwd) {
            let name = project_name_from_path(&cwd);
            let project = Project::with_name(&name);
            return Ok(Some(project));
        }

        let Some(parent) = cwd.parent() else {
            break;
        };
        cwd = parent.to_path_buf();
    }

    Ok(None)
}

fn winsize() -> Winsize {
    let (cols, rows) = match terminal_size() {
        Some((Width(w), Height(h))) => (w as u16, h as u16),
        None => DEFAULT_TERM_SIZE,
    };
    Winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    }
}

#[derive(Debug)]
struct CommandResult {
    stdout: String,
    stderr: String,
    code: i32,
}

fn run(exec: &Path, args: &[String]) -> Result<CommandResult> {
    let mut cmd = Command::new(exec);
    cmd.args(args);

    debug!("Running command {:?}", cmd);

    let winsize = winsize();
    let pty = openpty(&Some(winsize), None)?;
    let master = pty.master.as_raw_fd();
    let slave = pty.slave.as_raw_fd();
    // Stop file descriptor from closing on drop
    std::mem::forget(pty);

    let output = match unsafe { fork() } {
        Ok(res) => {
            match res {
                ForkResult::Parent { child, .. } => {
                    // We are the parent
                    trace!("Parent: spawned child with PID {}", child);
                    close(slave)?;

                    let mut f = unsafe { File::from_raw_fd(master) };
                    let mut buffer = String::new();

                    let code = match waitpid(child, None) {
                        Ok(status) => match status {
                            WaitStatus::Exited(_, code) => code,
                            WaitStatus::Signaled(_, signal, _) => signal as i32,
                            WaitStatus::Stopped(_, signal) => signal as i32,
                            _ => bail!("Unexpected wait status: {:?}", status),
                        },
                        Err(e) => {
                            bail!("waitpid failed: {}", e)
                        }
                    };

                    // It seems that the read_to_string call will fail on EOF. Ignore the result
                    // See https://stackoverflow.com/a/72159292
                    let _ = f.read_to_string(&mut buffer);

                    CommandResult {
                        stdout: buffer,
                        stderr: String::from("TODO"),
                        code,
                    }
                }
                ForkResult::Child => {
                    // We are the child
                    // Set up the child process to use the PTY
                    let slave_fd = slave.as_raw_fd();
                    dup2(slave_fd, libc::STDIN_FILENO).expect("Failed to duplicate to stdin");
                    dup2(slave_fd, libc::STDOUT_FILENO).expect("Failed to duplicate to stdout");
                    dup2(slave_fd, libc::STDERR_FILENO).expect("Failed to duplicate to stderr");

                    let e = cmd.exec();
                    // If we get this far, the exec failed
                    bail!("Exec failed: {:?}", e);
                }
            }
        }
        Err(e) => bail!("Fork failed: {:?}", e),
    };

    Ok(output)
}

fn no_filter(command: &Commands, filters: &Option<Filters>) -> Result<()> {
    if filters.is_some() {
        bail!(
            "Subcommand '{}' does not allow preceding filters",
            command.to_string()
        );
    }
    Ok(())
}

fn task_version(task_bin: &Path) -> Result<String> {
    let output = Command::new(task_bin).arg("--version").output()?.stdout;
    let s = str::from_utf8(&output)?;
    Ok(s.trim().to_string())
}

#[derive(Debug)]
enum Index {
    Index(usize),
    End,
}

fn set_project(project_provided: bool, args: &mut Vec<String>, index: Index) -> Result<()> {
    if !project_provided {
        if let Some(project) = find_project()? {
            let project_name = project.name();
            info!("Found project '{}' from cwd ansestory", project_name);
            match index {
                Index::Index(i) => args.insert(i, project.to_string()),
                Index::End => {
                    args.push(project.to_string());
                }
            }
        }
    }
    Ok(())
}
use std::ffi::OsString;
use std::fs;

/// Find task bin on the path, make sure it isn't this program (this program can be invoked under the name 'task')
fn find_taskwarrior(this_program: &Path) -> Result<PathBuf> {
    let Ok(matches) = which::which_all(TASK_BIN) else {
        bail!("Unable to find taskwarrior ('task') on the $PATH");
    };

    // This program is a multicall binary which mimics taskwarrior if called under the name 'task'.
    // It is likely the first bin nammed 'task' on the $PATH is this program, so loop until we find another that isn't this program
    for m in matches {
        let m = fs::canonicalize(m)?;
        trace!("Checking if '{}' is taskwarrior", m.display());
        if m != this_program {
            trace!("Using '{}' as taskwarrior", m.display());
            return Ok(m);
        } else {
            trace!("Found ourself in the path. Skipping");
        }
    }
    bail!("Unable to find taskwarrior ('task') on the $PATH");
}

fn main() -> Result<()> {
    color_eyre::install()?;
    env_logger::init();

    // Do some initial processing of args before passing off to clap to handle multicall
    let args: Vec<String> = std::env::args().collect();
    let this_program = PathBuf::from(&args[0]);
    let this_program = fs::canonicalize(this_program)?;
    trace!("This program: {}", this_program.display());
    let task_bin = find_taskwarrior(&this_program)?;

    let taskwarrior_version = task_version(&task_bin)?;
    let version_compat = &taskwarrior_version == SUPPORTED_TASKWARRIOR_VERSION;

    let name = this_program.file_name().unwrap();
    debug!("name: {:?}", name);
    if name == OsString::from("task") {
        // Mimic taskwarrior when invoked under 'task'. We do this by exec-ing taskwarrior and passing args unmodified
        let task_args: Vec<String> = std::env::args().skip(1).collect();
        let res = run(&task_bin, &task_args)?;
        print!("{}", res.stdout);
        std::process::exit(res.code);
    }

    match args[1].as_str() {
        "--version" => {
            let compatibility = if version_compat {
                "compatible"
            } else {
                "incompatible"
            };
            println!(
                "{}: {}, {}: {} ({})",
                NAME, VERSION, TASK_BIN, taskwarrior_version, compatibility
            );
            std::process::exit(0);
        }
        "-V" => {
            println!("{}", VERSION);
            std::process::exit(0);
        }
        _ => {}
    }

    if !version_compat {
        warn!(
            "Unsupported taskwarrior version {} found, but this program supports {}. Will continue anyways...",
            taskwarrior_version, SUPPORTED_TASKWARRIOR_VERSION
        );
    }

    let mut task_args = Vec::new();

    let mut project_filter_provided = false;
    let mut project_mod_provided = false;

    let args = Cli::parse_from(args);
    let filters = args.filter;
    if let Some(filters) = &filters {
        for filter in filters.filters() {
            match filter {
                // TODO: don't use match use let Filter::Project()
                Filter::Project(ref project) => {
                    project_filter_provided = true;
                }
                _ => {
                    // do nothing
                }
            }

            task_args.push(filter.to_string())
        }
    }

    match args.command {
        Some(command) => {
            // Add the subcommand after any filters
            task_args.push(command.to_string());

            match &command {
                Commands::Add { mods } => {
                    no_filter(&command, &filters)?;

                    for r#mod in mods {
                        // TODO dont use match use let Modifier::Project()
                        match r#mod {
                            Modifier::Project(ref project) => {
                                project_mod_provided = true;
                            }
                            _ => {
                                // do nothing
                            }
                        }
                        task_args.push(r#mod.to_string());
                    }

                    // Set the project as the final argument, making it the last modifier
                    set_project(project_mod_provided, &mut task_args, Index::End)?;
                }
                Commands::All => {
                    // Do nothing, pass args unmodified to taskwarrior. This won't pickup a project from the cwd ansestory
                }
                Commands::Blocked
                | Commands::Blocking
                | Commands::Completed
                | Commands::Count
                | Commands::Edit
                | Commands::Ids
                | Commands::Info
                | Commands::Information
                | Commands::Long
                | Commands::Ls
                | Commands::Minimal
                | Commands::Newest
                | Commands::Next
                | Commands::Oldest
                | Commands::Overdue
                | Commands::Projects
                | Commands::List
                | Commands::Purge
                | Commands::Recurring
                | Commands::Stats
                | Commands::Summary
                | Commands::Tags
                | Commands::Timesheet
                | Commands::Unblocked
                | Commands::Uuids
                | Commands::Waiting
                | Commands::Ready
                | Commands::Burndown { .. }
                | Commands::Ghistory { .. }
                | Commands::History { .. } => {
                    // Set project as the first arg, to make the first filter
                    set_project(project_filter_provided, &mut task_args, Index::Index(0))?;
                }
                Commands::Project => {
                    if project_filter_provided {
                        bail!("Usage error: project filter cannot be provided with 'project' subcommand");
                    }
                    set_project(false, &mut task_args, Index::Index(1))?;
                }
                Commands::Start { mods }
                | Commands::Stop { mods }
                | Commands::Prepend { mods }
                | Commands::Modify { mods }
                | Commands::Log { mods }
                | Commands::Done { mods }
                | Commands::Duplicate { mods }
                | Commands::Append { mods }
                | Commands::Annotate { mods }
                | Commands::Delete { mods }
                | Commands::Rm { mods } => {
                    for r#mod in mods {
                        // TODO dont use match use let Modifier::Project()
                        match r#mod {
                            Modifier::Project(ref project) => {
                                project_mod_provided = true;
                            }
                            _ => {
                                // do nothing
                            }
                        }
                        task_args.push(r#mod.to_string());
                    }

                    // Set the project as the final argument, making it the last modifier
                    set_project(project_mod_provided, &mut task_args, Index::End)?;
                }
                Commands::Calc { expression } => {
                    no_filter(&command, &filters)?;
                    task_args.extend_from_slice(&expression);
                }
                Commands::Calendar { extra_args }
                | Commands::Colors { extra_args }
                | Commands::Columns { extra_args }
                | Commands::Config { extra_args }
                | Commands::Context { extra_args }
                | Commands::Show { extra_args }
                | Commands::Synchronize { extra_args } => {
                    no_filter(&command, &filters)?;
                    task_args.extend_from_slice(&extra_args);
                }

                Commands::Denotate { extra_args } => {
                    task_args.extend_from_slice(&extra_args);
                }
                Commands::Execute { cmd } => {
                    no_filter(&command, &filters)?;
                    task_args.extend_from_slice(&cmd);
                }
                Commands::Export { report } => task_args.push(report.display().to_string()),
                Commands::TaskHelp { usage } => {
                    no_filter(&command, &filters)?;
                    if *usage {
                        task_args.push(String::from("usage"));
                    }
                }
                Commands::Import { files } => {
                    no_filter(&command, &filters)?;
                    let files: Vec<String> =
                        files.iter().map(|f| f.display().to_string()).collect();
                    task_args.extend_from_slice(&files);
                }
                Commands::Undo
                | Commands::Udas
                | Commands::Reports
                | Commands::Diagnostics
                | Commands::Commands
                | Commands::Logo
                | Commands::News => {
                    no_filter(&command, &filters)?;
                }
            }
        }
        None => {
            //
        }
    }

    let res = run(&task_bin, &task_args)?;
    let code = res.code;
    print!("{}", res.stdout);

    /*
    let Ok(child) = XCommand::builder(&task_bin)
        //.args(&["help", "usage"])?
        .build()
        .spawn()
    else {
        bail!("Unable to run '{}'", task_bin.display());
    };

    let stream = child.stream();
    pin_mut!(stream);
    while let Some(output) = stream.next().await {
        let (source, line) = output.unwrap();
        match source {
            StdioType::Stdout => {
                println!("[stdout]{}", line);
            }
            StdioType::Stderr => {
                println!("[stderr]{}", line);
            }
        }
    }
    */

    //let code = run(&task_bin, &processed_args).await?;

    std::process::exit(code)
}
