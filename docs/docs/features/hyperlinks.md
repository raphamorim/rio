---
title: 'Hyperlinks'
language: 'en'
---

Rio terminal supports opening hyperlinks from the terminal.

## Configuration

As of Rio 0.2.20, hyperlink hover keys are configurable through the hints system. You can customize which modifier keys trigger hyperlink highlighting and activation.

### Default Behavior

By default, Rio uses:
- **macOS**: `Command` key
- **Windows/Linux/BSD**: `Alt` key

### Custom Configuration

You can configure custom modifier keys in your `config.toml`:

```toml
[hints]
rules = [
    {
        regex = "(ipfs:|ipns:|magnet:|mailto:|gemini:|gopher:|https:|http:|news:|file:|git:|ssh:|ftp:)[^\u0000-\u001F\u007F-\u009F<>\"\\s{-}\\^⟨⟩`]+",
        hyperlinks = true,
        post-processing = true,
        persist = false,
        action = { command = "open" },
        mouse = { enabled = true, mods = ["Shift"] },  # Use Shift key instead
        binding = { key = "O", mods = ["Control", "Shift"] }
    }
]
```

Available modifier keys:
- `"Shift"` - Shift key
- `"Control"` or `"Ctrl"` - Control key  
- `"Alt"` - Alt key
- `"Super"`, `"Cmd"`, or `"Command"` - Super/Command key

You can combine multiple modifiers: `mods = ["Control", "Shift"]`

## MacOS

To activate hyperlink feature hold `Command` key when hovering a link (or your configured modifier):

![Demo macos hyperlink](/assets/features/demo-hyperlink-macos.gif)

## Windows / Linux / BSD

To activate hyperlink feature hold `Alt` key when hovering a link (or your configured modifier):

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

Will work as default rio terminal hyperlinks, by holding command for MacOS and `shift` key for all the other platforms and clicking the link.

### OSC 8 Example

```bash
printf '\e]8;;https://raphamorim.io/rio/\e\\This is a link\e]8;;\e\\\n'
```

![Demo hyperlink using OSC 8](/assets/features/demo-hyperlink-osc-8.png)
