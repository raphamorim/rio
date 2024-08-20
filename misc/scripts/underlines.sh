# Script retired from https://github.com/pop-os/cosmic-term/blob/be808b56cf24d03fc99cf44b0885078a81a16523/ansi-colors.sh#L64
# which licensed under GNU 3.0 https://github.com/pop-os/cosmic-term/blob/master/LICENSE

#!/usr/bin/env bash

set -e

CNAMES=("BLK" "RED" "GRN" "YEL" "BLU" "MAG" "CYN" "WHT")

printf "\033[1m" # bold
printf "\nUnderline With FG Colors:\n"

printf "\033[4m" # underline
for foreground in $(seq 0 7)
do
    printf "\033[$((foreground+30))m ${CNAMES[$foreground]} "
done
printf "\x1B[24m\n" # no underline

printf "\nUnderline Styles And Colors:\n"

printf "\nFG:  "
printf "\033[9mStrikeout\033[0m "
printf "\033[4mUnderline\033[0m "
printf "\033[4:2mDoubleUnderline\033[0m "
printf "\033[4:3mCurlyUnderline\033[0m "
printf "\033[4:4mDottedUnderline\033[0m "
printf "\033[4:5mDashedUnderline\033[0m "
printf "\n"

printf "INV: "
printf "\033[7m\033[9mStrikeout\033[0m "
printf "\033[7m\033[4mUngderline\033[0m "
printf "\033[7m\033[4:2mDoubleUnderline\033[0m "
printf "\033[7m\033[4:3mCurlyUnderline\033[0m "
printf "\033[7m\033[4:4mDottedUnderline\033[0m "
printf "\033[7m\033[4:5mDashedUnderline\033[0m "
printf "\n"

for line_color in $(seq 0 7)
do
    printf "${CNAMES[$line_color]}: "
    printf "          "
    printf "\033[58:5:"${line_color}m
    printf "\033[4mUnderline\033[0m "
    printf "\033[58:5:"${line_color}m
    printf "\033[4:2mDoubleUnderline\033[0m "
    printf "\033[58:5:"${line_color}m
    printf "\033[4:3mCurlyUnderline\033[0m "
    printf "\033[58:5:"${line_color}m
    printf "\033[4:4mDottedUnderline\033[0m "
    printf "\033[58:5:"${line_color}m
    printf "\033[4:5mDashedUnderline\033[0m "
    printf "\n"
done
