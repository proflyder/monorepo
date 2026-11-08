#!/usr/bin/env bash
# Self-test for platform/android/sign-release.sh.
#
# Creates a throwaway keystore with a strong password, takes any small APK
# (SIGN_RELEASE_TEST_APK, an existing Bazel build, or a minimal APK built here
# with aapt2), signs it through sign-release.sh, and checks that:
#   * the output verifies and carries the expected signer certificate,
#   * v2/v3 signatures are present,
#   * signing fails closed when SIGNING_CERT_SHA256 is wrong or missing.
#
# Exits 0 with a "SKIP" message when the Android build-tools are unavailable,
# because the script under test cannot run without them.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SIGN_SCRIPT="$ROOT/platform/android/sign-release.sh"
TEST_PASSWORD="sign-release-test-$(date +%s)-Aa1!x"

export ANDROID_HOME="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}"
BUILD_TOOLS="${ANDROID_BUILD_TOOLS:-$ANDROID_HOME/build-tools/36.0.0}"

for tool in apksigner zipalign aapt2; do
  if [[ ! -x "$BUILD_TOOLS/$tool" ]]; then
    echo "SKIP: $BUILD_TOOLS/$tool is missing; cannot exercise sign-release.sh"
    exit 0
  fi
done
command -v keytool >/dev/null 2>&1 || {
  echo "SKIP: keytool is missing; cannot create a throwaway keystore"
  exit 0
}

WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/sign-release-test.XXXXXX")"
cleanup() {
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

# --- Throwaway release keystore -------------------------------------------

KEYSTORE="$WORK_DIR/home-release.keystore"
STORE_PASS_FILE="$WORK_DIR/store.pass"
KEY_PASS_FILE="$WORK_DIR/key.pass"
ALIAS="home-release"
# Files end with a newline on purpose: sign-release.sh must trim it rather
# than feed it to apksigner.
printf '%s\n' "$TEST_PASSWORD" > "$STORE_PASS_FILE"
printf '%s\n' "$TEST_PASSWORD" > "$KEY_PASS_FILE"
keytool -genkeypair \
  -keystore "$KEYSTORE" \
  -storetype PKCS12 \
  -storepass:file "$STORE_PASS_FILE" \
  -keypass:file "$KEY_PASS_FILE" \
  -alias "$ALIAS" \
  -keyalg RSA -keysize 2048 -validity 365 \
  -dname "CN=Sign Release Test" >/dev/null 2>&1 \
  || fail "keytool could not create the test keystore"

EXPECTED_CERT="$(keytool -list -v -keystore "$KEYSTORE" -storepass:file "$STORE_PASS_FILE" \
  | sed -n 's/^[[:space:]]*SHA256: //p' | head -n 1 | tr -d ':' | tr '[:upper:]' '[:lower:]')"
[[ "$EXPECTED_CERT" =~ ^[0-9a-f]{64}$ ]] || fail "could not read the test certificate digest"

# --- Input APK -------------------------------------------------------------

INPUT_APK=""
if [[ -n "${SIGN_RELEASE_TEST_APK:-}" ]]; then
  [[ -f "$SIGN_RELEASE_TEST_APK" ]] || fail "SIGN_RELEASE_TEST_APK not found: $SIGN_RELEASE_TEST_APK"
  INPUT_APK="$SIGN_RELEASE_TEST_APK"
elif [[ -f "$ROOT/bazel-bin/projects/mobile/android/app/app.apk" ]]; then
  INPUT_APK="$ROOT/bazel-bin/projects/mobile/android/app/app.apk"
else
  ANDROID_JAR=""
  for candidate in "$ANDROID_HOME"/platforms/android-*/android.jar; do
    [[ -f "$candidate" ]] && ANDROID_JAR="$candidate"
  done
  [[ -n "$ANDROID_JAR" ]] || fail \
    "no Android platform jar to build a test APK (install platforms;android-36, or set SIGN_RELEASE_TEST_APK)"
  cat > "$WORK_DIR/AndroidManifest.xml" <<'XML'
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="dev.proflyder.home.signreltest"
    android:versionCode="1"
    android:versionName="1.0" />
XML
  "$BUILD_TOOLS/aapt2" link \
    -o "$WORK_DIR/input.apk" \
    --manifest "$WORK_DIR/AndroidManifest.xml" \
    -I "$ANDROID_JAR" \
    --min-sdk-version 26 \
    --target-sdk-version 36 \
    || fail "aapt2 could not build the test APK"
  INPUT_APK="$WORK_DIR/input.apk"
fi
echo "test APK: $INPUT_APK"

sign() {
  env \
    SIGNING_KEYSTORE="$KEYSTORE" \
    SIGNING_STORE_PASSWORD_FILE="$STORE_PASS_FILE" \
    SIGNING_KEY_PASSWORD_FILE="$KEY_PASS_FILE" \
    SIGNING_CERT_SHA256="$1" \
    "${@:2}" \
    "$SIGN_SCRIPT" "$INPUT_APK" "$WORK_DIR/signed.apk"
}

# --- Happy path: alias discovered from the keystore (no SIGNING_KEY_ALIAS) --

sign "$EXPECTED_CERT" >/dev/null || fail "sign-release.sh failed for a valid keystore"
[[ -f "$WORK_DIR/signed.apk" ]] || fail "sign-release.sh produced no APK"

"$BUILD_TOOLS/apksigner" verify "$WORK_DIR/signed.apk" >/dev/null \
  || fail "the signed APK does not verify"

ACTUAL_CERT="$("$BUILD_TOOLS/apksigner" verify --print-certs "$WORK_DIR/signed.apk" \
  | sed -n 's/^Signer #1 certificate SHA-256 digest: //p' | tr '[:upper:]' '[:lower:]')"
[[ "$ACTUAL_CERT" == "$EXPECTED_CERT" ]] \
  || fail "signer certificate is $ACTUAL_CERT, expected $EXPECTED_CERT"

"$BUILD_TOOLS/apksigner" verify --verbose "$WORK_DIR/signed.apk" \
  | grep -q '^Verified using v2 scheme (APK Signature Scheme v2): true$' \
  || fail "the signed APK has no v2 signature"
"$BUILD_TOOLS/apksigner" verify --verbose "$WORK_DIR/signed.apk" \
  | grep -q '^Verified using v3 scheme (APK Signature Scheme v3): true$' \
  || fail "the signed APK has no v3 signature"

# An explicit alias must work too.
sign "$EXPECTED_CERT" SIGNING_KEY_ALIAS="$ALIAS" >/dev/null \
  || fail "sign-release.sh failed with an explicit SIGNING_KEY_ALIAS"

# --- Fail closed ----------------------------------------------------------

WRONG_CERT="$(printf '0%.0s' $(seq 1 64))"
if sign "$WRONG_CERT" >/dev/null 2>&1; then
  fail "sign-release.sh accepted a different SIGNING_CERT_SHA256"
fi

if sign "" >/dev/null 2>&1; then
  fail "sign-release.sh accepted a missing SIGNING_CERT_SHA256"
fi

if env \
    SIGNING_KEYSTORE="$KEYSTORE" \
    SIGNING_STORE_PASSWORD_FILE="$WORK_DIR/missing.pass" \
    SIGNING_CERT_SHA256="$EXPECTED_CERT" \
    "$SIGN_SCRIPT" "$INPUT_APK" "$WORK_DIR/signed.apk" >/dev/null 2>&1; then
  fail "sign-release.sh accepted a missing password file"
fi

echo "PASS: sign-release.sh signs, verifies and fails closed (test key $EXPECTED_CERT)"
