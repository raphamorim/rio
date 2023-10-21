# check if stdout is a terminal...
if test -t 1; then

    # see if it supports colors...
    ncolors=$(tput colors)

    if test -n "$ncolors" && test $ncolors -ge 8; then
        bold="$(tput bold)"
        underline="$(tput smul)"
        standout="$(tput smso)"
        foreground="$(tput sgr0)"
        black="$(tput setaf 0)"
        red="$(tput setaf 1)"
        green="$(tput setaf 2)"
        yellow="$(tput setaf 3)"
        blue="$(tput setaf 4)"
        magenta="$(tput setaf 5)"
        cyan="$(tput setaf 6)"
        white="$(tput setaf 7)"
    fi
fi

echo "${foreground}foreground${foreground}"
echo "${black}black${foreground}"
echo "${red}red${foreground}"
echo "${green}green${foreground}"
echo "${yellow}yellow${foreground}"
echo "${blue}blue${foreground}"
echo "${white}white${foreground}"
echo "${magenta}magenta${foreground}"
echo "${cyan}cyan${foreground}"