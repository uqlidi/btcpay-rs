#!/bin/sh
# Writes a torrc with a hashed control password, then runs Tor in the foreground.
set -eu

: "${TOR_CONTROL_PASSWORD:?TOR_CONTROL_PASSWORD must be set}"

# `tor --hash-password` prints the salted hash on its last line; earlier lines are log noise.
HASHED="$(tor --hash-password "$TOR_CONTROL_PASSWORD" | tail -n 1)"

# Both ports bind to localhost, which is the whole point: this container shares BTCPay's network
# namespace, so localhost here is localhost for the plugin, and nothing outside that namespace can
# reach the control port. A control port is remote code execution over onion services, so it must
# never be exposed more widely than this.
cat > /tmp/torrc <<EOF
SocksPort 127.0.0.1:9050
ControlPort 127.0.0.1:9051
HashedControlPassword ${HASHED}
DataDirectory /var/lib/tor
Log notice stdout
EOF

exec tor -f /tmp/torrc
