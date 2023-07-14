---
layout: docs
class: docs
title: 'Documentation'
language: 'en'
---

## Command-line interface (CLI)

A command-line interface is a means of interacting with a device or computer program with commands from a user or client, and responses from the device or program, in the form of lines of text. Rio terminal has a command-line interface that you can use for different purposes.

{% highlight bash %}
$ rio --help
Rio terminal app

Usage: rio [OPTIONS]

Options:
  -e, --command <COMMAND>...  Command and args to execute (must be last argument)
  -h, --help                  Print help
  -V, --version               Print version
{% endhighlight %}

The options "-e" and "--command" executes the command and closes the terminal right way after the execution.

{% highlight bash %}
$ rio -e sleep 10
{% endhighlight %}

You can also <span class="keyword">RIO_LOG_LEVEL</span> enviroment variable for filter logs on-demand, for example:

{% highlight bash %}
$ RIO_LOG_LEVEL=debug rio -e echo 85
{% endhighlight %}