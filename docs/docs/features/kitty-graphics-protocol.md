---
title: 'Kitty graphics protocol'
language: 'en'
---

Rio supports the [Kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/), allowing applications to display images directly in the terminal.

## Supported features

- **Image formats**: PNG, RGB (24-bit), RGBA (32-bit), Grayscale, Grayscale+Alpha
- **Transmission**: Direct (inline base64), file, temporary file, shared memory
- **Compression**: Zlib
- **Chunked transfers**: Multi-part image transmission
- **Placements**: Display images at cursor or specific positions with z-index layering
- **Virtual placements**: Unicode placeholder-based rendering
- **Deletion**: By image ID, placement ID, cursor position, column, row, z-index, image number, cell+z filter, and ID range
- **Z-index layering**: Images render below or above text depending on z-index

## Usage

You can display images with tools that support the Kitty graphics protocol:

```bash
# Using kitty's built-in tool
kitty +kitten icat image.png

# Using other compatible tools
chafa --format=kitty image.png
timg -p kitty image.png
```

Applications like [notcurses](https://github.com/dankamongmen/notcurses) also use the Kitty graphics protocol for rich terminal UIs.

## Per-image GPU textures

Rio renders each Kitty image with its own dedicated GPU texture using a separate rendering pipeline from text. This ensures images don't leak GPU memory and are efficiently cleaned up when no longer displayed.

For more information on the protocol: [sw.kovidgoyal.net/kitty/graphics-protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/)
