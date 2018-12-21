extern crate boss;
extern crate clap;
extern crate futures;
extern crate hyper;
extern crate tokio;
extern crate tokio_core;
extern crate tokio_process;

use std::net::SocketAddr;
use std::sync::Arc;

use boss::data::{Boss, LaunchResult};
use clap::{App, Arg};
use hyper::{Body, Response, Server};
use hyper::http::StatusCode;
use hyper::service::service_fn_ok;
use hyper::rt::{self, Future};

const VERSION: &'static str = "0.1.0";
const DEFAULT_CONFIG_FILE: &'static str = "/etc/boss.yaml";

macro_rules! non_ok_response {
    ($status:expr, $msg:expr) => {
    Response::builder().status($status).body(Body::from($msg)).unwrap()
    }
}

fn run(config: Arc<Boss>) {
    let addr: SocketAddr = config.listen_addr.parse().unwrap();
    let boss_service = move || {

        let config_for_start = config.clone();
        service_fn_ok(move |req| {
            let client = String::from(req.uri().path());
            match config_for_start.start(&client) {
                LaunchResult::Launched(command) => {
                    let config_for_cleanup = config_for_start.clone();
                    let command_complete = command
                        .map(|status| { (status, config_for_cleanup) })
                        .then(move|args| {
                            let (status, config) = args.unwrap();
                            println!("command for \"{}\" has terminated with status {}", client, status);
                            config.cleanup(&client);
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

fn main() {
    let matches = App::new("boss")
        .version(VERSION)
        .author("Chuck Musser <cmusser@sonic.net>")
        .about("start processes on behalf of network clients")
        .arg(Arg::with_name("config").empty_values(false)
             .short("c").long("config")
             .help("YAML file containing configuration")
             .default_value(DEFAULT_CONFIG_FILE))
        .get_matches();

    match Boss::new(matches.value_of("config").unwrap()) {
        Ok(config) => run(Arc::new(config)),
        Err(e) => eprintln!("{}", e),
    }
}
