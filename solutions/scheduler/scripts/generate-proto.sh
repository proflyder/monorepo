#!/usr/bin/env bash
# Regenerate checked-in Go protobuf/Connect stubs.
#
# Requirements (not committed; install into an ignored tools directory):
#   protoc 33.x
#   protoc-gen-go v1.36.x      (google.golang.org/protobuf/cmd/protoc-gen-go)
#   protoc-gen-connect-go v1.21.x (connectrpc.com/connect/cmd/protoc-gen-connect-go)
#
# Example:
#   PATH=secrets/local-tools/protoc/bin:secrets/local-tools/bin:$PATH \
#     solutions/scheduler/scripts/generate-proto.sh
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$root"

protoc="${PROTOC:-protoc}"
"$protoc" \
  -I packages/proto \
  --go_out=. --go_opt=module=ricardo.local/monorepo \
  --connect-go_out=. --connect-go_opt=module=ricardo.local/monorepo \
  packages/proto/ricardo/v1/alarms.proto \
  packages/proto/ricardo/v1/scheduler.proto

gofmt -w packages/proto/ricardo/v1/*.pb.go packages/proto/ricardo/v1/ricardov1connect/*.go
