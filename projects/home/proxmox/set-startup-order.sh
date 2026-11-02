#!/usr/bin/env bash
# Separate host bootstrap: startup ordering requires global Sys.Modify in PVE.
set -euo pipefail
[[ $(id -u) == 0 ]]
qm config 100 | grep -q '^name: haos$'
qm config 101 | grep -q '^name: lab-k3s$'
pct config 103 | grep -q '^hostname: bb$'
qm set 100 --startup order=1,up=30,down=60
qm set 101 --startup order=2,up=30,down=90
pct set 103 --startup order=3,up=30,down=60
