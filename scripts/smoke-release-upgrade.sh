#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

hardware=false
if [[ ${1:-} == --hardware ]]; then
    hardware=true
    shift
fi
if (( $# > 1 )) || [[ ${1:-} == --help ]]; then
    echo "Usage: $0 [--hardware] [previous-release-tag]" >&2
    if [[ ${1:-} == --help ]]; then exit 0; fi
    exit 2
fi

previous_tag=${1:-$(git tag --merged HEAD --no-contains HEAD --sort=-version:refname --list 'v*' |
    awk '/^v[0-9]+\.[0-9]+\.[0-9]+$/ && !found++')}
if [[ ! "$previous_tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "A previous stable release tag is required. Fetch the release tags and history or pass a tag explicitly." >&2
    exit 2
fi

if "$hardware"; then
    if [[ ! -f frontend/.env.local || ! -x frontend/node_modules/.bin/playwright ]]; then
        echo "Hardware mode requires frontend/.env.local and the frontend Playwright dependencies." >&2
        exit 2
    fi
    if [[ ! -r ${SMOKE_TLS_CERT:-} || ! -r ${SMOKE_TLS_KEY:-} ]]; then
        echo "Set SMOKE_TLS_CERT and SMOKE_TLS_KEY to the readable PEM files trusted by the device." >&2
        exit 2
    fi
    settings=$(node --env-file=frontend/.env.local - <<'NODE'
const { TEST_DOMAIN, TEST_PORT, TEST_WALLBOX_DOMAIN, TEST_WALLBOX_UID } = process.env;
function relaySettings() {
    const device = new URL(TEST_WALLBOX_DOMAIN);
    const relay = new URL(TEST_DOMAIN);
    const port = TEST_PORT?.replace(/^:/, '') || relay.port || '443';
    if (!['http:', 'https:'].includes(device.protocol) || !TEST_WALLBOX_UID?.trim() ||
        relay.protocol !== 'https:' || relay.hostname.startsWith('[') ||
        !/^\d+$/.test(port) || Number(port) < 1 || Number(port) > 65535) {
        throw new Error();
    }
    return [relay.hostname, String(Number(port))].join('\n');
}
try {
    console.log(relaySettings());
} catch {
    console.error('Hardware mode requires TEST_WALLBOX_DOMAIN, TEST_WALLBOX_UID, and an HTTPS TEST_DOMAIN with a valid relay port.');
    process.exitCode = 2;
}
NODE
)
    mapfile -t relay_settings <<< "$settings"
    export SMOKE_RELAY_HOST="${relay_settings[0]}" SMOKE_TCP_PORT="${relay_settings[1]}"
    export SMOKE_BIND_IP=0.0.0.0 SMOKE_UDP_PORT=51820
    export SMOKE_ACCOUNT_PASSWORD
    SMOKE_ACCOUNT_PASSWORD=$(openssl rand -hex 18)
fi

previous_image="tinkerforge/esp32-remote-access:$previous_tag-warp"
test_dir=$(mktemp -d)
project="remote-access-smoke-${test_dir##*.}"
project=${project,,}
current_image=${CURRENT_SMOKE_IMAGE:-"esp32-remote-access:$project"}
export SMOKE_CERT_DIR="$test_dir/certs"
export SMOKE_RELAY_IMAGE="$previous_image"
export SMOKE_JWT_SECRET
SMOKE_JWT_SECRET=$(openssl rand -hex 32)
compose=(docker compose --env-file /dev/null -p "$project" -f docker/docker-compose-smoke.yml)

hardware_fixture() (
    cd frontend
    TEST_DOMAIN="$url" SMOKE_STATE_DIR="$test_dir" ./node_modules/.bin/playwright test \
        --config smoke/playwright.config.ts --output "$test_dir/playwright" --grep "hardware smoke: $1\$"
)

cleanup() {
    local result=$? keep_state=false
    if [[ -f "$test_dir/device-backup.json" ]] && ! hardware_fixture cleanup; then
        echo "Device cleanup failed. Saved relay settings are in $test_dir/device-backup.json." >&2
        keep_state=true
        result=1
    fi
    if (( result != 0 )); then
        "${compose[@]}" logs --no-color >&2 || true
    fi
    "${compose[@]}" down --volumes || true
    if [[ -z ${CURRENT_SMOKE_IMAGE:-} ]]; then
        docker image rm "$current_image" >/dev/null 2>&1 || true
    fi
    if "$keep_state"; then
        rm -r "$SMOKE_CERT_DIR"
    else
        rm -r "$test_dir"
    fi
    exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

mkdir "$SMOKE_CERT_DIR"
certificate="$SMOKE_CERT_DIR/cert.pem"
curl_tls=(--cacert "$certificate")
if "$hardware"; then
    cp -- "$SMOKE_TLS_CERT" "$certificate"
    cp -- "$SMOKE_TLS_KEY" "$SMOKE_CERT_DIR/key.pem"
    # Browser and curl use loopback. The physical device validates the LAN certificate.
    curl_tls=(--insecure)
else
    openssl req -x509 -newkey rsa:2048 -noenc -days 1 -subj /CN=127.0.0.1 \
        -addext subjectAltName=IP:127.0.0.1 -addext basicConstraints=critical,CA:FALSE \
        -keyout "$SMOKE_CERT_DIR/key.pem" -out "$certificate" 2>"$test_dir/openssl.log" ||
        { cat "$test_dir/openssl.log" >&2; exit 1; }
fi
# The container user needs to read the key. The parent temp directory is private.
chmod 644 "$SMOKE_CERT_DIR/key.pem"

if [[ -z ${CURRENT_SMOKE_IMAGE:-} ]]; then
    docker build -f docker/production/Dockerfile --build-arg BRAND=warp -t "$current_image" .
fi
docker pull "$previous_image"
if ! "$hardware"; then
    cargo build --locked --manifest-path tun/Cargo.toml --example release_upgrade
fi
"${compose[@]}" up -d --wait --wait-timeout 60 postgres

start_relay() {
    export SMOKE_RELAY_IMAGE="$1"
    "${compose[@]}" up -d --no-deps --force-recreate relay
    local https_address
    https_address=$("${compose[@]}" port relay 443)
    url="https://127.0.0.1:${https_address##*:}"
    udp_address=$("${compose[@]}" port --protocol udp relay 51820)
    udp_port=${udp_address##*:}
    for ((attempt = 0; attempt < 60; attempt++)); do
        if curl --noproxy '*' "${curl_tls[@]}" --fail --silent --max-time 1 "$url/" >/dev/null; then
            return
        fi
        sleep 1
    done
    echo "Relay $1 failed to start" >&2
    return 1
}

fixture() {
    cargo run -q --locked --manifest-path tun/Cargo.toml --example release_upgrade -- "$url" "$certificate" "$@"
}

echo "Registering and probing a device on $previous_tag"
start_relay "$previous_image"
if "$hardware"; then
    hardware_fixture account
else
    fixture account
fi
# Read the verification link from the disposable database instead of sending email.
verification=$("${compose[@]}" exec -T postgres psql -U smoke -d smoke -Atc 'SELECT id FROM verification')
test -n "$verification"
curl --noproxy '*' "${curl_tls[@]}" --fail --silent --show-error --max-time 10 \
    "$url/api/auth/verify?id=$verification" >/dev/null
registration="$test_dir/device.json"
if "$hardware"; then
    hardware_fixture register
else
    fixture register "$registration"
    fixture probe "$registration" "$udp_port"
fi

echo "Upgrading the relay and probing the same device"
start_relay "$current_image"
if "$hardware"; then
    hardware_fixture probe
else
    fixture probe "$registration" "$udp_port"
fi
