use std::{
    collections::{HashMap, HashSet},
    future::Future,
    process::ExitStatus,
    time::Instant,
};

use anyhow::Result;
use futures::{
    future::Either,
    stream::{FuturesUnordered, StreamExt},
};
use nix::{
    sys::signal::{kill, Signal},
    unistd::Pid,
};
use serde::{Deserialize, Deserializer};
use serde_yaml;
use shellwords;
use structopt::StructOpt;
use tokio::{
    process::Command,
    signal::unix::{signal, SignalKind},
};

/* The command specification, along with an alias for its "collection type"
*/
#[derive(Deserialize)]
struct CmdSpec {
    #[serde(deserialize_with = "get_argv_from_str")]
    argv: Vec<String>,
    #[serde(skip_deserializing)]
    pid: Option<Pid>,
}

type Cmds = HashMap<String, CmdSpec>;

/* The command specification's deserialization helper
*/
fn get_argv_from_str<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    String::deserialize(deserializer)
        .and_then(|cmd| shellwords::split(&cmd).map_err(|_| Error::custom("mismatched quotes")))
}

/* The output type of the future returned when processes are spawned.
*/
struct CompletedCmd {
    name: String,
    started_at: Instant,
    exit_status: ExitStatus,
}

/* Helper to spawn a process and return its future. The future provided by
   tokio::Process doesn't return everything needed to process the termination,
   so this augments it with some extra items: the start time and the textual
   identifier of the command. To do it, this maps the future to another
   future: an anonymous async function that takes ownership of the data to
   be saved and then awaits the "real" future (Tokio's Child). On completion,
   the result is mapped to an instance of CompletedCmd.
*/
fn get_cmd_future(
    name: &str,
    cmd: &mut CmdSpec,
) -> Result<impl Future<Output = Result<CompletedCmd, std::io::Error>>, std::io::Error> {
    Command::new(&cmd.argv[0])
        .args(&cmd.argv[1..])
        .spawn()
        .map(|r| {
            cmd.pid = Some(Pid::from_raw(r.id() as i32));
            let name = String::from(name);
            let started_at = Instant::now();
            async move {
                r.await.map(|exit_status| CompletedCmd {
                    name,
                    started_at,
                    exit_status,
                })
            }
        })
}

/* Read the commands to run from a YAML file into a commands collection.
*/
fn read_cmds(path: &str) -> Result<Cmds> {
    Ok(serde_yaml::from_reader(std::fs::File::open(path)?)?)
}

/* This is a closure for the filter_map function. It allows the futures
   Vec builder to return only a list of commands that were spawned successfully
   The failed ones are filtered out here along with a warning being printed.
*/
fn only_ok(
    spawn_result: Result<
        impl Future<Output = Result<CompletedCmd, std::io::Error>>,
        std::io::Error,
    >,
) -> Option<impl Future<Output = Result<CompletedCmd, std::io::Error>>> {
    match spawn_result {
        Ok(process) => Some(process),
        Err(e) => {
            println!("{:?}", e);
            None
        }
    }
}

/* TODO: Refactoring the start logic into a function cleanly depends on being
  able to use the unstable type_alias_impl_trait feature which allows the
  command future to be a specific type rather than an opaque one that cannot
  be guaranteed to be the right type in all the places where it is passed
  around. For platforms where a nightly build is not available, another
  techinque needs to be used, like simply repeating the code, which is what
  is done currently.
*/

