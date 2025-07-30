---
title: 'Hints'
language: 'en'
---

# Hints

Rio's hints system allows you to quickly interact with text patterns in your terminal by displaying keyboard shortcuts over matching content. This feature allows for quick interaction with terminal content through keyboard shortcuts.

## How it works

When you activate hint mode, Rio scans the visible terminal content for patterns you've configured (like URLs, file paths, or email addresses) and displays keyboard shortcuts over each match. You can then press the corresponding keys to perform actions on the matched text.

## Basic Usage

1. **Activate hint mode**: Press the configured key binding (default varies by hint type)
2. **Navigate**: Type the letters shown over the hint you want to select
3. **Execute**: The configured action (copy, open, paste, etc.) will be performed

## Configuration

Hints are configured in your `rio.toml` file under the `[hints]` section:

```toml
[hints]
# Characters used for hint labels (should be easy to type)
alphabet = "jfkdls;ahgurieowpq"

# URL hint example
[[hints.rules]]
regex = "(https://|http://|ftp://)[^\u{0000}-\u{001F}\u{007F}-\u{009F}<>\"\\s{-}\\^⟨⟩`\\\\]+"
hyperlinks = true
post-processing = true
persist = false

[hints.rules.action]
command = "xdg-open"  # Linux/BSD
# command = "open"    # macOS  
# command = { program = "cmd", args = ["/c", "start", ""] }  # Windows

[hints.rules.mouse]
enabled = true
mods = []

[hints.rules.binding]
key = "O"
mods = ["Control", "Shift"]
```

### Customizing Hint Colors

Hint label colors can be customized in the `[colors]` section of your configuration:

```toml
[colors]
hint-foreground = '#181818'  # Text color for hint labels
hint-background = '#f4bf75'  # Background color for hint labels
```

## Configuration Options

### Global Settings

- **`alphabet`**: String of characters used for hint labels. Should contain easily accessible keys.

### Per-Hint Settings

- **`regex`**: Regular expression pattern to match
- **`hyperlinks`**: Whether to treat matches as hyperlinks (enables special handling)
- **`post-processing`**: Apply post-processing to clean up matched text
- **`persist`**: Keep hint mode active after selection (useful for multiple selections)

### Actions

You can configure different types of actions:

#### Built-in Actions
```toml
[hints.rules.action]
action = "Copy"     # Copy to clipboard
# action = "Paste"  # Paste the matched text
# action = "Select" # Select the matched text
```

#### External Commands
```toml
[hints.rules.action]
command = "xdg-open"  # Simple command

# Or with arguments
command = { program = "code", args = ["--goto"] }
```

### Key Bindings

```toml
[hints.rules.binding]
key = "O"
mods = ["Control", "Shift"]
```

### Mouse Support

```toml
[hints.rules.mouse]
enabled = true
mods = ["Control"]  # Modifier keys required for mouse activation
```

## Example Configurations

### URL Opener
```toml
[[hints.rules]]
regex = "(https://|http://)[^\u{0000}-\u{001F}\u{007F}-\u{009F}<>\"\\s{-}\\^⟨⟩`\\\\]+"
hyperlinks = true
post-processing = true

[hints.rules.action]
command = "xdg-open"

[hints.rules.binding]
key = "O"
mods = ["Control", "Shift"]
```

### File Path Copier
```toml
[[hints.rules]]
regex = "/?(?:[\\w.-]+/)*[\\w.-]+"
post-processing = true

[hints.rules.action]
action = "Copy"

[hints.rules.binding]
key = "F"
mods = ["Control", "Shift"]
```

### Email Composer
```toml
[[hints.rules]]
regex = "[\\w.-]+@[\\w.-]+\\.[a-zA-Z]{2,}"

[hints.rules.action]
command = { program = "thunderbird", args = ["-compose", "to="] }

[hints.rules.binding]
key = "E"
mods = ["Control", "Shift"]
```

## Performance

Rio's hints system includes optimized rendering with damage tracking to ensure smooth performance:

- **Damage Tracking**: Only re-renders areas where hint labels have changed.
- **Efficient Cleanup**: Properly marks hint label areas for re-rendering when cleared.
- **Minimal Overhead**: Hints are only processed when activated.
