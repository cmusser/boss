# BOSS - the Barely Omnicient System Starter

## Overview
BOSS manages a set of commmands described in a YAML file, spawning
a process for each in the foreground and waiting for them to finish.
On completion, the processes are restarted. You can add, remove
or update the commands in the YAML file and inform BOSS via a 
signal. In response, it will update the set of running commands, spawning
and terminating processes as needed.

## Motivation
`boss` is useful for managing sets of closely coordinated processes,
A specific application is deploying [vpnd](https://github.com/cmusser/vpnd),
a VPN system where each process interacts with a single client. To serve
multiple clients, multiple server processes are needed, which gets unwieldy
if managed by hand. One way to solve this problem would be to modify
`vpnd` itself to support multiple clients. But this adds complexity, and
inevitably, bugs and performance degradation. `boss` allows easy management
for cases similar to this.

## Usage

### Configuration

The configuration file is in YAML format and describes the list of commands.
Each command has a name, and the actual command to execute: The configuration
looks like this:

    First Task:
      argv: /usr/bin/cmd first 4
    Second Task:
      argv: /usr/bin/cmd second 10
    Second And A Half Task:
      argv: /usr/bin/cmd second-and-a-half 5
    Third Task:
      argv: /usr/bin/cmd third 6

### Invocation

Launch `boss` and it will spawn all tasks in its configuration. The
stdin/stdout/stderr for the spawned processses are inherited from boss itself,
so redirect the output of boss in a way suitable for the commands. To change
the set of commands, or the actual commands invoked, edit the configuration
file and send `boss` the `SIGHUP` signal.

## History

The previous version of this program took a substantially different form, both
in design and implementation. It used to have a webserver frontend, the idea
being that one started processes individually via REST requests. Aside from
the potential for security problems (like, don't just run this bound to an
Internet-exposed interface), individual launching didn't actually add much value.
If you want to start processes in an ad hoc manner, just start them by hand.

As for the implementation, it was pre-`async/.await`, so the the asynchronous
aspects of the program were harder to read.

## Future
This could evolve into an something that could be used instead of `systemd`
or Docker Compose, possibly for use on non-Linux systems or embedded machines
where the full Linux wasn't present. It needs the ability to specify whether
processes should be restarted on termination and probably needs more logic
for signal handling in general, like responding differently depending on the
signal received by a process. Another enhancement might be reloading by
simply editing the configuration file, which `boss` could watch via `inotify`
or a similar mechanism. No need to send a signal to re-read the config.
