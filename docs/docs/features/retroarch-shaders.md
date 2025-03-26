---
title: 'RetroArch shaders'
language: 'en'
---

Rio allow to configure filters based on RetroArch shaders: [github.com/libretro/slang-shaders](https://github.com/libretro/slang-shaders).

```toml
[renderer]
filters = [
  # load builtin filter
  "newpixiecrt",
  
  # or load your own filter
  "/Users/raphael/Downloads/slang-shaders-master/crt/newpixie-crt.slangp"
]
```

![Demo shaders](/assets/features/demo-retroarch-1.png)

![Demo shaders 2](/assets/features/demo-retroarch-2.png)

