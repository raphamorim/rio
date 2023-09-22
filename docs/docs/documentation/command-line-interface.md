---
layout: docs
class: docs
title: 'Command-line interface'
language: 'en'
---

## Command-line interface

A command-line interface (CLI) is a means of interacting with a device or computer program with commands from a user or client, and responses from the device or program, in the form of lines of text. Rio terminal has a command-line interface that you can use for different purposes.

```
$ rio --help
Rio terminal app

Usage: rio [OPTIONS]

Options:
<p>  -e, --command <COMMAND>...  Command and args to execute (must be last argument)
<p>  -h, --help                  Print help
<p>  -V, --version               Print version

The options "-e" and "--command" executes the command and closes the terminal right way after the execution.
```
```
$ rio -e sleep 10
```
You can also <span class="keyword">RIO_LOG_LEVEL</span> environment variable for filter logs on-demand, for example:

$ RIO_LOG_LEVEL=debug rio -e echo 85
