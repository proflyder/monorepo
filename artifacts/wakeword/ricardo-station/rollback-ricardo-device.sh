#!/bin/sh
set -eu
cd /data/pulsar
cp backups/pre-ricardo/pulsar pulsar.restore
chmod 755 pulsar.restore
mv pulsar.restore pulsar
sed -i 's/^wake_word=.*/wake_word=alisa/' state.conf
./ctl.sh restart
