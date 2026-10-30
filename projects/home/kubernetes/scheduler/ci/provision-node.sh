#!/usr/bin/env bash
# Provision the least-privilege deploy account on the K3s node.
#
# Run as root on the node (or via `ssh root@node bash -s`). Idempotent: it
# recreates the account, the fixed helper, the sudoers entry and the one
# authorized key, and leaves every other account untouched.
#
#   provision-node.sh --pubkey-file FILE --vpn-source ADDRESS [--user ci-deploy]
#
# --vpn-source is the source address the VPN container NATs CI clients to
# (the VPN CT address), not the CI pool address.
set -euo pipefail
umask 027

USER_NAME=ci-deploy
VPN_SOURCE=
PUBKEY_FILE=
PUBKEY_TEXT=

usage() {
  sed -n '2,12p' "$0" >&2
  exit 2
}

die() { echo "error: $*" >&2; exit 1; }

while (($#)); do
  case "$1" in
    --pubkey-file) PUBKEY_FILE=$2; shift 2 ;;
    --pubkey) PUBKEY_TEXT=$2; shift 2 ;;
    --vpn-source) VPN_SOURCE=$2; shift 2 ;;
    --user) USER_NAME=$2; shift 2 ;;
    -h|--help) usage ;;
    *) die "unknown argument: $1" ;;
  esac
done

[[ $(id -u) == 0 ]] || die "run as root"
[[ -n $VPN_SOURCE ]] || die "--vpn-source is required"
if [[ -z $PUBKEY_TEXT ]]; then
  [[ -n $PUBKEY_FILE ]] || die "--pubkey-file or --pubkey is required"
  PUBKEY_TEXT=$(cat "$PUBKEY_FILE")
fi
[[ $PUBKEY_TEXT == ssh-ed25519\ * ]] || die "only ssh-ed25519 keys are accepted"
[[ $VPN_SOURCE =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "bad --vpn-source"
[[ $USER_NAME =~ ^[a-z_][a-z0-9_-]*$ ]] || die "bad --user"

if ! id "$USER_NAME" >/dev/null 2>&1; then
  useradd --create-home --shell /bin/bash "$USER_NAME"
fi
# No password login ever: lock the password field explicitly.
usermod --password '!' "$USER_NAME"

install -d -m 0700 -o "$USER_NAME" -g "$(id -gn "$USER_NAME")" /var/lib/ricardo-ci
install -d -m 0700 -o "$USER_NAME" -g "$(id -gn "$USER_NAME")" /var/lib/ricardo-ci/stage

SOURCE_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

install -m 0755 "$SOURCE_DIR/ricardo-ci-deploy" /usr/local/sbin/ricardo-ci-deploy
install -d -m 0755 /usr/local/share/ricardo-ci
install -m 0644 -o root -g root "$SOURCE_DIR/scheduler-nodeport.nft.tmpl" \
  /usr/local/share/ricardo-ci/scheduler-nodeport.nft.tmpl
install -m 0440 -o root -g root "$SOURCE_DIR/sudoers-ci-deploy" /etc/sudoers.d/ricardo-ci-deploy
visudo -cf /etc/sudoers.d/ricardo-ci-deploy >/dev/null

HOME_DIR=$(getent passwd "$USER_NAME" | cut -d: -f6)
install -d -m 0700 -o "$USER_NAME" -g "$(id -gn "$USER_NAME")" "$HOME_DIR/.ssh"
printf '%s\n' \
  "from=\"$VPN_SOURCE\",restrict,port-forwarding,permitopen=\"127.0.0.1:6443\" $PUBKEY_TEXT" \
  >"$HOME_DIR/.ssh/authorized_keys"
chown "$USER_NAME:$(id -gn "$USER_NAME")" "$HOME_DIR/.ssh/authorized_keys"
chmod 0600 "$HOME_DIR/.ssh/authorized_keys"

install -d -m 0755 /opt/ricardo/releases /opt/ricardo/control/releases

if systemctl is-active --quiet ssh; then
  systemctl reload ssh
elif systemctl is-active --quiet sshd; then
  systemctl reload sshd
fi

echo "provisioned $USER_NAME; allowed source $VPN_SOURCE; sudo: ricardo-ci-deploy only"
