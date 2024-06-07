#!/bin/bash

echo -e "regular"
echo -e "\e[1mbold\e[0m"
echo -e "\e[3mitalic\e[0m"
echo -e "\e[4munderline\e[0m"
echo -e "\e[9mstrikethrough\e[0m"

# https://sw.kovidgoyal.net/kitty/underlines/
echo -e ""
echo -e "\e[4:0mno underline\e[0m"
echo -e "\e[4:1mstraight underline\e[0m"
echo -e "\e[4:2mdouble underline\e[0m"
echo -e "\e[4:3mcurly underline\e[0m"
echo -e "\e[4:4mdotted underline\e[0m"
echo -e "\e[4:5mdashed underline\e[0m"
