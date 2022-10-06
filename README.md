# Rio: ⚡ terminal app 

1. Rio is licensed under MIT license
2. Runs on WPGU ([what's WPGU?](https://dmnsgn.me/blog/from-glsl-to-wgsl-the-future-of-shaders-on-the-web/))
3. This project depends of donations as well, so if you are using please consider to donate via [Github Sponsors](https://github.com/sponsors/raphamorim) or [ko-fi]().

## Features

- [x] WGPU rendering
- [ ] Keyboard input
- [ ] Screen resizing
- [ ] Style rendering (italic, bold, underline)
- [ ] Character set

## Configuration

The configuration should be the following paths otherwise Rio will use the default configuration ([which you can see here]())

- macOs path: `~/.rio/config.toml`

#### config.toml

```toml
# Define Rio properties as you want

# default width and height
default_size = [300, 300]

# options: high, average, low
perfomance = "high"
```

## References

- https://chromestatus.com/feature/6213121689518080
- https://dmnsgn.me/blog/from-glsl-to-wgsl-the-future-of-shaders-on-the-web/
- http://www.linusakesson.net/programming/tty/index.php
- https://www.uninformativ.de/blog/postings/2018-02-24/0/POSTING-en.html
- https://github.com/bisqwit/that_terminal
- https://en.wikipedia.org/wiki/Fira_(typeface)#Fira_Mono
