# Access over Tailscale

Tailscale gives every device a private, encrypted IP with automatic HTTPS — no port
forwarding, no certificates to manage.

## On the machine running bookmark

```sh
curl -fsSL https://tailscale.com/install.sh | sh
sudo tailscale up
bookmark serve --host 0.0.0.0 --port 8080
```

Grant HTTPS certificates and serve the port:

```sh
sudo tailscale cert "$(tailscale status --json | jq -r .Self.DNSName | sed 's/\.$//')"
tailscale serve --bg 8080
```

Open the printed `https://<machine>.<tailnet>.ts.net` URL from any of your devices.

## Lock the server to your tailnet

Set `require_tailscale = true` in `~/.config/bookmark/config.toml` to reject requests that do
not arrive over the `100.64.0.0/10` / Tailscale interface. Combined with binding to the tailnet
IP, this means the login page is never exposed to the public internet.

## Notes

- `tailscale serve` handles TLS termination; bookmark itself speaks plain HTTP on loopback.
- If you use `tailscale funnel`, you are deliberately exposing the service publicly — make sure
  registration is closed (`open_registration = false`) and passwords are strong.
- On Android, use the Tailscale app and reach the phone by its tailnet IP.
