#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
DOMAINS_DIR=$SCRIPT_DIR/domains
CA_CERT=$SCRIPT_DIR/certs/ca.crt
CA_KEY=$SCRIPT_DIR/certs/ca.key
OUTPUT_DIR=$SCRIPT_DIR/generated
CERT_OUTPUT_DIR=$OUTPUT_DIR/certs
CONFIG_OUTPUT_DIR=$OUTPUT_DIR/configs
NGINX_ROOT=${NGINX_ROOT:-/app}

# secp384r1 is an elliptic-curve parameter supported by OpenSSL.
EC_CURVE=secp384r1

if [ ! -d "$DOMAINS_DIR" ]; then
    printf '%s\n' "domains directory is missing" >&2
    exit 1
fi
if [ ! -r "$CA_CERT" ] || [ ! -r "$CA_KEY" ]; then
    printf '%s\n' "certs/ca.crt and certs/ca.key must be present and readable" >&2
    exit 1
fi

# Generated files are rebuilt so removed domains do not leave stale Nginx servers.
rm -rf "$CERT_OUTPUT_DIR" "$CONFIG_OUTPUT_DIR"
mkdir -p "$CERT_OUTPUT_DIR" "$CONFIG_OUTPUT_DIR"

for domains_file in "$DOMAINS_DIR"/*.txt; do
    [ -f "$domains_file" ] || continue

    list_name=${domains_file##*/}
    list_name=${list_name%.txt}
    case "$list_name" in
        ''|*[!A-Za-z0-9._-]*)
            printf 'Skipping invalid domain list filename: %s\n' "$domains_file" >&2
            continue
            ;;
    esac

    list_cert_dir=$CERT_OUTPUT_DIR/$list_name
    list_config_file=$CONFIG_OUTPUT_DIR/$list_name.conf
    mkdir -p "$list_cert_dir"
    : >"$list_config_file"

    # An environment override takes precedence over the first line.
    env_name=$(printf '%s' "$list_name" | tr '[:lower:]-.' '[:upper:]__')_UPSTREAM_URL
    eval "override_url=\${$env_name-}"

    exec 3< "$domains_file"
    IFS= read -r upstream <&3 || upstream=''
    upstream=${upstream%$(printf '\r')}

    if [ -n "$override_url" ]; then
        case "$override_url" in
            http://*|https://*) ;;
            *)
                printf 'Skipping %s: %s must start with http:// or https://\n' "$domains_file" "$env_name" >&2
                exec 3<&-
                continue
                ;;
        esac
        upstream_url=${override_url%/}
    else
        case "$upstream" in
            ''|'#'*|*[!A-Za-z0-9._:-]*|:*|*:)
                printf 'Skipping %s: first line must be an upstream hostname or hostname:port\n' "$domains_file" >&2
                exec 3<&-
                continue
                ;;
        esac
        upstream_url=http://$upstream
    fi

    while IFS= read -r domain <&3 || [ -n "$domain" ]; do
        # Strip a possible CR from files edited on Windows.
        domain=${domain%$(printf '\r')}
        case "$domain" in
            ''|'#'*) continue ;;
        esac

        proxy_as_is=false
        case "$domain" in
            !*)
                proxy_as_is=true
                domain=${domain#?}
                ;;
        esac

        # Keep output names, certificate subjects, and Nginx values safe.
        case "$domain" in
            ''|*[!A-Za-z0-9._*-]*)
                printf 'Skipping invalid domain name: %s\n' "$domain" >&2
                continue
                ;;
        esac

        key_domain=$domain
        key_domain=$(printf '%s' "$key_domain" | tr '*' '_')
        key_file=$list_cert_dir/$key_domain.key
        csr_file=$list_cert_dir/$key_domain.csr
        cert_file=$list_cert_dir/$key_domain.crt
        chain_file=$list_cert_dir/$key_domain.chain.crt

        extra_location=''
        if [ "$proxy_as_is" = true ]; then
            case "$domain" in
                *.s3.yandex.net|*.svc.cdn.yandex.net)
                    proxy_pass_line='return 403;'
                    ;;
                *)
                    proxy_pass_line='proxy_pass https://$host$request_uri;'
                    ;;
            esac
            proxy_ssl_name_line='proxy_ssl_name $host;'
            proxy_host_line='proxy_set_header Host $host;'
        else
            case "$domain" in
                clck.yandex.net|report.appmetrica.yandex.net|startup.mobile.yandex.net)
                    proxy_pass_line='return 204;'
                    ;;
                quasar.yandex.net|quasar.yandex.ru)
                    proxy_pass_line="proxy_pass $upstream_url/$domain\$request_uri;"
                    extra_location='    location /quasar_metrics/ {
        return 204;
    }
'
                    ;;
                *)
                    proxy_pass_line="proxy_pass $upstream_url/$domain\$request_uri;"
                    ;;
            esac
            proxy_ssl_name_line='proxy_ssl_name $proxy_host;'
            proxy_host_line='proxy_set_header Host $proxy_host;'
        fi

        ext_file=$(mktemp "${TMPDIR:-/tmp}/generate-cert.XXXXXX")
        trap 'rm -f "$ext_file"' 0 HUP INT TERM

        cat >"$ext_file" <<EOF
basicConstraints=critical,CA:FALSE
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth,clientAuth
subjectAltName=DNS:$domain
EOF

        openssl ecparam -name "$EC_CURVE" -genkey -noout -out "$key_file"
        openssl req -new -sha256 -key "$key_file" -out "$csr_file" \
            -subj "/C=RU/ST=Saint-Petersburg/L=Saint-Petersburg/O=v3rt3p/OU=https-proxy/CN=$domain"
        openssl x509 -req -sha256 -days 825 \
            -in "$csr_file" -CA "$CA_CERT" -CAkey "$CA_KEY" \
            -set_serial "0x$(openssl rand -hex 16)" \
            -extfile "$ext_file" -out "$cert_file"

        cat "$cert_file" "$CA_CERT" >"$chain_file"
        rm -f "$csr_file" "$cert_file"

        cat >>"$list_config_file" <<EOF
server {
    listen 80;
    server_name $domain;

    $extra_location
    location / {
        $proxy_pass_line
        proxy_http_version 1.1;
        proxy_ssl_server_name on;
        $proxy_ssl_name_line
        proxy_ssl_verify off;
        $proxy_host_line
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto http;
        proxy_set_header Upgrade \$http_upgrade;
        proxy_set_header Connection \$connection_upgrade;
    }
}

server {
    listen 443 ssl;
    server_name $domain;

    ssl_certificate $NGINX_ROOT/generated/certs/$list_name/$key_domain.chain.crt;
    ssl_certificate_key $NGINX_ROOT/generated/certs/$list_name/$key_domain.key;

    $extra_location
    location / {
        $proxy_pass_line
        proxy_http_version 1.1;
        proxy_ssl_server_name on;
        $proxy_ssl_name_line
        proxy_ssl_verify off;
        $proxy_host_line
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto https;
        proxy_set_header Upgrade \$http_upgrade;
        proxy_set_header Connection \$connection_upgrade;
    }
}

EOF

        rm -f "$ext_file"
        trap - 0 HUP INT TERM
    done
    exec 3<&-
done

printf '%s\n' "Generated certificates in $CERT_OUTPUT_DIR"
printf '%s\n' "Generated Nginx configs in $CONFIG_OUTPUT_DIR"