/* A convenience function for stopping processes
*/
fn stop_process(cmd_name: &str, cmds: &mut Cmds) {
    match cmds.get(cmd_name).unwrap().pid {
        Some(pid) => match kill(pid, Signal::SIGTERM) {
            Ok(()) => {
                println!("stopping {} (pid: {})", cmd_name, pid);
                cmds.remove(cmd_name);
            }
            Err(e) => eprintln!("error signaling process: {:?}", e),
        },
        None => eprintln!("{} not running", cmd_name),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[derive(Debug, StructOpt)]
    #[structopt(name = "boss", about = "Process manager")]
    struct Opt {
        /// Path to configuration file
        #[structopt(short, long, default_value = "boss.yaml")]
        config_file: String,
    }
    let opt = Opt::from_args();

    let mut cmds = read_cmds(&opt.config_file)?;

    let mut hangups = signal(SignalKind::hangup())?;

    /* All commands become part of a FutureUnordered stream which is populated
    in two places: from a Vec of futures here, at startup, and by pushing
    individual futures later, after the processes finish. The specific
    types are inferred by the return signatures of the `get_cmd_future()
    and `only_ok()` functions. Even though the types look the same, they
    are two different types in view of the type system.  Because of this,
    the `Either` wrapper type must be used to accomodate both of them. The
    other way to handle the type variability is via using BoxFutures but
    Either doesn't involve a heap allocation.
    */
    let mut all_futures: FuturesUnordered<_> = cmds
        .iter_mut()
        .map(|(name, cmd)| get_cmd_future(name, cmd))
        .filter_map(only_ok)
        .map(|ok| Either::Left(ok))
        .collect();

    loop {
        tokio::select! {
            /* Process the receipt of the HUP signal. */
            _ = hangups.recv() => {
                match read_cmds(&opt.config_file) {
                    Ok(mut new_cmds) => {
                        let cur_cmd_names: HashSet<String> = cmds.keys().cloned().collect();
                        let updated_cmd_names: HashSet<String> = new_cmds.keys().cloned().collect();
                        let mut changes = false;

                        /* Stop commands that have been removed from the list. */
                        for cmd_name in cur_cmd_names.difference(&updated_cmd_names) {
                            changes = true;
                            stop_process(cmd_name, &mut cmds);
                        }

                        /* Start newly-added commands. */
                        for cmd_name in updated_cmd_names.difference(&cur_cmd_names) {
                            changes = true;
                            let mut cmd = new_cmds.remove(cmd_name).unwrap();
                            match get_cmd_future(cmd_name, &mut cmd) {
                                Ok(spawned_child) => {
                                    println!("starting {}", cmd_name);
                                    all_futures.push(Either::Right(spawned_child));
                                    cmds.insert(cmd_name.to_string(), cmd);
                                }
                                Err(e) => println!("spawn failed: {:?}", e),
                            }
                        }

                        /* Stop commands that have been updated, re-inserting the
                           new argument vector back into the set. These will be
                           restarted with the revised args when the current ones finish.
                        */
                        for cmd_name in cur_cmd_names.intersection(&updated_cmd_names) {
                            if cmds.get(cmd_name).unwrap().argv != new_cmds.get(cmd_name).unwrap().argv {
                                changes = true;
                                let cmd = new_cmds.remove(cmd_name).unwrap();
                                stop_process(cmd_name, &mut cmds);
                                cmds.insert(cmd_name.to_string(), cmd);
                            }
                        }
                        if !changes { println!("no changes to commands") }
                   },
                   Err(e) => eprintln!("error re-reading config: {:?}", e),
                }
            },

            /* Process command terminations. The resolved future here is the
               next item of the FuturesUnordered stream. These items are a
               two level construct: an Option that contains a Result.
            */
            completed_process = all_futures.next() => {
                /* The first level (the Option) is either an actual Result of one of
                   the Child futures, or the None value indicating end of stream. In
                   practice, this would only be reached if the user removed all the
                   commands from the config, which is unlikely.
                */
                match completed_process {
                    Some(result) => {
                       /* The second level (the Result) is the final disposition of the process.
                          Both zero and non-zero exit statuses come through the Ok case, so it's
                          unclear how the Err case happens. But it's possible to get a
                          std::io::Error here.
                       */
                        match result {
                            Ok(child) => {
                                let result = match child.exit_status.code() {
                                    Some(code) => format!("exited with status {}", code),
                                    None => format!("terminated by signal")
                                };
                                println!(
                                    "{}: {}, after {} sec.",
                                    child.name,
                                    result,
                                    child.started_at.elapsed().as_secs(),
                                );
                                match cmds.get_mut(&child.name) {
                                    Some(cmd) => match get_cmd_future(&child.name, cmd) {
                                        Ok(spawned_child) => {
                                            println!("restarting: {}", child.name);
                                            all_futures.push(Either::Right(spawned_child))
                                        }
                                        Err(e) => println!("spawn failed: {:?}", e),
                                    },
                                    None => println!("final invocation of : {}", child.name),
                                }
                            }
                            /* TODO: the Error variant doesn't contain the command
                               information (specifically the name), it wouldn't be
                               possible to restart here, or print the name of which
                               command failed. Need to map the Error associated type
                               for the future.
                            */
                            Err(e) => eprintln!("error with process spawn: {:?}", e),
                        }
                    }
                    None => {
                        println!("All processes finished");
                        break;
                    }
                }
            },
        }
    }
    Ok(())
}
