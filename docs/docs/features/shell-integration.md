---
title: 'Shell integration'
language: 'en'
---

Rio supports integrating with the shell through the following means:

- OSC 7 Escape sequences to advise the terminal of the working directory.
<!-- - OSC 133 Escape sequence to define Input, Output and Prompt zones. -->
<!-- - OSC 1337 Escape sequences to set user vars for tracking additional shell state. -->

OSC is escape sequence jargon for Operating System Command.

## Title integration

Programs notify Rio of the current working directory and document by sending it commands. You may need to configure your shell or other programs to enable this behavior.

The working directory and location of the current document may be set using the Operating System Command (OSC) escape sequence:

```
ESC ] Ps ; Pt BEL
```

The parameter Ps is either 6 (document) or 7 (working directory) and Pt is a “file:” URL. The URL should include a hostname to disambiguate local and remote paths, and characters must be percent-encoded as appropriate.

When both the working directory and document are set only the document is displayed.
