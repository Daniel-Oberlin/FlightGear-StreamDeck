#!/bin/bash
# Resolve through the ~/flightgear symlink to find the repo checkout.
REPO="$(readlink -f "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/..")"
fgfs --launcher \
     --httpd=8080 \
     --addon="$REPO/addon" \
     --disable-vr \
     --prop:/sim/current-view/y-offset-m=0.0