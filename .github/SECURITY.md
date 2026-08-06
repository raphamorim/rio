# Security Policy

## Reporting a vulnerability

Please report security issues privately, through
[**Report a vulnerability**](https://github.com/raphamorim/rio/security/advisories/new)
on the repository's Security tab. It opens a draft advisory that only you and
the maintainers can read.

Please don't open a public issue for a vulnerability. A terminal runs whatever
its user runs, so a working exploit posted publicly is usable against people
who have not had a chance to update yet.

If GitHub is not an option, email rapha850@gmail.com instead.

## What to include

A report is easiest to act on when it has:

- the version, platform, and which application or crate is affected
- what an attacker controls, and what they get out of it
- steps to reproduce, ideally the shortest input that shows the problem
- anything you already know about the fix

Reports that need a shell, a terminal, or a specific configuration are welcome
even when the reproduction is fiddly. Say what you tried and what you are
unsure about; a partial report is worth more than a silent one.

## Scope

The applications: **Rio** (Linux, macOS, Windows, BSD) and **Canario** (macOS).

The published crates, whose embedders inherit anything wrong in them:
`rio-vt`, `librio`, `sugarloaf`, `rio-backend`, `rio-window`, `rio-graphics`,
`rio-fonts`, `rio-grapheme-width`, `rio-notifier`, `teletypewriter`,
`corcovado`.

A terminal's central risk is that the bytes it renders are not trusted: they
arrive from remote hosts, from programs, from files someone else wrote. So the
interesting reports are the ones where output, or something that carries it,
crosses into action on the host machine. For example:

- escape sequences that reach beyond the grid: writing files, running commands,
  reading back data the program should not have
- a confirmation, prompt, or preview that can be made to read differently than
  what it does
- text placed on the clipboard, or replayed into a shell, that executes without
  the user meaning it to
- hints, links, image protocols, and deep links, all of which turn untrusted
  content into something clickable
- memory-safety failures in the parser, the grid, or the renderers, reachable
  from terminal output

Out of scope: anything requiring an attacker who can already run code as the
user, and configuration files the user wrote themselves.

## Supported versions

Rio is pre-1.0 and moves quickly. Fixes land in the next release, on the
latest version only; there are no backports to earlier ones. Please check
against the latest release before reporting.

## Disclosure

We will work with you on a fix in the draft advisory, and publish it once a
release carrying the fix is out. Advisories credit the reporter by default, and
we request a CVE unless you would rather we did not. If you plan to write or
speak about the issue, tell us your timeline and we will work to it.

Thank you for reporting responsibly.
