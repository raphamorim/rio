# WA

Cross-platform window assistant made primarily for Rio terminal.

WA was built for windowing for Games and Desktop applications.

```rust
let app = App::new(
	wa::Target::Application,
	Box::new(EventHandlerInstance::new(config))
);

// Available only on Target::Application
menu::create_menu();

app.run();
```

- On MacOS applications uses `NSView` and games uses [`MTKView`](https://developer.apple.com/documentation/metalkit/mtkview).

## Support

| Functionality | MacOS  | Windows  | Linux Wayland | Linux x11 |
| :-- | :--  | :--  | :-- | :-- |
| Multi window | YES (application only)  | NO | NO | NO |
| Tabs | YES (application only) | NO | NO | NO |
| Set background color | YES | NO | NO | NO |
| Set transparency | YES | NO | NO | NO |
| Open Url | YES (application only)  | NO | NO | NO |
| Theming | YES  | NO | NO | NO |

## Acknowledgments

- WA was built originally from a fork from [Macroquad](https://github.com/not-fl3/macroquad) which is licensed under MIT license.

## Reference

- https://developer.apple.com/documentation/metalkit/mtkview
- https://docs.rs/core-foundation/0.9.4/src/core_foundation/runloop.rs.html
- https://suelan.github.io/2021/02/13/20210213-dive-into-runloop-ios/