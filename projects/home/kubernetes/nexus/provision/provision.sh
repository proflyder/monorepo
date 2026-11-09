#!/bin/sh
# Provision the Nexus repositories, the read-only CI user and the security
# settings through the Nexus REST API.
#
# Runs inside the `nexus-provision` Job. Flux recreates the Job whenever this
# script changes (the ConfigMap content hash changes the mounted name); every
# step is idempotent: an existing object is updated with PUT instead of being
# created again, so a second run changes nothing.
#
# Inputs (environment, supplied by the Job from the `nexus-secrets` Secret):
#   NEXUS_URL             in-cluster base URL, for example http://nexus.nexus.svc:8081
#   NEXUS_ADMIN_PASSWORD  current `admin` password
#   NEXUS_CI_PASSWORD     desired password of the `ci-reader` user
#
# Secrets are never printed: curl runs without `-v`/`-x`, and errors carry only
# HTTP status codes and object names.
set -eu

BASE="${NEXUS_URL:?NEXUS_URL is required}"
ADMIN_USER="admin"
: "${NEXUS_ADMIN_PASSWORD:?NEXUS_ADMIN_PASSWORD is required}"
: "${NEXUS_CI_PASSWORD:?NEXUS_CI_PASSWORD is required}"

BLOB_STORE=ci-cache

# `curl` wrapper: prints the HTTP status code, writes the body to /tmp/body.
# Authentication is passed through a config file so the password never lands
# in argv (and therefore never in a process listing or a crash dump).
umask 077
AUTH_CONFIG="$(mktemp)"
# curl config quoting: a backslash and a double quote must be escaped.
credential="$(printf '%s:%s' "$ADMIN_USER" "$NEXUS_ADMIN_PASSWORD" |
  sed -e 's/\\/\\\\/g' -e 's/"/\\"/g')"
printf 'user = "%s"\n' "$credential" > "$AUTH_CONFIG"
unset credential
trap 'rm -f "$AUTH_CONFIG"' EXIT INT TERM

request() {
  method="$1"
  path="$2"
  shift 2
  curl -sS --config "$AUTH_CONFIG" -X "$method" \
    -o /tmp/body -w '%{http_code}' \
    "$BASE$path" "$@"
}

expect() {
  code="$1"
  what="$2"
  case "$code" in
    200|201|204) ;;
    *)
      echo "provisioning failed for $what: HTTP $code" >&2
      # The body is a Nexus error message, not a secret; trim it for the log.
      head -c 400 /tmp/body >&2 || true
      echo >&2
      exit 1
      ;;
  esac
}

wait_ready() {
  echo "waiting for Nexus at $BASE"
  i=0
  while [ "$i" -lt 120 ]; do
    code="$(curl -sS -o /dev/null -w '%{http_code}' "$BASE/service/rest/v1/status" || true)"
    if [ "$code" = "200" ]; then
      echo "Nexus is writable"
      return 0
    fi
    i=$((i + 1))
    sleep 5
  done
  echo "Nexus did not become writable in time" >&2
  return 1
}

ensure_blob_store() {
  code="$(request GET "/service/rest/v1/blobstores/file/$BLOB_STORE")"
  if [ "$code" = "200" ]; then
    echo "blob store $BLOB_STORE exists"
    return 0
  fi
  code="$(request POST /service/rest/v1/blobstores/file \
    -H 'Content-Type: application/json' \
    -d "{\"name\":\"$BLOB_STORE\",\"path\":\"/nexus-data/blob/$BLOB_STORE\"}")"
  expect "$code" "blob store $BLOB_STORE"
  echo "created blob store $BLOB_STORE"
}

# A fresh Nexus ships sample repositories (maven-central, maven-public, ...) on
# the `default` blob store, and the API refuses to move a repository between
# blob stores. These are pure caches, so a repository on another store is
# dropped and recreated on ours.
drop_if_foreign_store() {
  format="$1"
  type="$2"
  name="$3"
  code="$(request GET "/service/rest/v1/repositories/$format/$type/$name")"
  [ "$code" = "200" ] || return 0
  if grep -q "\"blobStoreName\" *: *\"$BLOB_STORE\"" /tmp/body; then
    return 0
  fi
  code="$(request DELETE "/service/rest/v1/repositories/$name")"
  expect "$code" "removal of $name from a foreign blob store"
  echo "removed $name (was on another blob store)"
}

