#!/usr/bin/env bash
# Sign a Bazel-built APK with the personal release key and verify the result.
#
# Bazel (rules_android) can only produce debug-signed APKs: it always signs
# with `--ks-pass pass:android` and no alias. The release key therefore lives
# outside Bazel and is applied here, on the final artifact, with apksigner.
#
# Usage:
#   platform/android/sign-release.sh <in.apk> <out.apk>
#
# Environment:
#   SIGNING_KEYSTORE             keystore file (required, never committed)
#   SIGNING_STORE_PASSWORD_FILE  file with the keystore password (required
#                                unless SIGNING_STORE_PASSWORD is set)
#   SIGNING_STORE_PASSWORD       keystore password (avoid; prefer the file)
#   SIGNING_KEY_PASSWORD_FILE    file with the key password (default: the
#                                store password; PKCS12 uses one password)
#   SIGNING_KEY_PASSWORD         key password (avoid; prefer the file)
#   SIGNING_KEY_ALIAS            key alias (default: the only alias)
#   SIGNING_CERT_SHA256          expected signer certificate SHA-256, hex,
#                                colons optional (required; fail closed)
#   ANDROID_HOME / ANDROID_BUILD_TOOLS
#                                SDK root / build-tools directory (default
#                                $ANDROID_HOME/build-tools/36.0.0)
#
# Passwords travel to apksigner/keytool only as `file:` references to private
# temporary files; they never appear in argv or logs.
set -euo pipefail

usage() {
  sed -n '2,27p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

die() {
  echo "sign-release: $*" >&2
  exit 1
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi
if [[ $# -ne 2 ]]; then
  usage >&2
  exit 2
fi
IN_APK="$1"
OUT_APK="$2"

[[ -f "$IN_APK" ]] || die "input APK not found: $IN_APK"
[[ "$IN_APK" -ef "$OUT_APK" ]] && die "input and output APK must differ"

: "${SIGNING_KEYSTORE:?SIGNING_KEYSTORE must point at the release keystore}"
[[ -f "$SIGNING_KEYSTORE" ]] || die "keystore not found: $SIGNING_KEYSTORE"

# The expected signer is mandatory: a build signed by any other key must never
# be published, so a missing expectation is an error, not a warning.
EXPECTED_CERT="${SIGNING_CERT_SHA256:-}"
[[ -n "$EXPECTED_CERT" ]] || die "SIGNING_CERT_SHA256 must be set (fail closed)"
EXPECTED_CERT="$(printf '%s' "$EXPECTED_CERT" | tr -d ':' | tr '[:upper:]' '[:lower:]')"
[[ "$EXPECTED_CERT" =~ ^[0-9a-f]{64}$ ]] \
  || die "SIGNING_CERT_SHA256 is not a SHA-256 hex digest"

export ANDROID_HOME="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}"
BUILD_TOOLS="${ANDROID_BUILD_TOOLS:-$ANDROID_HOME/build-tools/36.0.0}"
for tool in apksigner zipalign; do
  [[ -x "$BUILD_TOOLS/$tool" ]] || die "missing $BUILD_TOOLS/$tool (set ANDROID_BUILD_TOOLS)"
done
command -v keytool >/dev/null 2>&1 || die "keytool not on PATH (install a JDK)"

WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/sign-release.XXXXXX")"
chmod 700 "$WORK_DIR"
cleanup() {
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

# Copy a password into a private file without its trailing newline. Command
# substitution strips newlines, so `printf '%s'` yields the exact value.
password_file() {
  local label="$1" from_file="$2" from_env="$3" target="$4"
  local value=""
  if [[ -n "$from_file" ]]; then
    [[ -f "$from_file" ]] || die "$label password file not found: $from_file"
    value="$(cat "$from_file")"
  elif [[ -n "$from_env" ]]; then
    value="$from_env"
  else
    return 1
  fi
  [[ -n "$value" ]] || die "$label password is empty"
  (umask 077 && printf '%s' "$value" > "$target")
  return 0
}

STORE_PASS_FILE="$WORK_DIR/store.pass"
KEY_PASS_FILE="$WORK_DIR/key.pass"
password_file "keystore" "${SIGNING_STORE_PASSWORD_FILE:-}" "${SIGNING_STORE_PASSWORD:-}" \
  "$STORE_PASS_FILE" || die "set SIGNING_STORE_PASSWORD_FILE (or SIGNING_STORE_PASSWORD)"
if ! password_file "key" "${SIGNING_KEY_PASSWORD_FILE:-}" "${SIGNING_KEY_PASSWORD:-}" \
  "$KEY_PASS_FILE"; then
  cp "$STORE_PASS_FILE" "$KEY_PASS_FILE"
fi

ALIAS="${SIGNING_KEY_ALIAS:-}"
if [[ -z "$ALIAS" ]]; then
  # A PKCS12 keystore normally holds one key entry. Refuse to guess when the
  # keystore holds several, so the published build cannot silently switch key.
  aliases=()
  while IFS= read -r line; do
    case "$line" in
      "Alias name: "*) aliases+=("${line#Alias name: }") ;;
    esac
  done < <(keytool -list -v -keystore "$SIGNING_KEYSTORE" \
    -storepass:file "$STORE_PASS_FILE" 2>/dev/null)
  case "${#aliases[@]}" in
    0) die "keystore has no key entry (wrong password or empty keystore)" ;;
    1) ALIAS="${aliases[0]}" ;;
    *) die "keystore holds ${#aliases[@]} entries; set SIGNING_KEY_ALIAS" ;;
  esac
fi
[[ -n "$ALIAS" ]] || die "empty key alias"

ALIGNED_APK="$WORK_DIR/aligned.apk"
SIGNED_APK="$WORK_DIR/signed.apk"

echo "zipalign (16 KiB pages): $(basename "$IN_APK")"
"$BUILD_TOOLS/zipalign" -P 16 -f 4 "$IN_APK" "$ALIGNED_APK"

echo "apksigner: alias $ALIAS"
"$BUILD_TOOLS/apksigner" sign \
  --ks "$SIGNING_KEYSTORE" \
  --ks-key-alias "$ALIAS" \
  --ks-pass "file:$STORE_PASS_FILE" \
  --key-pass "file:$KEY_PASS_FILE" \
  --v1-signing-enabled false \
  --v2-signing-enabled true \
  --v3-signing-enabled true \
  --out "$SIGNED_APK" \
  "$ALIGNED_APK"

# Full integrity check of the produced artifact before it is published.
"$BUILD_TOOLS/apksigner" verify "$SIGNED_APK"

ACTUAL_CERT="$("$BUILD_TOOLS/apksigner" verify --print-certs "$SIGNED_APK" \
  | sed -n 's/^Signer #1 certificate SHA-256 digest: //p' | tr '[:upper:]' '[:lower:]')"
[[ -n "$ACTUAL_CERT" ]] || die "could not read the signer certificate"
if [[ "$ACTUAL_CERT" != "$EXPECTED_CERT" ]]; then
  die "signer is $ACTUAL_CERT, expected $EXPECTED_CERT"
fi

mkdir -p "$(dirname "$OUT_APK")"
cp "$SIGNED_APK" "$OUT_APK"
echo "signed $(basename "$OUT_APK"), signer SHA-256 $ACTUAL_CERT"
