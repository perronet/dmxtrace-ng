#!/bin/bash

# To test the script use INSTALL_PATH=/tmp/install

mkdir -p output

# Clean previous output
make -C libtraceevent clean
make -C libtracefs clean
make -C trace-cmd clean
rm -rf output/*

CUSTOM_PATH=$(pwd)/output

cd libtraceevent
INSTALL_PATH="$CUSTOM_PATH" ../trace-cmd/make-trace-cmd.sh install
if (("$?" != 0)); then
    echo "Build error in libtraceevent."
    exit 1
fi

cd ../libtracefs
INSTALL_PATH="$CUSTOM_PATH" ../trace-cmd/make-trace-cmd.sh install
if (("$?" != 0)); then
    echo "Build error in libtracefs."
    exit 1
fi

cd ../trace-cmd
INSTALL_PATH="$CUSTOM_PATH" ./make-trace-cmd.sh install_libs
if (("$?" != 0)); then
    echo "Build error in libtracecmd."
    exit 1
fi

cd ..
make -C libtraceevent clean
make -C libtracefs clean
make -C trace-cmd clean