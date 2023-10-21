#!/bin/bash
color(){
    for c; do
        printf '\e[48;5;%dm%03d' $c $c
    done
    printf '\e[0m \n'
}

IFS=$' \t\n'
color {0..15}
for ((i=0;i<6;i++)); do
    color $(seq $((i*36+16)) $((i*36+51)))
done
color {232..255}