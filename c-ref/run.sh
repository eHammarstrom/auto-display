#!/bin/bash

CFLAGS="-Wall -Werror -Wpedantic"

gcc $CFLAGS main.c -lXrandr -lX11

./a.out
