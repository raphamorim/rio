#!/bin/bash

i=1
while [ $i -le 100000000000 ]
do
    echo $i
    i=`expr $i + 1`
done