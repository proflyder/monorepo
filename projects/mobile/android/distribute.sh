#!/usr/bin/env bash
# Build the Home Android app and distribute it to Firebase App Distribution.
#
# Two modes:
#   * default — build, sign and upload. Runs locally through
#     `mise run admin:android-distribute` (and any manual run).
#   * `--upload-only` — upload an APK that was already built, tested and signed
#     elsewhere. CI's `android-distribute` job uses this on the artifact of the
#     `android-app` matrix entry, so the APK is built exactly once.
#
# The build mode requires the private files
#   secrets/android/home-release.keystore        (and its password files)
#   platform/android/keys/google-services.json
# (see projects/mobile/android/README.md). Locally
# `mise run admin:android-distribute` passes them from OpenBao (fnox). Bazel only
# produces a debug-signed APK; the release key is applied to that artifact by
# `platform/android/sign-release.sh`, which also refuses to publish a build
# whose signer certificate is unexpected.
#
# Authentication uses an existing `npx firebase-tools login` session or
# GOOGLE_APPLICATION_CREDENTIALS (a service-account JSON). Credentials are
# never printed.
#
# Usage:
#   projects/mobile/android/distribute.sh [--dry-run] [--ci]
#   projects/mobile/android/distribute.sh [--dry-run] [--ci] \
#     --upload-only --release-info <release-info.json>
#
# --upload-only reads the APK name, its SHA-256 and the Firebase app id from
# `release-info.json` (written next to the APK by the building job; see
# release_info.py), verifies the digest and uploads. It never builds, signs or
# reads `google-services.json`.
#
# --ci formats release notes for a GitHub Actions run: for a pull request
# `PR #<number>: <title>` (read from PR_NUMBER/PR_TITLE), otherwise the commit
# subject; plus the short SHA and the run URL from the GitHub environment.
set -euo pipefail

FIREBASE_TOOLS_VERSION="15.34.2"
APP_ID_PACKAGE="dev.proflyder.home"
TESTER_GROUP="${HOME_TESTERS_GROUP:-admins}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
RELEASE_INFO_PY="$ROOT/projects/mobile/android/release_info.py"
GOOGLE_SERVICES_LINK="$ROOT/platform/android/keys/google-services.json"
APK="$ROOT/bazel-bin/projects/mobile/android/app/app.apk"
RELEASE_APK="$ROOT/bazel-bin/projects/mobile/android/app/app-release.apk"
SIGN_SCRIPT="$ROOT/platform/android/sign-release.sh"
SECRETS_ANDROID="$ROOT/secrets/android"