# ensure_repo FORMAT TYPE NAME PAYLOAD
ensure_repo() {
  format="$1"
  type="$2"
  name="$3"
  payload="$4"
  drop_if_foreign_store "$format" "$type" "$name"
  code="$(request GET "/service/rest/v1/repositories/$name")"
  if [ "$code" = "200" ]; then
    code="$(request PUT "/service/rest/v1/repositories/$format/$type/$name" \
      -H 'Content-Type: application/json' -d "$payload")"
    expect "$code" "repository $name"
    echo "updated $name ($format/$type)"
  else
    code="$(request POST "/service/rest/v1/repositories/$format/$type" \
      -H 'Content-Type: application/json' -d "$payload")"
    expect "$code" "repository $name"
    echo "created $name ($format/$type)"
  fi
}

# Optional repository: some formats are absent or differ between Nexus
# releases. A failure is reported and does not abort provisioning.
optional_repo() {
  format="$1"
  type="$2"
  name="$3"
  payload="$4"
  drop_if_foreign_store "$format" "$type" "$name"
  code="$(request GET "/service/rest/v1/repositories/$name")"
  if [ "$code" = "200" ]; then
    code="$(request PUT "/service/rest/v1/repositories/$format/$type/$name" \
      -H 'Content-Type: application/json' -d "$payload")"
  else
    code="$(request POST "/service/rest/v1/repositories/$format/$type" \
      -H 'Content-Type: application/json' -d "$payload")"
  fi
  case "$code" in
    200|201|204) echo "configured optional repository $name ($format/$type)" ;;
    *)
      echo "warning: optional repository $name ($format/$type) not configured: HTTP $code" >&2
      ;;
  esac
}

common_storage='{"blobStoreName":"ci-cache","strictContentTypeValidation":true,"writePolicy":"ALLOW"}'
# Upstreams (GitHub release downloads above all) return transient 5xx. With
# auto-blocking one such answer took the whole repository offline and Nexus
# answered 404/503 for every path; a long negative cache then kept those 404s
# for a day. Never auto-block, retry upstream calls, keep misses briefly.
common_negative='{"enabled":true,"timeToLive":5}'
common_http='{"blocked":false,"autoBlock":false,"connection":{"retries":3,"timeout":60}}'

wait_ready
ensure_blob_store
# The shipped maven-public group references maven-central; drop the group
# first so its members can be recreated on our blob store.
drop_if_foreign_store maven group maven-public

maven_central="$(cat <<JSON
{
  "name": "maven-central",
  "online": true,
  "storage": $common_storage,
  "proxy": {"remoteUrl": "https://repo1.maven.org/maven2/", "contentMaxAge": 1440, "metadataMaxAge": 1440},
  "negativeCache": $common_negative,
  "httpClient": $common_http,
  "maven": {"versionPolicy": "RELEASE", "layoutPolicy": "PERMISSIVE", "contentDisposition": "ATTACHMENT"}
}
JSON
)"
ensure_repo maven proxy maven-central "$maven_central"

google_maven="$(cat <<JSON
{
  "name": "google-maven",
  "online": true,
  "storage": $common_storage,
  "proxy": {"remoteUrl": "https://maven.google.com/", "contentMaxAge": 1440, "metadataMaxAge": 1440},
  "negativeCache": $common_negative,
  "httpClient": $common_http,
  "maven": {"versionPolicy": "RELEASE", "layoutPolicy": "PERMISSIVE", "contentDisposition": "ATTACHMENT"}
}
JSON
)"
ensure_repo maven proxy google-maven "$google_maven"

maven_public="$(cat <<JSON
{
  "name": "maven-public",
  "online": true,
  "storage": $common_storage,
  "group": {"memberNames": ["maven-central", "google-maven"]}
}
JSON
)"
ensure_repo maven group maven-public "$maven_public"

github_raw="$(cat <<JSON
{
  "name": "github",
  "online": true,
  "storage": $common_storage,
  "proxy": {"remoteUrl": "https://github.com/", "contentMaxAge": 1440, "metadataMaxAge": 1440},
  "negativeCache": $common_negative,
  "httpClient": $common_http,
  "raw": {"contentDisposition": "ATTACHMENT"}
}
JSON
)"
ensure_repo raw proxy github "$github_raw"

codeload_raw="$(cat <<JSON
{
  "name": "github-codeload",
  "online": true,
  "storage": $common_storage,
  "proxy": {"remoteUrl": "https://codeload.github.com/", "contentMaxAge": 1440, "metadataMaxAge": 1440},
  "negativeCache": $common_negative,
  "httpClient": $common_http,
  "raw": {"contentDisposition": "ATTACHMENT"}
}
JSON
)"
ensure_repo raw proxy github-codeload "$codeload_raw"

