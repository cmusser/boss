# BOSS - the Barely Omnicient System Starter

## Overview
BOSS is a webserver that responds to requests by invoking a process if
the process is not known to be running already. Processes to run are
arranged in a two level hierarchy: one or more "applications" at the
top level and under that, one or more "clients".

## Motivation
`boss` was designed to deploy vpnd--a VPN system where the server side
is designed to interact with a single peer--in a way that serves
multiple clients. One way to solving this particular problem with
`vpnd` would be modifying `vpnd` itself to support multiple clients.
But this adds complexity, and with it, the possibility of bugs and
performance degradation. `boss` allows a fairly elegant multi-user
capability without requiring that `vpnd` be modified.

## Usage

### Configuration

The configuration file is in YAML format and describes the listen
address, the list of applications and for each app, the list of
clients. Each client specifies a command to invoke. The configuration
looks like this:

    listen_addr: 0.0.0.0:8080
    apps:
      some-app:
        fred:
          launch_cmd: /usr/bin/some-app -c /etc/some-app/fred.conf
        martha:
          launch_cmd: /usr/bin/some-app -c /etc/vpnd/some-app/martha.conf
      some-other-app:
        johnny:
          launch_cmd: /usr/bin/some-other-app -c /etc/some-other-app/johnny.conf
        bocephus:
          launch_cmd: /usr/bin/some-other-app -c /etc/some-other-app/bocephus.conf

### Client access

To request an application for a client, use a web client, such as curl:

	 ~ >> curl localhost:8080/some-app/fred
    available

Note that a response from `boss` only indicates whether the requested
service is available. It does not return any information about
it--the assumption is that clients have all the information needed to
connect to said service. Adding this kind of information might be a
future enhancement.

## Security
One could reasonably ask: "is invoking processes in response to web
requests __really a good idea__?" At the very least, you should consider
this to be a capability that should be used sparingly. A number of
things could make it less iffy:

- Running the service over HTTPS, which could be accomplished in the
  short term by putting `boss` behind a reverse proxy that does
  the TLS termination. The `listen_addr` configuration directive
  is of use here. Small setups could just listen on `localhost`.
  
- Having some kind of authentication method, at the very least an API
  key or request token encrypted with a shared secret.
  
- Very careful consideration of the commands you put in the
  configuration file.
  
- The possibility that this should be a library crate that is used by
  programs that contain a hardwired set of very specific commands to
  execute.  This would encourage programs explicitly designed
  to have limited scope. The responsibility for thinking this through
  would be with the application developer.

All these things considered, `boss` is a good way of providing a
flexible command starter for programs that are designed to run on
behalf of (and at the request of) a single external client.
