use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::prelude::*;
use std::net::SocketAddr;
use std::process::Command;
use std::sync::{Arc, RwLock};

use hyper::{Body, Response, Server};
use hyper::http::StatusCode;
use hyper::service::service_fn_ok;
use hyper::rt::{self, Future};
use tokio_process::CommandExt;

macro_rules! non_ok_response {
    ($status:expr, $msg:expr) => {
    Response::builder().status($status).body(Body::from($msg)).unwrap()
    }
}

#[derive(Debug, Deserialize)]
pub struct Boss {
    pub listen_addr: String,
    pub clients: Arc<RwLock<HashMap<String,ClientProcess>>>
}

impl Boss {

    pub fn new(config_path: &str) -> Result<Self,String> {
        match File::open(config_path) {
            Err(err) => Err(format!("couldn't open {} ({})", config_path, err.description())),
            Ok(mut file) => {
                let mut yaml = String::new();
                match file.read_to_string(&mut yaml) {
                    Err(err) => Err(format!("couldn't read {}: {}", config_path, err.description())),
                    Ok(_) => {
                        match ::serde_yaml::from_str::<Boss>(&yaml) {
                            Ok(boss) => {
                                println!("using config in '{}'", config_path);
                                Ok(boss)
                            },
                            Err(e) => Err(format!("{}", e.description()))
                        }
                    },
                }
            },
        }
    }

    pub fn run(boss: Boss) {
        let addr: SocketAddr = boss.listen_addr.parse().unwrap();
        let boss_service = move || {
            let clients_for_start = boss.clients.clone();
            service_fn_ok(move |req| {
                let client = String::from(req.uri().path());
                
                match clients_for_start.write().unwrap().get_mut(&client) {
                    Some(client_data) => {
                        match client_data.pid {
                            Some(pid) => {
                                println!("already running with pid {}", pid);
                                Response::new(Body::from("available\n"))
                            },
                            None => {
                                let cmd_array = client_data.launch_cmd.split_whitespace().collect::<Vec<&str>>();
                                match Command::new(&cmd_array[0]).args(cmd_array[1..].into_iter()).spawn_async() {
                                    Ok(command) => {
                                        let pid = command.id();
                                        println!("launching {} with PID {}", client_data.launch_cmd, pid);
                                        client_data.pid = Some(pid);
                                        let clients_for_cleanup = clients_for_start.clone();
                                        let command_complete = command
                                            .map(|status| { (status, clients_for_cleanup) })
                                            .then(move|args| {
                                                let (status, clients_for_cleanup) = args.unwrap();
                                                println!("command for \"{}\" has terminated with status {}", client, status);
                                                match clients_for_cleanup.write().unwrap().get_mut(&client) {
                                                    Some(client_data) => client_data.pid = None,
                                                    None => println!("client \"{}\" not found", client)
                                                }
                                                futures::future::ok(())
                                            });
                                        rt::spawn(command_complete);
                                        Response::new(Body::from("available\n"))
                                    },
                                    Err(e) =>  {
                                        println!("couldn't start \"{}\": {}", client_data.launch_cmd, e);
                                        non_ok_response!(StatusCode::INTERNAL_SERVER_ERROR, "couldn't start")
                                    }
                                }
                            }
                        }
                    },
                    None => {
                        println!("client \"{}\" not found", client);
                        non_ok_response!(StatusCode::NOT_FOUND,
                                         format!("no client \"{}\"", client))
                    }
                }
            })
        };
        
        let server = Server::bind(&addr).serve(boss_service)
            .map_err(|e| eprintln!("server error: {}", e));
        println!("Listening on http://{}", addr);
        rt::run(server);
    }

}

#[derive(Debug, Deserialize)]
pub struct ClientProcess {
    pub launch_cmd: String,
    #[serde(skip)]
    pub pid: Option<u32>,
}
