#!/bin/bash
# Resolve through the ~/flightgear symlink to find the repo checkout.
REPO="$(readlink -f "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/..")"
export XR_RUNTIME_JSON=/var/lib/flatpak/app/io.github.wivrn.wivrn/current/active/files/share/openxr/1/openxr_wivrn.json

fgfs --launcher \
     --httpd=8080 \
     --addon="$REPO/addon" \
     --enable-vr \
     --prop:/sim/current-view/y-offset-m=0.2