---
title: 'VT320 Soft Character (DRCS)'
language: 'en'
---

# VT320 Soft Character (DRCS)

Rio implements VT320 terminal's Dynamically Redefinable Character Set (DRCS) functionality, also known as "soft characters". The implementation allows terminals to receive, store, and display custom character glyphs defined by the application.

## OSC Commands

This implementation supports the following OSC commands:

- **OSC 53** - Define a soft character:

```bash
OSC 53 ; char_code ; width ; height ; base64_data ST
```
  
- **OSC 54** - Select a DRCS character set:

```bash
OSC 54 ; set_id ST
```
  
- **OSC 153** - Reset all soft characters:

```bash
OSC 153 ST
```

## Character Bitmap Format

The DRCS characters are stored as 1-bit-per-pixel bitmaps, packed into bytes. The bitmap data is organized row by row, with each row padded to a byte boundary. The bits are ordered from most significant to least significant within each byte.

For example, an 8x8 character would require 8 bytes of data (one byte per row).

## Resources

- [VT320 Terminal Reference Manual](https://vt100.net/dec/vt320/soft_characters)
- [DRCS Technical Documentation](https://vt100.net/docs/vt320-uu/chapter4.html#S4.10.5)
