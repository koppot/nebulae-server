# Unraid install guide — Discord edition

Discord-ready rewrite of `docs/unraid-install.md`. Discord does not render
tables or horizontal rules and caps a message at 2,000 characters, so this
version is plain bullets plus code fences, split into 8 messages. Every block
below fits a single message with room to spare.

**How to post:** for each message below, copy everything inside the
four-backtick block and paste it into Discord as its own message, in order.
The four backticks themselves are not part of the message — they just mark
the exact copy boundaries. Do not merge messages; post them one by one.

---

## MESSAGE 1 of 8 — Intro

````
**Nebulae Server — Unraid Install Guide (Beta)**

Run your torrents on the Unraid server with rtorrent, and drive them from the Nebulae desktop app on your PC. Same native app, no web UI.

**What you are installing — one Docker container:**
• rtorrent 0.9.8 — the torrent daemon that downloads and seeds your files
• nebulae-server bridge — a small authenticated API the desktop app talks to

The two run side by side in the container and talk over a local socket. Only two things are exposed to the network: the bridge API port and the BitTorrent port.

**What you need:**
• Unraid 6.x with the Docker service enabled
• The Nebulae desktop app on your computer (grab the installer from `github.com/koppot/Nebulae-torrent_client/releases` — where the desktop releases are published)
• Your Unraid server IP, e.g. `192.168.1.10`
````

---

## MESSAGE 2 of 8 — Install the container

````
**Step 1 — Install the container**

In Unraid, open the **APPS** tab (Community Applications):
1. Search for `nebulae-server`
2. Click the install icon on the **nebulae-server** template

Not showing up in your app feed yet? Add it manually instead:
1. **DOCKER** tab → **ADD CONTAINER**
2. **Name:** `nebulae-server`
3. **Repository:** `ghcr.io/koppot/nebulae-server:beta`
4. Click **APPLY** once — Unraid pulls the image and complains about missing settings. That's expected; fill them in per Step 2.
````

---

## MESSAGE 3 of 8 — Configure the container

````
**Step 2 — Configure the container**

• **API Token** (required) — a long random string. This is the password the desktop app presents to the bridge. Generate one with `openssl rand -hex 32` or your password manager.
• **Data directory** — default `/mnt/user/appdata/nebulae-server`. Want downloads on the array with parity protection? Point it at a share instead, e.g. `/mnt/user/downloads/nebulae`.
• **Bridge port** — `9174` TCP. The desktop app connects here. Leave it as is unless the port is taken.
• **BitTorrent port** — `6881` TCP. Already running another torrent client on 6881? Map a different host port.
• **DHT port** — `6881` UDP. Keep it matching the BitTorrent port.

Click **APPLY** and wait for the image to pull and the container to start.
````

---

## MESSAGE 4 of 8 — Folder layout + verify

````
**Inside the data directory** (the container creates these):
```
downloads/   your finished + in-progress downloads
session/     rtorrent state — do not delete while running
watch/       drop .torrent files here to auto-start them
```

**Step 3 — Verify it's running**

1. **DOCKER** tab → click the container's log icon
2. You should see, in order:
`[entrypoint] starting rtorrent (session: /data/session)`
`[entrypoint] rtorrent ready; starting nebulae-server bridge`
3. Quiet after that = normal. There is no web page on 9174 — the port serves the app's API only.

Seeing `FATAL: rtorrent SCGI socket did not appear`? Stop the container and start it again. If it persists, file an issue with the full log.
````

---

## MESSAGE 5 of 8 — Connect the desktop app

````
**Step 4 — Connect the desktop app**

1. Open the Nebulae app on your computer
2. Go to **Settings → Server**
3. Enable the rtorrent server bridge
4. Fill in:
• **Server URL:** `http://<your-unraid-ip>:9174`
• **API token:** the exact token from Step 2
5. Click **Test connection** — it should report reaching the server with the rtorrent engine
6. Restart the Nebulae app

After the restart the whole app runs against the server: the transfer list, add dialogs, and per-torrent controls all operate on rtorrent. To go back to downloading locally, untick the box in **Settings → Server** and restart the app.
````

---

## MESSAGE 6 of 8 — Everyday use + updating + security

````
**Everyday use**
• Add torrents in the app (magnet links or .torrent files), or drop `.torrent` files into the `watch/` folder — they appear in the app within seconds
• Downloads land in `<data directory>/downloads`
• Pause / resume / recheck / reannounce / categories all work from the app
• Rate limits and per-file priority work too; where a knob is unavailable, the app tells you instead of failing silently

**Updating**
The `:beta` tag is updated with each beta release. **DOCKER** tab → **Check for Updates** / use **Update** when Unraid offers it. Downloads and session state live in your data directory and survive updates untouched.

**Security notes**
• The bridge speaks plain HTTP with bearer-token auth. Keep it on your LAN — do not port-forward 9174 to the internet.
• Behind a WireGuard tunnel into your LAN, the same URL + token work unchanged.
• Only the two ports above are published; the rtorrent control socket stays inside the container.
````

---

## MESSAGE 7 of 8 — Troubleshooting

````
**Troubleshooting**

• **Test connection fails with 401 / unauthorized** — the token in the app doesn't match the container's API Token. Copy it again exactly: no quotes, no trailing spaces.
• **Connection refused / timeout** — wrong IP or port, container not started, or a firewall in between. Confirm the log shows the bridge started and that `http://<ip>:9174` answers on your LAN.
• **Docker pull fails (denied / manifest unknown)** — the package went private or was renamed. Confirm the image path is exactly `ghcr.io/koppot/nebulae-server:beta` and that the package is public (it is, as of the first release).
• **App connects but no download speed** — open the BitTorrent port: forward/host-firewall TCP 6881, and keep UDP 6881 mapped for DHT. Also check the swarm has seeds.
• **Downloads go to the wrong place** — the location defaults to `/data/downloads` (= your mapped data directory). Set the location per torrent in the app if you moved the mapping.
• **Port 6881 already in use by another container** — change the host side of the mapping (e.g. `6882:6881`); the app and peers follow the host port automatically.
• **Want to wipe everything** — stop the container and delete the data directory's contents. The image is replaced on update; your data isn't.
````

## MESSAGE 8 of 8 — Addendum (first release is live)

````
**Addendum — first server release is live (2026-10-14)**

This guide was written before the first container image existed. It's published now, and everything above works as written.

• **The image is real and public.** `ghcr.io/koppot/nebulae-server:beta` is up and anonymously pullable. Sanity-check from any machine first: `docker pull ghcr.io/koppot/nebulae-server:beta`

• **Desktop app: use v0.1.0-beta.8 or newer.** Server mode (Settings → Server) first appeared in beta.8 on `github.com/koppot/Nebulae-torrent_client/releases`. beta.7 and older have no Server tab and cannot connect — update before Step 4.

• **Windows is unsigned by design.** The project's free-signing application was declined, so the beta.8 installer is unsigned: SmartScreen will warn — "More info" → "Run anyway". Verify the download against `SHA256SUMS.txt` (or the per-file `.sha256`) first; each release also publishes `WINDOWS-SIGNING-STATUS.txt` stating the signing status in plain text.

• **Two repos, two jobs:** server deployment (this guide) = `koppot/nebulae-server`; desktop app releases = `koppot/Nebulae-torrent_client`.

Everything else — ports, token, data directory, watch folder, connection steps — is exactly as documented above.
````
