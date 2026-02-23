# Development VPN Setup

WireGuard tunnel between Arch Linux dev machine and Debian 13 US VPS. Routes Binance API traffic through the VPS so requests originate from a US IP instead of Canada.

This is development-phase infrastructure only. In production, the service runs directly on the VPS — no VPN involved.

---

## Why WireGuard

- Kernel-space — 2-5ms overhead vs OpenVPN's 10-20ms
- ~4,000 lines of code. Config is a single file per side
- In mainline Linux since 5.6 — both Arch and Debian 13 ship it. Nothing to compile
- 50-80% less CPU than OpenVPN. The VPS will also run the production service
- Modern crypto (ChaCha20, Curve25519), no cipher negotiation

---

## Server Setup (Debian 13 VPS)

### Install and generate keys

```bash
sudo apt update && sudo apt install wireguard
wg genkey | sudo tee /etc/wireguard/server_private.key | wg pubkey | sudo tee /etc/wireguard/server_public.key
sudo chmod 600 /etc/wireguard/server_private.key
```

### Enable IP forwarding

```bash
echo "net.ipv4.ip_forward = 1" | sudo tee /etc/sysctl.d/99-wireguard.conf
sudo sysctl --system
```

### Server config — `/etc/wireguard/wg0.conf`

Find your public-facing interface name first: `ip route show default` (look for the `dev` field — likely `eth0`, `ens3`, `ens5`). Substitute below.

```ini
[Interface]
Address = 10.66.66.1/24
ListenPort = 51820
PrivateKey = <server_private.key contents>

# NAT: masquerade client traffic so it exits with the VPS public IP
PostUp = iptables -A FORWARD -i wg0 -j ACCEPT; iptables -A FORWARD -o wg0 -j ACCEPT; iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE
PostDown = iptables -D FORWARD -i wg0 -j ACCEPT; iptables -D FORWARD -o wg0 -j ACCEPT; iptables -t nat -D POSTROUTING -o eth0 -j MASQUERADE

[Peer]
PublicKey = <client_public.key contents>
AllowedIPs = 10.66.66.2/32
```

### Firewall and start

```bash
sudo ufw allow 51820/udp
sudo systemctl enable --now wg-quick@wg0
```

---

## Client Setup (Arch Linux)

### Install and generate keys

```bash
sudo pacman -S wireguard-tools
wg genkey | sudo tee /etc/wireguard/client_private.key | wg pubkey | sudo tee /etc/wireguard/client_public.key
sudo chmod 600 /etc/wireguard/client_private.key
```

Copy the client public key back to the server's `[Peer]` section.

### Client config — `/etc/wireguard/wg0.conf`

```ini
[Interface]
Address = 10.66.66.2/24
PrivateKey = <client_private.key contents>
PostUp = /usr/local/bin/binance-route-update.sh
PostDown = ip route flush table binance; ip rule del table binance 2>/dev/null

[Peer]
PublicKey = <server_public.key contents>
Endpoint = <VPS_PUBLIC_IP>:51820
AllowedIPs = 10.66.66.0/24, 0.0.0.0/0
Table = off
PersistentKeepalive = 25
```

`AllowedIPs = 0.0.0.0/0` with `Table = off` tells WireGuard to accept return traffic from any IP through the tunnel, but prevents wg-quick from adding system routes automatically. The route script handles routing instead.

### Start

```bash
sudo systemctl enable --now wg-quick@wg0
```

---

## Split Tunneling — Route Only Binance Traffic

Only Binance API traffic goes through the VPN. Everything else uses your normal connection.

The challenge: Binance uses AWS CloudFront. Their domains (`api.binance.com`, `fapi.binance.com`, `stream.binance.com`) resolve to IPs that change frequently. Can't hardcode a static set.

### Route update script — `/usr/local/bin/binance-route-update.sh`

