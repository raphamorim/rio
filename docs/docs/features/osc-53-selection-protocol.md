---
title: 'OSC 53 - Terminal Text Selection Protocol'
language: 'en'
---

# OSC 53 - Terminal Text Selection Protocol

## Overview

OSC 53 is a terminal control sequence protocol originally created by Rio Terminal to provide a standardized mechanism for applications to programmatically control text selection within the terminal viewport. This protocol enables precise selection of text regions using row and column coordinates, facilitating advanced terminal applications and automation workflows.

## Protocol Origin

This protocol was designed and first implemented by Rio Terminal to address the lack of a standard method for programmatic text selection in terminal emulators. While terminals have long supported mouse-based selection and clipboard operations via OSC 52, there was no standard way for applications to define selections programmatically.

## Syntax

```
OSC 53 ; <operation> ; <parameters> ST
```

Where:
- `OSC` = `ESC ]` (0x1B 0x5D)
- `ST` = String Terminator (`ESC \` or `BEL`)
- `<operation>` = Operation code (see Operations section)
- `<parameters>` = Operation-specific parameters

## Operations

### Set Selection (operation: `s`)

Defines a text selection region using start and end coordinates.

```
OSC 53 ; s ; <start_row>,<start_col>;<end_row>,<end_col> ST
```

Parameters:
- `start_row`: Starting row (0-based, relative to viewport)
- `start_col`: Starting column (0-based)
- `end_row`: Ending row (0-based, relative to viewport)
- `end_col`: Ending column (0-based)

Example:
```
OSC 53 ; s ; 5,10;8,45 ST
```
Selects text from row 5, column 10 to row 8, column 45.

### Clear Selection (operation: `c`)

Clears the current selection.

```
OSC 53 ; c ST
```

### Query Selection (operation: `q`)

Requests the current selection coordinates. The terminal responds with:

```
OSC 53 ; r ; <start_row>,<start_col>;<end_row>,<end_col> ST
```

If no selection exists, the terminal responds with:
```
OSC 53 ; r ; none ST
```

### Copy Selection (operation: `y`)

Copies the current selection to the system clipboard.

```
OSC 53 ; y ST
```

## Coordinate System

- **Origin**: Top-left corner of the viewport is (0,0)
- **Row Range**: 0 to (visible_rows - 1)
- **Column Range**: 0 to (terminal_width - 1)
- **Direction**: Selection can be forward (start < end) or backward (start > end)
- **Scrollback**: Negative row values may be used to select into scrollback buffer (implementation-specific)

## Selection Behavior

1. **Boundary Handling**: Coordinates exceeding viewport boundaries are clamped to valid ranges
2. **Line Wrapping**: Selection respects line wrapping; wrapped lines are treated as continuous
3. **Double-Width Characters**: Column coordinates account for character cell width
4. **Empty Selection**: When start equals end, selection is cleared
5. **Visual Feedback**: Terminal should provide visual indication of selected text

## Error Handling

Invalid operations or malformed parameters should be silently ignored. No error response is generated to maintain backward compatibility with terminals that do not support this protocol.

## Security Considerations

1. **Clipboard Access**: Copy operations should respect system security policies
2. **Selection Limits**: Implementations may impose reasonable limits on selection size
3. **User Override**: Users should be able to disable programmatic selection via terminal preferences

## Examples

```bash
# Select entire first line
printf '\033]53;s;0,0;0,79\033\\'

# Select rectangular region
printf '\033]53;s;10,20;15,40\033\\'

# Clear selection
printf '\033]53;c\033\\'

# Query current selection
printf '\033]53;q\033\\'

# Copy selection to clipboard
printf '\033]53;y\033\\'
```

## Implementation Notes

- Terminals supporting this protocol should advertise capability via terminfo/termcap
- Selection operations should integrate with native terminal selection mechanisms
- Mouse selection should update the programmatic selection state accordingly
- The protocol is designed to be extensible for future operations

## Adoption

As the originator of this protocol, Rio Terminal provides the reference implementation. Other terminal emulators are encouraged to adopt this specification to provide consistent programmatic selection capabilities across different terminal environments.