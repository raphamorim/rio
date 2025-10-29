---
title: 'Vi mode'
language: 'en'
---

Rio implements the Vi mode feature originally from [Alacritty](https://github.com/alacritty/alacritty/blob/master/docs/features.md#vi-mode).

By default you can launch Vi mode by using `alt` + `shift` + `space`.

![Demo Vi mode](/assets/features/demo-vi-mode.gif)

Below you can see the list of all default key bindings related to Vi mode. If you don't like of any specified key binding you can always turn off or modify (check [key bindings documentation section](/docs/key-bindings) for more information).

| Trigger                   | Action                     | Condition            |
| :------------------------ | :------------------------- | :------------------- |
| `alt` + `shift` + `space` | Toggle Vi Mode             | No restriction       |
| `i`                       | Toggle Vi Mode             | Vi mode is activated |
| `control` + `c`           | Toggle Vi Mode             | Vi mode is activated |
| `y`                       | `Copy` or `ClearSelection` | Vi mode is activated |
| `v`                       | Start normal selection     | Vi mode is activated |
| `v` + `shift`             | Start line selection       | Vi mode is activated |
| `v` + `control`           | Start block selection      | Vi mode is activated |
| `v` + `alt`               | Start semantic selection   | Vi mode is activated |
| `z`                       | Center around Vi cursor    | Vi mode is activated |
| `y` + `control`           | Scroll up 1 line           | Vi mode is activated |
| `e` + `control`           | Scroll down 1 line         | Vi mode is activated |
| `b` + `control`           | Scroll page up             | Vi mode is activated |
| `u` + `control`           | Scroll half page up        | Vi mode is activated |
| `d` + `control`           | Scroll half page down      | Vi mode is activated |
| `e` + `control`           | Scroll down 1 line         | Vi mode is activated |
| `k`                       | Move cursor up             | Vi mode is activated |
| `j`                       | Move cursor down           | Vi mode is activated |
| `h`                       | Move cursor left           | Vi mode is activated |
| `l`                       | Move cursor right          | Vi mode is activated |
| Arrow up                  | Move cursor up             | Vi mode is activated |
| Arrow down                | Move cursor down           | Vi mode is activated |
| Arrow left                | Move cursor left           | Vi mode is activated |
| Arrow right               | Move cursor right          | Vi mode is activated |
| `0`                       | Move first                 | Vi mode is activated |
| `4` + `shift`             | Move last                  | Vi mode is activated |
| `6` + `shift`             | Move first occupied        | Vi mode is activated |
| `h` + `shift`             | Move high                  | Vi mode is activated |
| `m` + `shift`             | Move middle                | Vi mode is activated |
| `l` + `shift`             | Move low                   | Vi mode is activated |
| `b`                       | Move semantic left         | Vi mode is activated |
| `w`                       | Move semantic right        | Vi mode is activated |
| `e`                       | Move semantic right end    | Vi mode is activated |
| `b` + `shift`             | Move word left             | Vi mode is activated |
| `w` + `shift`             | Move word right            | Vi mode is activated |
| `e` + `shift`             | Move word right end        | Vi mode is activated |
| `5`                       | Move by bracket rule       | Vi mode is activated |
