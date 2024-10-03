use async_stream::stream;
use color_eyre::eyre::bail;
use color_eyre::owo_colors::OwoColorize;
use color_eyre::Result;
use fern::Dispatch;
use log::debug;
use log::error;
use log::info;
use nix::pty::openpty;
use nix::pty::OpenptyResult;
use nix::sys::signal::Signal;
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::dup2;
use nix::unistd::execve;
use nix::unistd::pipe;
use nix::unistd::setsid;
use nix::unistd::ForkResult;
use nix::unistd::{close, fork, Pid};
use std::env;
use std::ffi::CString;
use std::fs::File;
use std::mem;
use std::os::fd::AsRawFd;
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::FromRawFd;
use std::os::unix::prelude::RawFd;
use std::path::Path;
use std::pin::Pin;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio_fd::AsyncFd;
use tokio_stream::wrappers::LinesStream;
use tokio_stream::{Stream, StreamExt, StreamMap};
use which::which;

fn path_to_cstring(path: &Path) -> CString {
    let bytes = path.as_os_str().as_bytes();
    CString::new(bytes).unwrap()
}

#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone)]
pub enum StdioType {
    Stdout,
    Stderr,
}

pub enum XStatus {
    Exited(i32),
    Signaled(Signal),
}

pub struct XChildHandle {
    pid: Pid,
    stdout: i32,
    stderr: i32,
}

impl XChildHandle {
    fn new(pid: Pid, stdout: i32, stderr: i32) -> Result<Self> {
        Ok(XChildHandle {
            pid,
            stdout,
            stderr,
        })
    }
    pub fn pid(&self) -> Pid {
        self.pid
    }

