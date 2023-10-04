#!/bin/bash

export LD_LIBRARY_PATH=/opt/tritium/gstreamer/lib/x86_64-linux-gnu/ 
export PKG_CONFIG_PATH=/opt/tritium/gstreamer/lib/x86_64-linux-gnu/pkgconfig/

cargo run 