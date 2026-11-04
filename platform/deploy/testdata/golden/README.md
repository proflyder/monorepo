# Golden render fixtures

`<unit>/<file>` is the output the **Python** renderer produced for
`projects/home/kubernetes/.../<file>` with the committed TEST-NET env in
`../env/` and the `--set` values listed below. The Go renderer must reproduce
these files **byte for byte** (`go test ./platform/deploy/internal/render`).

The fixtures were captured before the Python renderers were deleted
(`projects/home/kubernetes/scheduler/render.py`, its `mcp-gateway/` and
`wakegrant/` wrappers, and the heredoc in `ingress/deploy.sh`). To regenerate
after an intentional template change, check those files out of the commit that
precedes the cutover (`git show 89c42de:<path>`), render, and commit the output
again.

Values are synthetic: TEST-NET `192.0.2.0/24` addresses, zero-filled image
digests, `*.example.test` hostnames. Never capture a real private env.

## Invocations

`nft` is rendered with the scheduler allowlist (`--unit scheduler`); the two
`.nft` files are host nftables templates, not Kubernetes manifests.

| Unit | Env | `--set` |
|---|---|---|
| scheduler | `env/scheduler.env.example` | `BUILD_ID=0123456789ab`, `SCHEDULER_IMAGE`, `HA_BRIDGE_IMAGE`, `HA_CONTROL_IMAGE`, `PREFLIGHT_IMAGE` |
| currency-bot | `env/currency-bot.env.example` | `BUILD_ID=0123456789ab`, `CURRENCY_BOT_IMAGE`, `CLOUDFLARED_IMAGE`, `CURRENCY_WEBHOOK_URL=https://bot.example.test/telegram/webhook`, `CURRENCY_BOT_NODE_PORT=30100` |
| mcp-gateway | `env/mcp-gateway.env.example` | `BUILD_ID=0123456789ab`, `MCP_GATEWAY_IMAGE`, `BB_CT_IP=192.0.2.30`, `BB_PROJECT_STATION_LAB`, `BB_PROJECT_ENGLISH_NOTES`, `BB_DISPATCHER_THREAD_ID` |
| wakegrant | `env/wakegrant.env.example` | `BUILD_ID=0123456789ab`, `WAKE_GRANT_IMAGE`, `WAKE_GRANT_NODE_PORT=30101` |
| nft | `env/scheduler.env.example` | same as scheduler |
| ingress | (none; fixed key set) | `HOME_DOMAIN=home.example.test`, `LAN_CIDR=192.0.2.0/24`, `HAOS_ADDRESS=192.0.2.28` and the five derived `*_HOST` values |

Image references are `ghcr.io/proflyder/<name>@sha256:<64 × digit>`, where the
digit increases per image (`0` for scheduler, `1` for ha-bridge, and so on).
