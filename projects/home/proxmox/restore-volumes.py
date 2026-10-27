#!/usr/bin/env python3
"""Restore PVC directories to a fresh guest without importing old cluster identity.

Run as root before starting workloads. Archive must be the verified preinstall
snapshot. Existing destination is refused; outer root-only archive stays private.
"""
import argparse
import json
import os
import posixpath
import re
from pathlib import Path, PurePosixPath
import subprocess
import tarfile


def main(archive, destination, manifest):
    if os.geteuid() != 0:
        raise SystemExit('Run as root')
    if destination.exists():
        raise SystemExit('Refusing to overwrite existing volume directories')
    with tarfile.open(archive) as tar:
        members = tar.getmembers()
        roots = {m.name.split('/')[0] for m in members}
        if len(roots) != 1:
            raise ValueError('Expected one snapshot root')
        root = roots.pop()
        volumes = json.load(tar.extractfile(root + '/volumes.json'))
        interpreter_links = []
        for member in members:
            parts = PurePosixPath(member.name).parts
            if '..' in parts or member.name.startswith('/'):
                raise ValueError('Unsafe archive path')
            if member.name.startswith(root + '/volumes/') and (member.issym() or member.islnk()):
                target = PurePosixPath(member.linkname)
                base = posixpath.dirname(member.name) if member.issym() else ''
                resolved = posixpath.normpath(posixpath.join(base, member.linkname))
                volume_root = '/'.join(parts[:3]) + '/'
                if target.is_absolute():
                    # Native Hermes tool venvs retain absolute Python launchers.
                    # Restore them against the pinned container interpreter,
                    # never recreate links into the old user's home.
                    if (parts[2] == 'runtime-hermes' and member.issym()
                            and re.fullmatch(r'python(?:3(?:\.\d+)?)?', parts[-1])
                            and re.fullmatch(r'/(?:home|usr|opt)/[^\n]+/python3(?:\.\d+)?', member.linkname)):
                        interpreter_links.append(member.name)
                        continue
                    raise ValueError('Unsafe volume link')
                if not resolved.startswith(volume_root):
                    raise ValueError('Unsafe volume link')
    destination.mkdir(parents=True, mode=0o700)
    exclusions = ['--exclude=' + name for name in interpreter_links]
    subprocess.run(['tar', *exclusions, '-xzf', str(archive), '--same-owner', '--strip-components=2',
                    '-C', str(destination), root + '/volumes'], check=True)
    # Containers keep their source UID/GID/mode; make only the outer directory
    # traversable so non-root containers can reach their own volume directories.
    destination.chmod(0o755)
    for name in interpreter_links:
        path = destination.joinpath(*PurePosixPath(name).parts[2:])
        path.symlink_to('/usr/local/bin/python3')
    print('Rebased Hermes tool interpreter links:', len(interpreter_links))
    telemetry = []
    for volume in volumes:
        if volume['status']['phase'] != 'Bound':
            continue
        path = destination / volume['metadata']['name']
        if not path.is_dir():
            raise ValueError('Missing bound volume: ' + path.name)
        claim = volume['spec']['claimRef']
        if claim['namespace'] != 'monitoring':
            continue  # Application PVs/PVCs are owned by the runtime Helm chart.
        telemetry.append({
            'apiVersion': 'v1', 'kind': 'PersistentVolume',
            'metadata': {'name': volume['metadata']['name']},
            'spec': {
                'capacity': volume['spec']['capacity'],
                'accessModes': ['ReadWriteOnce'],
                'persistentVolumeReclaimPolicy': 'Retain',
                'storageClassName': 'local-retain',
                'local': {'path': str(path)},
                'claimRef': {'namespace': claim['namespace'], 'name': claim['name']},
                'nodeAffinity': {'required': {'nodeSelectorTerms': [{'matchExpressions': [{
                    'key': 'kubernetes.io/hostname', 'operator': 'In', 'values': ['lab-host']
                }]}]}}
            }
        })
    manifest.write_text(json.dumps({'apiVersion': 'v1', 'kind': 'List', 'items': telemetry}, indent=2))
    manifest.chmod(0o600)
    print('Restored bound volumes; prepared', len(telemetry), 'prebound telemetry PVs')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('archive', type=Path)
    parser.add_argument('--destination', type=Path, default=Path('/srv/k3s/volumes'))
    parser.add_argument('--manifest', type=Path, required=True)
    args = parser.parse_args()
    main(args.archive.resolve(), args.destination.resolve(), args.manifest.resolve())
