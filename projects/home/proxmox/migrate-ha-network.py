#!/usr/bin/env python3
"""Rebase only known HA integration endpoints when HA moves to HAOS.

Run offline while HA Core is stopped. Preserve original file separately. No
credentials, entity IDs or unrelated integrations are rewritten.
"""
import argparse
import copy
import json
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit

LOCAL = {'127.0.0.1', 'localhost', '::1'}
URLS = {'music_assistant': ('url', {8095}), 'llama_cpp': ('base_url', {8642, 8643}),
        'matter': ('url', {5580})}


def migrate(document, host):
    result = copy.deepcopy(document)
    changed = 0
    for entry in result['data']['entries']:
        data = entry['data']
        if entry['domain'] == 'wyoming':
            if data.get('host') in LOCAL and data.get('port') in {10200, 10300, 10305}:
                data['host'] = host
                changed += 1
        elif entry['domain'] in URLS:
            key, ports = URLS[entry['domain']]
            url = urlsplit(data.get(key, ''))
            if url.hostname in LOCAL and url.port in ports:
                data[key] = urlunsplit(url._replace(netloc=f'{host}:{url.port}'))
                changed += 1
    return result, changed


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('destination', type=Path)
    parser.add_argument('--k3s-host', required=True)
    args = parser.parse_args()
    document, count = migrate(json.loads(args.source.read_text()), args.k3s_host)
    args.destination.write_text(json.dumps(document, ensure_ascii=False, indent=2) + '\n')
    args.destination.chmod(0o600)
    print('Rebased integration endpoints:', count)
