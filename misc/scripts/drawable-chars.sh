#!/bin/bash
#
# Show the codepoint ranges Rio renders with built-in "drawable" sprites
# (box-drawing, blocks, braille, and — as they land — sextants, octants,
# powerline, geometric shapes, branch glyphs). Run inside a Rio window to
# eyeball the sprite rasterizer.
#
# With `fonts.use-drawable-chars = true` (the default) these render
# procedurally and should be crisp + seamless in any font; ranges not yet
# implemented fall back to the active font's glyphs. Toggle the setting to
# compare sprite vs font rendering.

set -u

# Emit one UTF-8 character for a decimal codepoint. Pure shell arithmetic
# + octal printf escapes, so it works on the stock macOS bash (3.2) with
# no python/perl dependency.
emit() {
  local cp=$1
  if [ "$cp" -lt 128 ]; then
    printf "$(printf '\\%03o' "$cp")"
  elif [ "$cp" -lt 2048 ]; then
    printf "$(printf '\\%03o\\%03o' \
      $(((cp >> 6) | 0xC0)) $(((cp & 0x3F) | 0x80)))"
  elif [ "$cp" -lt 65536 ]; then
    printf "$(printf '\\%03o\\%03o\\%03o' \
      $(((cp >> 12) | 0xE0)) $(((cp >> 6 & 0x3F) | 0x80)) $(((cp & 0x3F) | 0x80)))"
  else
    printf "$(printf '\\%03o\\%03o\\%03o\\%03o' \
      $(((cp >> 18) | 0xF0)) $(((cp >> 12 & 0x3F) | 0x80)) \
      $(((cp >> 6 & 0x3F) | 0x80)) $(((cp & 0x3F) | 0x80)))"
  fi
}

# grid <hex-start> <hex-end> [cols]
# Print an inclusive codepoint range as a grid, `cols` glyphs per row.
grid() {
  local cp=$((16#$1)) end=$((16#$2)) cols=${3:-16} i=0
  while [ "$cp" -le "$end" ]; do
    emit "$cp"
    printf ' '
    i=$((i + 1))
    [ $((i % cols)) -eq 0 ] && printf '\n'
    cp=$((cp + 1))
  done
  [ $((i % cols)) -ne 0 ] && printf '\n'
  printf '\n'
}

hr() { printf '\n\033[1;36m%s\033[0m\n' "$1"; }

hr "Box Drawing  U+2500-257F"
grid 2500 257F 16

hr "Block Elements  U+2580-259F"
grid 2580 259F 16

hr "Braille  U+2800-28FF"
grid 2800 28FF 32

hr "Sextants (Legacy Computing)  U+1FB00-1FB3B"
grid 1FB00 1FB3B 16

hr "Octants (Legacy Computing Supplement)  U+1CD00-1CDE5"
grid 1CD00 1CDE5 32

hr "Powerline  U+E0B0-E0D4"
grid E0B0 E0D4 16

hr "Geometric Shapes  U+25A0-25FF"
grid 25A0 25FF 16

hr "Branch Drawing  U+F5D0-F60D"
grid F5D0 F60D 16

# Real-world scenes — literal glyphs, so adjacent cells must join cleanly.
hr "Scenes (joins, shades, gauges)"
printf '  \xe2\x95\xad'; printf '\xe2\x94\x80%.0s' $(seq 1 11); printf '\xe2\x95\xae'
printf '   \xe2\x94\x8c'; printf '\xe2\x94\x80%.0s' $(seq 1 11); printf '\xe2\x94\x90'
printf '   \xe2\x95\x94'; printf '\xe2\x95\x90%.0s' $(seq 1 11); printf '\xe2\x95\x97\n'
echo "  │  rounded  │   │  light    │   ║  double   ║"
printf '  \xe2\x95\xb0'; printf '\xe2\x94\x80%.0s' $(seq 1 11); printf '\xe2\x95\xaf'
printf '   \xe2\x94\x94'; printf '\xe2\x94\x80%.0s' $(seq 1 11); printf '\xe2\x94\x98'
printf '   \xe2\x95\x9a'; printf '\xe2\x95\x90%.0s' $(seq 1 11); printf '\xe2\x95\x9d\n'
echo
echo "  shade ramp:  ░▒▓█        eighths:  ▁▂▃▄▅▆▇█"
echo "  spinner:     ⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏"
echo "  powerline:   ▌ main ▶ feature ▶ "
echo
