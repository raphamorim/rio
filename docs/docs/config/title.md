---
title: 'title'
language: 'en'
---

Rio allows you to configure window and tabs title through configuration via template.

- `content` - Configure window title using template

  - Default: `{{ title || program }}`

- `placeholder` - Configure initial title
  
  - Default: `▲`

## content

Note: **Variables are not case sensitive.**

Possible options:

- `TITLE`: terminal title via OSC sequences for setting terminal title
- `PROGRAM`: (e.g `fish`, `zsh`, `bash`, `vim`, etc...)
- `ABSOLUTE_PATH`: (e.g `/Users/rapha/Documents/a/rio`)
<!-- - `CANONICAL_PATH`: (e.g `.../Documents/a/rio`, `~/Documents/a`) -->
- `COLUMNS`: current columns
- `LINES`: current lines

### Example 1:

```toml
[title]
content = "{{ PROGRAM }} - {{ ABSOLUTE_PATH }}"
```

Result: `fish - .../Documents/a/rio`.

### Example 2:

```toml
[title]
content = "{{ program }} ({{columns}}x{{lines}})"
```

Result: `fish (85x23)`.

### Example 3:

You can use `||` operator, in case the value is empty or non-existent it will use the following:

```toml
[title]
content = "{{ TITLE | PROGRAM }}"
```

In this case, `TITLE` is non-existent so will use `PROGRAM`.

Result: `zsh`

## placeholder

Configure initial title.
  
```toml
[title]
placeholder = "▲"
```