    pub fn stream(&self) -> impl Stream<Item = Result<(StdioType, String), std::io::Error>> + '_ {
        stream! {
            let child_pid = self.pid;

            let mut join = tokio::task::spawn_blocking(move || {
                // TODO: I think we can use a regular spawn instead of a non-blocking spawn if we pass
                // 'WaitPidFlag::WNOHANG' to waitpid() and loop over waitpid() calls, continuing when
                // we match WaitStatus::StillAlive
                // https://docs.rs/nix/latest/nix/sys/wait/enum.WaitStatus.html
                    let Ok(status) = waitpid(child_pid, None) else {
                        panic!("Error waiting for child to complete");
                    };
                    debug!("Child status: {:?}", status);
                    status
            });

            let stdout = AsyncFd::try_from(self.stdout).unwrap();
            let stderr = AsyncFd::try_from(self.stderr).unwrap();
            let mut stdout_reader = LinesStream::new(BufReader::new(stdout).lines());
            let mut stderr_reader = LinesStream::new(BufReader::new(stderr).lines());

            let stdout_stream = Box::pin(stream! {
                while let Some(Ok(item)) = stdout_reader.next().await {
                    yield item;
                }
            })
                as Pin<Box<dyn Stream<Item = String> + Send>>;

            let stderr_stream = Box::pin(stream! {
                while let Some(Ok(item)) = stderr_reader.next().await {
                    yield item;
                }
            })
                as Pin<Box<dyn Stream<Item = String> + Send>>;

            let mut map = StreamMap::with_capacity(2);
            map.insert(StdioType::Stdout, stdout_stream);
            map.insert(StdioType::Stderr, stderr_stream);

            loop {
                tokio::select! {
                    // Force polling in listed order instead of randomly. This prevents us from
                    // deadlocking when the command exits. - TODO: this might not be needed anymore
                    biased;
                    Some(output) = map.next() => {
                        yield Ok(output);
                    },
                    status = &mut join => {
                        let status = status.unwrap(); // TODO: handle unwrap

                        // Pick up any final output that was written in the time it took us to check
                        // this 'select!' branch
                        while let Some(output) = map.next().await {
                            yield Ok(output);
                        }

                        close(self.stdout).unwrap();
                        close(self.stderr).unwrap();

                        // TODO: do we need to handle any other WaitStatus variants?
                        // https://docs.rs/nix/latest/nix/sys/wait/enum.WaitStatus.html
                        match status {
                            WaitStatus::Exited(pid, return_code) => {
                                debug!("Child exited with return code {}", return_code);
                                //return Ok(XStatus::Exited(return_code))
                                // TODO: how do we return the child's status?
                                return;
                            },
                            WaitStatus::Signaled(pid, signal, _) => {
                                debug!("Child was killed by signal {:?}", signal);
                                //return Ok(XStatus::Signaled(signal))
                                return;
                            },
                            _ => {
                                panic!("Child process in unexpected state: '{:?}'", status);
                            },
                        }
                    },
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct EnvVar {
    key: CString,
    value: CString,
}

#[derive(Debug)]
pub struct XCommand {
    command: CString,
    args: Vec<CString>,
    env: Vec<EnvVar>,
}

impl XCommand {
    pub fn builder<P: AsRef<Path>>(command: P) -> XCommandBuilder {
        XCommandBuilder::new(command)
    }

    /// Replace the current process with the executed command
    fn exec(&self) -> Result<()> {
        // Prepend the comnand name to the array of args
        let mut args = self.args.clone();
        args.insert(0, self.command.clone());

        // Format each variable as 'key=value'
        let env: Vec<CString> = self
            .env
            .iter()
            .map(|var| {
                let k = var.key.clone();
                let v = var.value.clone();
                let key_bytes = k.as_bytes();
                let eq_bytes = "=".as_bytes();
                let value_bytes = v.as_bytes();
                let mut formatted =
                    Vec::with_capacity(key_bytes.len() + value_bytes.len() + eq_bytes.len());
                formatted.extend_from_slice(key_bytes);
                formatted.extend_from_slice(eq_bytes);
                formatted.extend_from_slice(value_bytes);
                CString::new(formatted).unwrap()
            })
            .collect();

        // Cannot call println or unwrap in child - see
        // https://docs.rs/nix/0.25.0/nix/unistd/fn.fork.html#safety
        //nix::unistd::write(libc::STDOUT_FILENO, "I'm a new child process - stdout\n".as_bytes()).ok();
        //nix::unistd::write(libc::STDERR_FILENO, "I'm a new child process - stderr\n".as_bytes()).ok();

        match execve(&self.command, &args, &env) {
            Ok(_) => {}
            Err(e) => {
                bail!(
                    "Unable to execve command '{:?}' with args {:?}. Reason: {}",
                    self.command,
                    args,
                    e
                );
            }
        }
        Ok(())
    }

    pub fn spawn(&self) -> Result<XChildHandle> {
        debug!("Running '{:?}' with args {:?}", self.command, self.args);
        // Open two ptys, one for stdout and one for stderr
        // This seems ludicrous however I cannot find a way to seprately send both streams and
        // fake a pty.
        // This SO question summs it up
        // https://stackoverflow.com/questions/34186035/can-you-fool-isatty-and-log-stdout-and-stderr-separately

        let res = openpty(None, None)?;
        let master = res.master;
        let slave = res.slave;
        let stdout_master = master.as_raw_fd();
        let stdout_slave = slave.as_raw_fd();
        // Stop drop from closing descriptors
        mem::forget(master);
        mem::forget(slave);

        let res = openpty(None, None)?;
        let master = res.master;
        let slave = res.slave;
        let stderr_master = master.as_raw_fd();
        let stderr_slave = slave.as_raw_fd();
        // Stop drop from closing descriptors
        mem::forget(master);
        mem::forget(slave);

        let Ok(res) = (unsafe { fork() }) else {
            bail!("fork() failed");
        };

        match res {
            ForkResult::Parent { child } => {
                // We are the parent
                close(stdout_slave).unwrap();
                close(stderr_slave).unwrap();
                // Return a handle to the child
                Ok(XChildHandle::new(child, stdout_master, stderr_master).unwrap())
            }
            ForkResult::Child => {
                // We are the child
                close(stdout_master).unwrap();
                close(stderr_master).unwrap();

                /*
                setsid()?;
                let _ = unsafe { libc::ioctl(stdout_slave, libc::TIOCSCTTY, libc::STDOUT_FILENO) };
                let _ = unsafe { libc::ioctl(stderr_slave, libc::TIOCSCTTY, libc::STDERR_FILENO) };
                */

                // Redirect the pty stdout/err to this process's stdout/err
                dup2(stdout_slave.as_raw_fd(), libc::STDOUT_FILENO).unwrap();
                dup2(stderr_slave.as_raw_fd(), libc::STDERR_FILENO).unwrap();

                // TODO: pass through stdin

                //Exec the command
                let Err(e) = self.exec() else {
                    unreachable!();
                };

                error!("failed to exec: {}", e);
                // TODO: set exit code based on error
                std::process::exit(1);
            }
        }
    }
}

pub struct XCommandBuilder {
    command: CString,
    args: Vec<CString>,
    env: Vec<EnvVar>,
}

impl XCommandBuilder {
    pub fn new<P: AsRef<Path>>(command: P) -> Self {
        let path = command.as_ref();
        XCommandBuilder {
            command: path_to_cstring(path),
            args: Vec::new(),
            env: Vec::new(),
        }
    }

    /// Set an argument for the process
    pub fn arg(mut self, arg: &str) -> Result<Self> {
        let Ok(arg) = CString::new(arg) else {
            bail!("Unable to create CString from '{}'", arg);
        };
        self.args.push(arg);
        Ok(self)
    }

    /// Set the args of the process
    /// (Replaces any currently assigned args)
    pub fn args(mut self, args: &[&str]) -> Result<Self> {
        let mut cstr_args = Vec::with_capacity(args.len());
        for arg in args {
            let Ok(arg) = CString::new(*arg) else {
                bail!("Unable to create CString from '{}'", arg);
            };
            cstr_args.push(arg);
        }

        self.args = cstr_args;
        Ok(self)
    }

    /// Build a XCommand
    pub fn build(self) -> XCommand {
        XCommand {
            command: self.command,
            args: self.args,
            env: self.env,
        }
    }
}

/*
pub async fn run<P: AsRef<Path>>(command: P, args: &[String]) -> Result<i32> {
    let command: &Path = command.as_ref();
    let mut cmd = Command::new(command);

    // Create a oneshot to get the status code of the child process back to our main task
    let (tx, rx) = oneshot::channel();

    cmd.stdout(Stdio::piped());
    if args.len() > 0 {
        cmd.args(args);
    }
    debug!("{:?}", cmd);

    let Ok(mut child) = cmd.spawn() else {
        bail!("Failed to spawn command '{}'", command.display());
    };

    let Some(stdout) = child.stdout.take() else {
        bail!(
            "Unable to get a handle to child process' ({}) stdout",
            command.display()
        );
    };

    let mut reader = BufReader::new(stdout).lines();
    tokio::spawn(async move {
        let status = child
            .wait()
            .await
            .expect("Child process encountered an error");
        // Send the status to the main task
        tx.send(status)
            .expect("Error sending the status to the main task");
    });

    while let Some(line) = reader.next_line().await? {
        println!("{}", line);
    }

    let Ok(status) = rx.await else {
        bail!("Unable to read status code from child process");
    };
    let Some(code) = status.code() else {
        bail!("Unablae to get status code from child process");
    };
    Ok(code)
}
*/
