---
layout: post
title: "What's coming next?"
date: 2026-03-11 12:00
description: "A look at what's coming in the next major version of Rio terminal."
categories: macos linux windows
authors: raphamorim
---

Hey folks!

It's been a while since the last proper update on where Rio is headed. I've been on parental leave, and as some of you may know, I've also been working on a programming language called Jam. Between those two things, Rio updates have been quieter than usual, but development never stopped. So let's talk about what's been cooking.

The next version of Rio is a significant step forward. A lot of the work has gone into the rendering pipeline, the UI layer, and laying the groundwork for something bigger. Let me walk you through it.

The renderer has been substantially reworked. Here's a rundown:

- **Metal support**: Rio now runs natively on Metal, Apple's GPU API. This means lower overhead and better integration on macOS.
- **Rect system rewrite**:The rect pipeline has been fixed and images have been merged into the same rect system, removing a whole class of rendering inconsistencies.
- **Shape-level deduplication**:Previously, only rich text elements had IDs and could skip recomputation. Now any type of shape can do the same, which cuts down redundant GPU work significantly.
- **Z-index fixes**:Layering issues that caused elements to render in the wrong order have been resolved.
- **Strikethrough fix**:Strikethrough rendering was broken in certain scenarios and is now working correctly.
- **Column/row dimension fixes**:Terminal dimensions are now computed correctly, fixing a category of layout bugs.
- **Consistent 60+ FPS**:Even when hammering the terminal with empty line breaks, Rio now maintains at least a consistent 60fps. Performance was a priority throughout.

New UI features also:

- **Command palette**:Rio now ships with a built-in command palette. It's the foundation for a more discoverable, keyboard-driven interface.
- **New navigation system**:Tab navigation has been redesigned from scratch with a new island-style tab bar.
- **Title bar progress bar**:Long-running operations can now show progress directly in the title bar.
- **Tab management improvements**:Creating or switching tabs now properly manages visible rich text, fixing visual glitches during transitions.
- **New welcome screen**:First-launch experience has been refreshed.
- **Editable tab titles and colors**:You can now rename tabs and change their colors directly through the UI.

And more stuff to announce like Quake mode, Kitty image protocol, [BiDi (bidirectional text) support](https://terminal-wg.pages.freedesktop.org/bidi/) and etcetera.

## Introducing librio

This is one I'm particularly excited about.

**librio** is Rio's terminal engine extracted as a standalone library, similar in spirit to [libghostty](https://ghostty.org). The idea is simple: other applications should be able to embed a high-quality terminal without reinventing the wheel.

librio handles the input processing, the grid, and all the terminal emulation logic, packaged as a library that can be consumed by native applications.

## Super Rio

To prove out librio, I've been working on a project called **Super Rio**, a terminal application built fully native in Swift, powered entirely by librio under the hood. It's a native macOS app that gets all of Rio's terminal capabilities without reimplementing them.

Super Rio is both a showcase for librio and an exploration of what a truly native terminal experience looks like when the heavy lifting is handled by a proven engine. It's also where I plan to explore AI features, things like inline suggestions, natural language commands, and intelligent context awareness.

Here's a quick look at Super Rio in action:

<iframe width="560" height="315" src="https://www.youtube.com/embed/c8mcMNn0f5s" title="Super Rio" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture" allowfullscreen></iframe>

More details on Super Rio will come as it matures.

## Help keep Rio alive on Linux and Windows

Rio is a passion project. Sponsorship currently covers the Apple Developer license, stickers, and domain costs, the basics to keep the project running and distributed.

One area where I could really use help is **Linux and Windows support**: My MacBook is the machine I use to develop and debug Rio on Linux and Windows (through VMs/remote), but it's also my music production machine, loaded with audio software and configs. I've been making it work, but it hasn't been the best experience.

I'd love to get a dedicated Linux machine solely for Rio development, something that lets me properly test, debug, and iterate without workarounds. If you find Rio useful and want to help, consider [sponsoring the project](https://github.com/sponsors/raphamorim). Every bit helps keep the project moving forward across all platforms.

---

That's it for now. There's a lot more in the pipeline, but these are the highlights. Thanks for sticking around, and as always, happy hacking!