DRY_RUN=0
CI_MODE=0
UPLOAD_ONLY=0
SIGNING_READY=0
RELEASE_INFO=""
while [[ $# -gt 0 ]]; do
  argument="$1"
  case "$argument" in
    --dry-run) DRY_RUN=1 ;;
    --ci) CI_MODE=1 ;;
    --upload-only) UPLOAD_ONLY=1 ;;
    --release-info)
      [[ $# -ge 2 ]] || { echo "--release-info needs a path" >&2; exit 2; }
      RELEASE_INFO="$2"
      shift
      ;;
    -h|--help)
      sed -n '2,36p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "Unknown argument: $argument" >&2
      exit 2
      ;;
  esac
  shift
done

SHORT_SHA="$(git -C "$ROOT" rev-parse --short HEAD)"

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

if [[ "$UPLOAD_ONLY" -eq 1 ]]; then
  if [[ -z "$RELEASE_INFO" ]]; then
    echo "--upload-only requires --release-info <release-info.json>" >&2
    exit 2
  fi
  if [[ ! -f "$RELEASE_INFO" ]]; then
    echo "Release metadata not found: $RELEASE_INFO" >&2
    exit 1
  fi
  UPLOAD_APK="$(cd "$(dirname "$RELEASE_INFO")" && pwd)/$(python3 "$RELEASE_INFO_PY" read "$RELEASE_INFO" apk)"
  if [[ ! -f "$UPLOAD_APK" ]]; then
    echo "APK listed in $RELEASE_INFO not found: $UPLOAD_APK" >&2
    exit 1
  fi
  # The artifact crossed a job boundary: refuse to upload bytes that are not
  # the ones the building job tested and signed.
  EXPECTED_SHA256="$(python3 "$RELEASE_INFO_PY" read "$RELEASE_INFO" sha256)"
  ACTUAL_SHA256="$(sha256_of "$UPLOAD_APK")"
  if [[ "$EXPECTED_SHA256" != "$ACTUAL_SHA256" ]]; then
    echo "APK SHA-256 mismatch: artifact is $ACTUAL_SHA256, metadata says $EXPECTED_SHA256" >&2
    exit 1
  fi
  APP_ID="$(python3 "$RELEASE_INFO_PY" read "$RELEASE_INFO" firebase_app_id)"
  if [[ -z "$APP_ID" ]]; then
    echo "Could not read firebase_app_id from $RELEASE_INFO" >&2
    exit 1
  fi
  echo "Uploading the prebuilt APK $UPLOAD_APK (sha256 $ACTUAL_SHA256)"
else
  # Release signing material. Locally `mise run admin:android-distribute` passes
  # it from OpenBao through fnox (temp files, removed on exit). The keystore is
  # stored base64-encoded, so SIGNING_KEYSTORE_B64_FILE is decoded into a
  # private temp file here. Fallback: files in the gitignored secrets/android/
  # (`mise run secrets:file`). Values are never printed.
  if [[ -z "${SIGNING_KEYSTORE:-}" && -n "${SIGNING_KEYSTORE_B64_FILE:-}" ]]; then
    decoded_keystore="$(mktemp "${TMPDIR:-/tmp}/release-keystore.XXXXXX")"
    trap 'rm -f "$decoded_keystore"' EXIT
    base64 --decode < "$SIGNING_KEYSTORE_B64_FILE" > "$decoded_keystore"
    SIGNING_KEYSTORE="$decoded_keystore"
  fi
  SIGNING_KEYSTORE="${SIGNING_KEYSTORE:-$SECRETS_ANDROID/home-release.keystore}"
  SIGNING_STORE_PASSWORD_FILE="${SIGNING_STORE_PASSWORD_FILE:-$SECRETS_ANDROID/home-release.store.password}"
  SIGNING_KEY_PASSWORD_FILE="${SIGNING_KEY_PASSWORD_FILE:-$SECRETS_ANDROID/home-release.key.password}"
  SIGNING_KEY_ALIAS_FILE="${SIGNING_KEY_ALIAS_FILE:-$SECRETS_ANDROID/home-release.alias}"
  SIGNING_CERT_SHA256_FILE="${SIGNING_CERT_SHA256_FILE:-$SECRETS_ANDROID/home-release.cert.sha256}"
  if [[ -z "${SIGNING_KEY_ALIAS:-}" && -f "$SIGNING_KEY_ALIAS_FILE" ]]; then
    SIGNING_KEY_ALIAS="$(tr -d '[:space:]' < "$SIGNING_KEY_ALIAS_FILE")"
  fi
  if [[ -z "${SIGNING_CERT_SHA256:-}" && -f "$SIGNING_CERT_SHA256_FILE" ]]; then
    SIGNING_CERT_SHA256="$(tr -d '[:space:]' < "$SIGNING_CERT_SHA256_FILE")"
  fi
  export SIGNING_KEYSTORE SIGNING_STORE_PASSWORD_FILE SIGNING_KEY_PASSWORD_FILE
  export SIGNING_KEY_ALIAS="${SIGNING_KEY_ALIAS:-}"
  export SIGNING_CERT_SHA256="${SIGNING_CERT_SHA256:-}"

  SIGNING_READY=1
  for required in "$SIGNING_KEYSTORE" "$SIGNING_STORE_PASSWORD_FILE" "$SIGNING_KEY_PASSWORD_FILE"; do    [[ -f "$required" ]] || SIGNING_READY=0
  done
  [[ -n "$SIGNING_CERT_SHA256" ]] || SIGNING_READY=0
  if [[ "$SIGNING_READY" -eq 0 && "$DRY_RUN" -eq 0 ]]; then
    echo "Missing release signing material." >&2
    echo "Expected keystore $SIGNING_KEYSTORE, password files next to it, and a signer SHA-256" >&2
    echo "($SIGNING_CERT_SHA256_FILE or SIGNING_CERT_SHA256)." >&2
    echo "Run it as 'mise run admin:android-distribute' (signing material from OpenBao);" >&2
    echo "see projects/mobile/android/README.md." >&2
    exit 1
  fi

  if [[ ! -e "$GOOGLE_SERVICES_LINK" ]]; then
    echo "Missing $GOOGLE_SERVICES_LINK" >&2
    echo "Link secrets/android/google-services.json to it; see projects/mobile/android/README.md." >&2
    exit 1
  fi

  export ANDROID_HOME="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}"
  BUILD_TOOLS="${ANDROID_BUILD_TOOLS:-$ANDROID_HOME/build-tools/36.0.0}"
  for tool in apksigner zipalign; do
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
  VERSION_NAME="0.1.0-$SHORT_SHA"

  echo "Building //projects/mobile/android:app (versionCode $VERSION_CODE, versionName $VERSION_NAME)"
  (
    cd "$ROOT"
    # `--config=android` first: it activates platform/android/android.bazelrc, and
    # the `--define` values below must win over the defaults defined there.
    "$BAZEL" build \
      --config=android \
      --define "home_version_code=$VERSION_CODE" \
      --define "home_version_name=$VERSION_NAME" \
      //projects/mobile/android:app
  )

  if [[ ! -f "$APK" ]]; then
    echo "APK not found at $APK" >&2
    exit 1
  fi

  if [[ "$SIGNING_READY" -eq 1 ]]; then
    echo "Signing the APK with the release key"
    "$SIGN_SCRIPT" "$APK" "$RELEASE_APK"
    UPLOAD_APK="$RELEASE_APK"
  else
    echo "Dry run: release signing material is absent; would run"
    echo "  platform/android/sign-release.sh $APK $RELEASE_APK"
    UPLOAD_APK="$RELEASE_APK"
  fi

  APP_ID="$(python3 "$RELEASE_INFO_PY" app-id "$GOOGLE_SERVICES_LINK" "$APP_ID_PACKAGE")"
  if [[ -z "$APP_ID" ]]; then
    echo "Could not read mobilesdk_app_id for $APP_ID_PACKAGE" >&2
    exit 1
  fi
fi

if [[ "$CI_MODE" -eq 1 ]]; then
  # Release notes for a GitHub Actions run. On a PR the job exports PR_NUMBER
  # and PR_TITLE (the title is untrusted text, so it arrives through the
  # environment, never as shell source). A push to develop is a squash merge:
  # its commit subject is the PR title.
  if [[ -n "${PR_NUMBER:-}" ]]; then
    SUBJECT="PR #${PR_NUMBER}: ${PR_TITLE:-$(git -C "$ROOT" log -1 --pretty=%s)}"
  else
    SUBJECT="$(git -C "$ROOT" log -1 --pretty=%s)"
  fi
  RUN_URL="${GITHUB_RUN_URL:-}"
  if [[ -z "$RUN_URL" && -n "${GITHUB_SERVER_URL:-}" && -n "${GITHUB_REPOSITORY:-}" \
        && -n "${GITHUB_RUN_ID:-}" ]]; then
    RUN_URL="$GITHUB_SERVER_URL/$GITHUB_REPOSITORY/actions/runs/$GITHUB_RUN_ID"
  fi
  RELEASE_NOTES="$SUBJECT"$'\n'"$SHORT_SHA"
  if [[ -n "$RUN_URL" ]]; then
    RELEASE_NOTES="$RELEASE_NOTES"$'\n'"$RUN_URL"
  fi
else
  RELEASE_NOTES="$(git -C "$ROOT" describe --tags --always --dirty)"
  RELEASE_NOTES="$RELEASE_NOTES"$'\n'"$(git -C "$ROOT" log --oneline -5)"
fi

if [[ "$UPLOAD_ONLY" -eq 0 && "$SIGNING_READY" -eq 1 ]]; then
  echo "Release APK signed and verified: $RELEASE_APK"
fi
echo "Firebase app: $APP_ID, group: $TESTER_GROUP"
echo "Release notes:"
printf '%s\n' "$RELEASE_NOTES" | sed 's/^/  /'

FIREBASE_ARGS=(
  "--yes" "firebase-tools@$FIREBASE_TOOLS_VERSION"
  appdistribution:distribute "$UPLOAD_APK"
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
