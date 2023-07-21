# Copa

Copa is a fork of [Alacritty's VTE](https://github.com/alacritty/vte/) intended to extend [Paul Williams' ANSI parser state
machine] with custom instructions.

The state machine doesn't assign meaning to the parsed data and is
thus not itself sufficient for writing a terminal emulator. Instead, it is
expected that an implementation of the `Perform` trait which does something useful with the parsed data. The `Parser` handles the book keeping, and the `Perform` gets to simply handle actions.

See the [ansicode.txt](resources/ansicode.txt) for more info.

[Paul Williams' ANSI parser state machine]: https://vt100.net/emu/dec_ansi_parser
[docs]: https://docs.rs/crate/vte/
