---
title: 'Themes'
language: 'en'
---

Rio has a configuration property called "theme". You can set the theme that you want to use and Rio will look in the folder "themes" in the configuration path.

In the example below, we will setup Dracula theme for Rio (https://github.com/dracula/rio-terminal).

After download the `dracula.toml` file and moved it to folder called "themes" inside of the configuration folder, for example in linux `$XDG_CONFIG_HOME/rio/themes/dracula.toml`.

```toml
theme = "dracula"
```

It should look like this:

![Dracula theme example](/assets/posts/0.0.5/dracula-nvim.png)

Another example would be install [Lucario color scheme for Rio terminal](https://github.com/raphamorim/lucario/#rio-terminal). Moving the downloaded file to `$XDG_CONFIG_HOME/rio/themes/lucario.toml`

```toml
theme = "lucario"
```

![Lucario theme example](https://github.com/raphamorim/lucario/raw/main/images/rio.png)

If you are looking for a different theme. You can find more than 250 themes for Rio terminal in this repository: [mbadolato/iTerm2-Color-Schemes/tree/master/rio](https://github.com/mbadolato/iTerm2-Color-Schemes/tree/master/rio).