nodejs_raw="$(cat <<JSON
{
  "name": "nodejs-dist",
  "online": true,
  "storage": $common_storage,
  "proxy": {"remoteUrl": "https://nodejs.org/dist/", "contentMaxAge": 1440, "metadataMaxAge": 1440},
  "negativeCache": $common_negative,
  "httpClient": $common_http,
  "raw": {"contentDisposition": "ATTACHMENT"}
}
JSON
)"
ensure_repo raw proxy nodejs-dist "$nodejs_raw"

npm_proxy="$(cat <<JSON
{
  "name": "npmjs",
  "online": true,
  "storage": $common_storage,
  "proxy": {"remoteUrl": "https://registry.npmjs.org/", "contentMaxAge": 1440, "metadataMaxAge": 1440},
  "negativeCache": $common_negative,
  "httpClient": $common_http,
  "npm": {"metadataMaxAge": 1440}
}
JSON
)"
ensure_repo npm proxy npmjs "$npm_proxy"

pypi_proxy="$(cat <<JSON
{
  "name": "pypi",
  "online": true,
  "storage": $common_storage,
  "proxy": {"remoteUrl": "https://pypi.org/", "contentMaxAge": 1440, "metadataMaxAge": 1440},
  "negativeCache": $common_negative,
  "httpClient": $common_http
}
JSON
)"
ensure_repo pypi proxy pypi "$pypi_proxy"

# Cargo support varies by Nexus edition and release. Try it, but never fail the
# whole provisioning run when this install does not expose the format.
cargo_proxy="$(cat <<JSON
{
  "name": "crates",
  "online": true,
  "storage": $common_storage,
  "proxy": {"remoteUrl": "https://index.crates.io/", "contentMaxAge": 1440, "metadataMaxAge": 1440},
  "negativeCache": $common_negative,
  "httpClient": $common_http,
  "cargo": {"requireAuthentication": false}
}
JSON
)"
optional_repo cargo proxy crates "$cargo_proxy"

# Anonymous access is off: every repository request must authenticate.
code="$(request PUT /service/rest/v1/security/anonymous \
  -H 'Content-Type: application/json' \
  -d '{"enabled":false,"userId":"anonymous","realmName":"NexusAuthorizingRealm"}')"
expect "$code" "anonymous access setting"
echo "disabled anonymous access"

reader_role="$(cat <<JSON
{
  "id": "ci-reader",
  "name": "ci-reader",
  "description": "Read-only repository access for CI caches",
  "privileges": ["nx-repository-view-*-*-read", "nx-repository-view-*-*-browse"],
  "roles": []
}
JSON
)"
code="$(request GET /service/rest/v1/security/roles/ci-reader)"
if [ "$code" = "200" ]; then
  code="$(request PUT /service/rest/v1/security/roles/ci-reader \
    -H 'Content-Type: application/json' -d "$reader_role")"
else
  code="$(request POST /service/rest/v1/security/roles \
    -H 'Content-Type: application/json' -d "$reader_role")"
fi
expect "$code" "role ci-reader"
echo "configured role ci-reader"

# The user record carries no password; the password is set separately and
# streamed on stdin, so it never appears in argv or in the job log.
reader_user='{"userId":"ci-reader","firstName":"CI","lastName":"Reader","emailAddress":"ci-reader@home.invalid","source":"default","status":"active","roles":["ci-reader"]}'
request GET '/service/rest/v1/security/users?userId=ci-reader' >/dev/null
if grep -q '"userId" *: *"ci-reader"' /tmp/body; then
  code="$(request PUT /service/rest/v1/security/users/ci-reader \
    -H 'Content-Type: application/json' -d "$reader_user")"
  expect "$code" "user ci-reader"
else
  # Creation requires a password: send the record with it on stdin.
  code="$(printf '%s' "$reader_user" | sed 's/}$//' | {
    cat
    printf ',"password":"%s"}' "$(printf '%s' "$NEXUS_CI_PASSWORD" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g')"
  } | request POST /service/rest/v1/security/users \
    -H 'Content-Type: application/json' --data-binary @-)"
  expect "$code" "user ci-reader"
fi
code="$(printf '%s' "$NEXUS_CI_PASSWORD" | request PUT \
  /service/rest/v1/security/users/ci-reader/change-password \
  -H 'Content-Type: text/plain' --data-binary @-)"
expect "$code" "ci-reader password"
echo "configured user ci-reader"

echo "Nexus provisioning complete"
