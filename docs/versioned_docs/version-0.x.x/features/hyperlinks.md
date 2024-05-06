---
title: 'Hyperlinks'
language: 'en'
---

Rio terminal supports opening hyperlinks from the terminal.

## MacOS

To activate hyperlink feature hold `Command` key when hovering a link:

![Demo macos hyperlink](/assets/features/demo-hyperlink-macos.gif)

## Windows / Linux / BSD

To activate hyperlink feature hold `alt` key when hovering a link:

![Demo windows hyperlink](/assets/features/demo-hyperlink-windows.png)

![Demo linux hyperlink](/assets/features/demo-hyperlink-linux.png)

## OSC 8

Rio terminal support OSC 8 for defining hyperlinks.

```bash
OSC 8 ; [params] ; [url] ST
```

The `[params]` consists of zero or more colon-delimited key-value pairs. A key-value pair is formatted as `key=value`. The only currently defined key is id.

If the url is absent then that ends the hyperlink. Typical usage would look like:

```bash
OSC 8 ; ; https://example.com/ ST Link to example website OSC 8 ; ; ST
```

Will work as default rio terminal hyperlinks, by holding command or `alt` key (depending of your platform) and clicking the link.

### OSC 8 Example

```bash
printf '\e]8;;https://raphamorim.io/rio/\e\\This is a link\e]8;;\e\\\n'
```

![Demo hyperlink using OSC 8](/assets/features/demo-hyperlink-osc-8.png)
