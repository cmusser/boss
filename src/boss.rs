extern crate boss;
extern crate clap;
extern crate futures;

use boss::data::Boss;
use clap::{App, Arg};

const VERSION: &'static str = "0.1.0";
const DEFAULT_CONFIG_FILE: &'static str = "/etc/boss.yaml";


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
        Ok(boss) => Boss::run(boss),
        Err(e) => eprintln!("{}", e),
    }
}
