#!/usr/bin/env bash

authorized_key_line() {
  local from=$1 command=$2 key=$3
  printf 'from="%s",command="%s",restrict %s\n' "$from" "$command" "$key"
}

install_authorized_key() {
  local target=$1 owner=$2 keyfile=$3 from=$4 command=$5
  local key body directory
  key=$(tr -d '\r' <"$keyfile")
  key=${key%$'\n'}
  [[ -n $key && $key != *$'\n'* ]] ||
    { echo "error: ssh public key must contain exactly one non-empty line" >&2; return 1; }
  [[ $key == ssh-* || $key == ecdsa-* || $key == sk-* ]] ||
    { echo "error: file does not look like an OpenSSH public key" >&2; return 1; }
  body=$(printf '%s\n' "$key" | awk '{print $2}')
  [[ -n $body ]] || { echo "error: ssh public key has no key body" >&2; return 1; }
  [[ $from != *'"'* && $from != *$'\n'* ]] ||
    { echo "error: key source address must not contain quotes or newlines" >&2; return 1; }

  directory=$(dirname "$target")
  if [[ -n $owner ]]; then
    install -d -o "$owner" -g "$owner" -m 700 "$directory"
  else
    install -d -m 700 "$directory"
  fi
  touch "$target"
  if [[ -n $owner ]]; then
    chown "$owner:$owner" "$target"
  fi
  chmod 600 "$target"

  if grep -qF -- "$body" "$target"; then
    echo "restricted key already present in $target"
    return 0
  fi
  authorized_key_line "$from" "$command" "$key" >>"$target"
  echo "restricted key installed in $target"
}
