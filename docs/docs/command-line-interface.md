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
  -e, --command <COMMAND>...       Command and args to execute (must be last argument)
  -w, --working-dir <WORKING_DIR>  Start the shell in the specified working directory
      --write-config [<PATH>]      Writes the config to a given path or the default location
      --log-file                   Writes the logs to a file inside the config directory
      --title-placeholder <TITLE>  Start window with specified title
  -h, --help                       Print help
  -V, --version                    Print version
```

The options "-e" and "--command" executes the command and closes the terminal right way after the execution.

```sh
$ rio -e sleep 10
```

You can also `RIO_LOG_LEVEL` environment variable for filter logs on-demand, for example:

```sh
$ RIO_LOG_LEVEL=debug rio -e echo 85
```

## Manual Pages

Rio provides comprehensive manual pages that can be installed on Unix-like systems:

- `man rio` - Main Rio terminal manual page
- `man 5 rio` - Configuration file format documentation
- `man 5 rio-bindings` - Key bindings reference

### Installing Man Pages

The man pages are available in the `extra/man/` directory and require `scdoc` to build:

```sh
# Install scdoc (macOS)
brew install scdoc

# Install scdoc (Ubuntu/Debian)
sudo apt install scdoc

# Build and install man pages
cd extra/man
make
sudo make install
```

After installation, you can access the documentation offline using the `man` command.
