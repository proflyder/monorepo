#!/usr/bin/env bash
# Build the Home Android app and distribute it to Firebase App Distribution.
#
# Runs locally and in the `android-distribute` workflow. Requires the private
# files
#   platform/android/keys/home-debug.keystore
#   platform/android/keys/google-services.json
# (see projects/mobile/android/README.md). CI materialises both from OpenBao;
# locally they are links under secrets/android/. The APK must be signed with
# the personal debug key: distributing the public fallback key would produce
# builds that cannot update an existing install.
#
# Authentication uses an existing `npx firebase-tools login` session or
# GOOGLE_APPLICATION_CREDENTIALS (a service-account JSON). Credentials are
# never printed.
#
# Usage:
#   projects/mobile/android/distribute.sh [--dry-run] [--ci]
#
# --ci formats release notes for a GitHub Actions push run (commit subject,
# short SHA and run URL from the GitHub environment).
set -euo pipefail

FIREBASE_TOOLS_VERSION="15.34.2"
CERT_SHA256="88ef5d61a365afbf15d290168d4ee7fd20a80be13ea4d4667ef1e521a2a504ad"
APP_ID_PACKAGE="dev.proflyder.home"
TESTER_GROUP="${HOME_TESTERS_GROUP:-owner}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
ANDROID_DIR="$ROOT/projects/mobile/android"
KEYSTORE_LINK="$ROOT/platform/android/keys/home-debug.keystore"
GOOGLE_SERVICES_LINK="$ROOT/platform/android/keys/google-services.json"
APK="$ROOT/bazel-bin/projects/mobile/android/app/app.apk"

DRY_RUN=0
CI_MODE=0
for argument in "$@"; do
  case "$argument" in
    --dry-run) DRY_RUN=1 ;;
    --ci) CI_MODE=1 ;;
    -h|--help)
      sed -n '2,21p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "Unknown argument: $argument" >&2
      exit 2
      ;;
  esac
done

if [[ ! -e "$KEYSTORE_LINK" ]]; then
  echo "Missing $KEYSTORE_LINK" >&2
  echo "Link secrets/android/home-debug.keystore to it; see projects/mobile/android/README.md." >&2
  exit 1
fi
if [[ ! -e "$GOOGLE_SERVICES_LINK" ]]; then
  echo "Missing $GOOGLE_SERVICES_LINK" >&2
  echo "Link secrets/android/google-services.json to it; see projects/mobile/android/README.md." >&2
  exit 1
fi

export ANDROID_HOME="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}"
BUILD_TOOLS="${ANDROID_BUILD_TOOLS:-$ANDROID_HOME/build-tools/36.0.0}"
for tool in apksigner aapt2; do
  if [[ ! -x "$BUILD_TOOLS/$tool" ]]; then
    echo "Missing $BUILD_TOOLS/$tool (set ANDROID_BUILD_TOOLS)." >&2
    exit 1
  fi
done

# Local worktrees use `bazelisk`; GitHub's setup-bazel exposes the same pinned
# Bazelisk as `bazel`.
if command -v bazelisk >/dev/null 2>&1; then
  BAZEL=bazelisk
elif command -v bazel >/dev/null 2>&1; then
  BAZEL=bazel
else
  echo "Neither bazelisk nor bazel is on PATH." >&2
  exit 1
fi

# Each distribution needs a higher versionCode. UTC epoch seconds are
# strictly increasing per build, time-ordered, and fit int32 until 2038.
# A git commit count is not monotonic for dirty/uncommitted builds and can
# collide with unrelated repositories.
VERSION_CODE="$(date -u +%s)"
SHORT_SHA="$(git -C "$ROOT" rev-parse --short HEAD)"
VERSION_NAME="0.1.0-$SHORT_SHA"

echo "Building //projects/mobile/android:app (versionCode $VERSION_CODE, versionName $VERSION_NAME)"
(
  cd "$ROOT"
  "$BAZEL" build //projects/mobile/android:app \
    --define "home_version_code=$VERSION_CODE" \
    --define "home_version_name=$VERSION_NAME"
)

if [[ ! -f "$APK" ]]; then
  echo "APK not found at $APK" >&2
  exit 1
fi

SIGNER_SHA256="$("$BUILD_TOOLS/apksigner" verify --print-certs "$APK" \
  | sed -n 's/^Signer #1 certificate SHA-256 digest: //p')"
if [[ "$SIGNER_SHA256" != "$CERT_SHA256" ]]; then
  echo "Refusing to distribute: APK signer is $SIGNER_SHA256" >&2
  echo "Expected the personal debug key ($CERT_SHA256). The public fallback key cannot update an existing install." >&2
  exit 1
fi

APP_ID="$(python3 - "$GOOGLE_SERVICES_LINK" "$APP_ID_PACKAGE" <<'PY'
import json
import sys

path, package = sys.argv[1], sys.argv[2]
with open(path, encoding="utf-8") as handle:
    config = json.load(handle)
for client in config.get("client", []):
    info = client.get("client_info", {})
    if info.get("android_client_info", {}).get("package_name") == package:
        print(info.get("mobilesdk_app_id", ""))
        break
else:
    sys.exit("google-services.json has no client for %s" % package)
PY
)"
if [[ -z "$APP_ID" ]]; then
  echo "Could not read mobilesdk_app_id for $APP_ID_PACKAGE" >&2
  exit 1
fi

if [[ "$CI_MODE" -eq 1 ]]; then
  # A push to develop is a squash merge: its subject is the PR title and the
  # commit list of the merge itself is a single line.
  COMMIT_SUBJECT="$(git -C "$ROOT" log -1 --pretty=%s)"
  RUN_URL="${GITHUB_RUN_URL:-}"
  if [[ -z "$RUN_URL" && -n "${GITHUB_SERVER_URL:-}" && -n "${GITHUB_REPOSITORY:-}" \
        && -n "${GITHUB_RUN_ID:-}" ]]; then
    RUN_URL="$GITHUB_SERVER_URL/$GITHUB_REPOSITORY/actions/runs/$GITHUB_RUN_ID"
  fi
  RELEASE_NOTES="$COMMIT_SUBJECT"$'\n'"$SHORT_SHA"
  if [[ -n "$RUN_URL" ]]; then
    RELEASE_NOTES="$RELEASE_NOTES"$'\n'"$RUN_URL"
  fi
else
  RELEASE_NOTES="$(git -C "$ROOT" describe --tags --always --dirty)"
  RELEASE_NOTES="$RELEASE_NOTES"$'\n'"$(git -C "$ROOT" log --oneline -5)"
fi

echo "APK signer verified: $SIGNER_SHA256"
echo "Firebase app: $APP_ID, group: $TESTER_GROUP"
echo "Release notes:"
printf '%s\n' "$RELEASE_NOTES" | sed 's/^/  /'

FIREBASE_ARGS=(
  "--yes" "firebase-tools@$FIREBASE_TOOLS_VERSION"
  appdistribution:distribute "$APK"
  --app "$APP_ID"
  --groups "$TESTER_GROUP"
  --release-notes "$RELEASE_NOTES"
)

if [[ "$DRY_RUN" -eq 1 ]]; then
  echo "Dry run: would execute"
  printf '  npx'; printf ' %q' "${FIREBASE_ARGS[@]}"; printf '\n'
  echo "Dry run: skipped authentication and upload."
  exit 0
fi

cd "$ROOT"
npx "${FIREBASE_ARGS[@]}"
