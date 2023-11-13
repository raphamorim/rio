---
title: 'Command-line interface'
language: 'en'
---

A command-line interface (CLI) is a means of interacting with a device or computer program with commands from a user or client, and responses from the device or program, in the form of lines of text. Rio terminal has a command-line interface that you can use for different purposes.

```sh
$ rio --help
A hardware-accelerated GPU terminal emulator powered by WebGPU, focusing to run in desktops and browsers

Usage: rio [OPTIONS]

Options:
  -e, --command <COMMAND>...  Command and args to execute (must be last argument)
  --working-dir <WORKING_DIR>  Start the shell in the specified working directory
  -h, --help                  Print help
  -V, --version               Print version
```

The options "-e" and "--command" executes the command and closes the terminal right way after the execution.

```sh
$ rio -e sleep 10
```

You can also `RIO_LOG_LEVEL` environment variable for filter logs on-demand, for example:

```sh
$ RIO_LOG_LEVEL=debug rio -e echo 85
```
