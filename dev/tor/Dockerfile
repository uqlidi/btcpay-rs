# Tor for the coinswap plugin to publish its maker as an onion service.
#
# Built here rather than pulled because there is no maintained first-party Tor image:
# `btcpayserver/tor` does not exist on Docker Hub and `dperson/torproxy` has been unmaintained
# for years. This is small enough that owning it is cheaper than depending on either.
#
# It exists as a container at all only because of how coinswap reaches Tor. See the `tor` service
# in docker-compose.yml: coinswap hardcodes 127.0.0.1 for the control port, the SOCKS proxy and
# the onion service's forward target, so this has to share BTCPay's network namespace rather than
# be a service reachable by name.
FROM alpine:3.22

RUN apk add --no-cache tor

# The control port needs an auth method or Tor refuses to start, and coinswap authenticates with
# a password (`AUTHENTICATE "..."`) rather than a cookie. Tor wants that password hashed, and the
# hash is salted so it cannot be baked into the image and still match an operator-chosen password.
# Hence generating torrc at startup.
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

USER tor
ENTRYPOINT ["/entrypoint.sh"]
