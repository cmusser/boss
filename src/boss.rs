extern crate boss;
extern crate clap;
extern crate daemonize;
extern crate futures;
extern crate hyper;
extern crate tokio;
extern crate tokio_core;
extern crate tokio_process;

use std::fs::File;
use std::net::SocketAddr;
use std::sync::Arc;

use boss::data::{Boss, LaunchResult};
use clap::{App, Arg};
use daemonize::Daemonize;
use hyper::{Body, Response, Server};
use hyper::http::StatusCode;
use hyper::service::service_fn_ok;
use hyper::rt::{self, Future};

const VERSION: &'static str = "0.1.0";
const DEFAULT_CONFIG_FILE: &'static str = "/etc/boss.yaml";
const DEFAULT_DAEMON_STDOUT_FILENAME: &'static str = "/var/log/boss.out";
const DEFAULT_DAEMON_STDERR_FILENAME: &'static str = "/var/log/boss.err";

macro_rules! non_ok_response {
    ($status:expr, $msg:expr) => {
    Response::builder().status($status).body(Body::from($msg)).unwrap()
    }
}

fn run(boss: Boss) {
    let addr: SocketAddr = boss.listen_addr.parse().unwrap();
    let boss = Arc::new(boss);
    let boss_service = move || {

        let boss_for_start = boss.clone();
        service_fn_ok(move |req| {
            let client = String::from(req.uri().path());
            match boss_for_start.start(&client) {
                LaunchResult::Launched(command) => {
                    let boss_for_cleanup = boss_for_start.clone();
                    let command_complete = command
                        .map(|status| { (status, boss_for_cleanup) })
                        .then(move|args| {
                            let (status, boss) = args.unwrap();
                            println!("command for \"{}\" has terminated with status {}", client, status);
                            boss.cleanup(&client);
                            futures::future::ok(())
                        });
                    rt::spawn(command_complete);
                    Response::new(Body::from("available\n"))
                },
                LaunchResult::AlreadyRunning => Response::new(Body::from("available\n")),
                LaunchResult::ClientNotFound => non_ok_response!(StatusCode::NOT_FOUND,
                                                                 format!("no client \"{}\"", client)),
                LaunchResult::Err => non_ok_response!(StatusCode::INTERNAL_SERVER_ERROR,
                                                      "couldn't start"),
            }
        })
    };
    
    let server = Server::bind(&addr).serve(boss_service)
        .map_err(|e| eprintln!("server error: {}", e));
    println!("Listening on http://{}", addr);
    rt::run(server);
}

fn run_daemon(boss: Boss) -> Result<(), String> {
    let stdout = File::create(DEFAULT_DAEMON_STDOUT_FILENAME)
        .map_err(|e| format!("couldn't open {} -- {}",
                             DEFAULT_DAEMON_STDOUT_FILENAME, e))?;
    let stderr = File::create(DEFAULT_DAEMON_STDERR_FILENAME)
                .map_err(|e| format!("couldn't open {} -- {}",
                             DEFAULT_DAEMON_STDERR_FILENAME, e))?;
    let daemonize = Daemonize::new().pid_file("/var/run/boss.pid")
                            .stdout(stdout).stderr(stderr);
    daemonize.start().map_err(|e| format!("couldn't daemonize -- {}", e))?;
    Ok(run(boss))
}

fn main() {
    let matches = App::new("boss")
        .version(VERSION)
        .author("Chuck Musser <cmusser@sonic.net>")
        .about("start processes on behalf of network clients")
        .arg(Arg::with_name("config").empty_values(false)
             .short("c").long("config")
             .help("YAML file containing configuration")
             .default_value(DEFAULT_CONFIG_FILE))
        .arg(Arg::with_name("foreground")
             .short("f").long("foreground")
             .help("Run in foreground"))
        .get_matches();

    match Boss::new(matches.value_of("config").unwrap()) {
        Ok(boss) => {
            if matches.is_present("foreground") {
                run(boss)
            } else {
                if let Err(e) = run_daemon(boss) { eprintln!("failed: {}", e) }
            }
        },
        Err(e) => eprintln!("{}", e),
    }
}