```bash
#!/bin/bash
# Resolve Binance API domains and route their IPs through WireGuard

DOMAINS="api.binance.com fapi.binance.com stream.binance.com api1.binance.com api2.binance.com api3.binance.com api4.binance.com"
WG_INTERFACE="wg0"
WG_GATEWAY="10.66.66.1"
ROUTE_TABLE="binance"

# Ensure custom routing table exists
if ! grep -q "^200 ${ROUTE_TABLE}$" /etc/iproute2/rt_tables; then
    echo "200 ${ROUTE_TABLE}" >> /etc/iproute2/rt_tables
fi

# Flush old routes
ip route flush table ${ROUTE_TABLE} 2>/dev/null

# Resolve each domain and add routes
for domain in $DOMAINS; do
    ips=$(dig +short "$domain" A | grep -E '^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$')
    for ip in $ips; do
        ip route add "$ip/32" via "$WG_GATEWAY" dev "$WG_INTERFACE" table ${ROUTE_TABLE} 2>/dev/null
    done
done

# Add routing rule (if not already present)
if ! ip rule show | grep -q "lookup ${ROUTE_TABLE}"; then
    ip rule add table ${ROUTE_TABLE} priority 100
fi

logger "binance-route-update: Routes updated for ${DOMAINS}"
```

```bash
sudo chmod +x /usr/local/bin/binance-route-update.sh
```

### Refresh routes on a schedule

CloudFront IPs change. Refresh every 5 minutes:

```bash
# sudo crontab -e
*/5 * * * * /usr/local/bin/binance-route-update.sh
```

The script runs once on WireGuard startup (via `PostUp`) and then every 5 minutes via cron to catch IP changes.

---

## DNS Notes

- **DNS queries don't need to route through the VPN.** Your local DNS resolves the domain to an IP, then the routing table sends that IP's traffic through WireGuard. This works.
- **CloudFront geo-steering:** CloudFront may return different edge IPs based on where the DNS query originates. This is fine — the edge server checks the *request* source IP (your VPS), not the DNS query source. If you hit issues, force resolution through the VPS: `dig +short "$domain" A @10.66.66.1`
- **WebSocket connections** (`stream.binance.com`) are long-lived. Existing connections survive IP changes. On reconnect, the cron job will have already updated routes for the new IP.

---

## Verification

### Tunnel is up

```bash
sudo wg show
# Should show peer with recent handshake and non-zero transfer
ping 10.66.66.1
```

### Binance traffic routes through VPN

```bash
# Resolve a Binance domain
dig +short api.binance.com

# Check routing for that IP
ip route get <resolved_ip>
# Should show: via 10.66.66.1 dev wg0

# Trace the route
traceroute -n <resolved_ip>
# First hop should be 10.66.66.1
```

### Binance API responds

```bash
# This is the definitive test — if it returns JSON, you're through
curl -s https://api.binance.com/api/v3/time
# {"serverTime":1740000000000}

curl -s https://api.binance.com/api/v3/ping
# {}
```

### Non-Binance traffic stays local

```bash
ip route get 8.8.8.8
# Should NOT show wg0

curl -s https://api.ipify.org
# Should show your Canadian ISP IP, not the VPS IP
```

### Routes are populated

```bash
ip route show table binance
# Should list Binance IPs with via 10.66.66.1 dev wg0
```

---

## VPS Firewall

Lock down the VPS:

```bash
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow ssh
sudo ufw allow 51820/udp
sudo ufw enable
```

Outbound unrestricted for Binance API calls. Inbound only SSH and WireGuard. Add production service ports when needed.

---

## Notes

- **Production:** When the service runs directly on the VPS, it makes Binance API calls from its own IP. No VPN involved. This setup is temporary dev infrastructure.
- **Binance IP whitelisting:** Once working, whitelist the VPS public IP on any Binance API keys. Confirms they see the correct IP and adds a security layer.
- **ToS awareness:** Using a VPN to access Binance from a restricted region is against their Terms of Service. They don't actively enforce this for data-only access, but be aware. The production setup (service running directly on the US VPS) is on firmer ground.
