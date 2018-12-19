use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::prelude::*;
use std::process::Command;
use std::sync::RwLock;

use tokio_process::{Child, CommandExt};

pub enum LaunchResult {
    Launched(Child),
    AlreadyRunning,
    AppNotFound,
    ClientNotFound,
    Err,
}

#[derive(Debug, Deserialize)]
pub struct Boss {
    pub listen_addr: String,
    pub apps: RwLock<HashMap<String,HashMap<String,ClientProcess>>>
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
                        match serde_yaml::from_str::<Boss>(&yaml) {
                            Ok(config) => {
                                println!("using config in '{}'", config_path);
                                Ok(config)
                            },
                            Err(e) => Err(format!("{}", e.description()))
                        }
                    },
                }
            },
        }
    }

    pub fn start(&self, app: &str, client: &str) -> LaunchResult {
        match self.apps.write().unwrap().get_mut(app) {
            Some(app_users) => {
                match app_users.get_mut(client) {
                    Some(client_data) => {
                        match client_data.pid {
                            Some(pid) => {
                                println!("already running with pid {}", pid);
                                LaunchResult::AlreadyRunning
                            },
                            None => {
                                let cmd_array = client_data.launch_cmd.split_whitespace().collect::<Vec<&str>>();
                                match Command::new(&cmd_array[0]).args(cmd_array[1..].into_iter()).spawn_async() {
                                    Ok(command) => {
                                        let pid = command.id();
                                        println!("launching {} with PID {}", client_data.launch_cmd, pid);
                                        client_data.pid = Some(pid);
                                        LaunchResult::Launched(command)
                                    },
                                    Err(e) =>  {
                                        println!("couldn't start \"{}\": {}", client_data.launch_cmd, e);
                                        LaunchResult::Err
                                    }
                                }
                            }
                        }
                    },
                    None => {
                        println!("no user {} for {}",client, app);
                        LaunchResult::ClientNotFound
                    }
                }
            },
            None => {
                println!("application {} not found", app);
                LaunchResult::AppNotFound
            }
        }
    }

    pub fn cleanup(&self, app: &str, client: &str) {
        match self.apps.write().unwrap().get_mut(app) {
            Some(app_users) => {
                match app_users.get_mut(client) {
                    Some(client_data) => client_data.pid = None,
                    None => println!("no user {} for {}",client, app)
                }
            },
            None => println!("application {} not found", app),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ClientProcess {
    pub launch_cmd: String,
    #[serde(skip)]
    pub pid: Option<u32>,
}
