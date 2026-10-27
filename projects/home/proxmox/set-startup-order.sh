#!/usr/bin/env bash
# Separate host bootstrap: startup ordering requires global Sys.Modify in PVE.
set -euo pipefail
[[ $(id -u) == 0 ]]
qm config 100 | grep -q '^name: haos$'
qm config 101 | grep -q '^name: lab-k3s$'
qm set 100 --startup order=1,up=30,down=60
qm set 101 --startup order=2,up=30,down=90
