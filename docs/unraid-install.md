# Installing Nebulae Server on Unraid

> Posting to Discord? Use the paste-ready 8-message edition:
> **[docs/unraid-install-discord.md](unraid-install-discord.md)**.

This guide walks you through installing Nebulae Server on an Unraid box and
connecting the Nebulae desktop app to it. When you are done, your torrents run
on the server (rtorrent, started and managed by the container) while the
Nebulae desktop app on your workstation drives them — same native interface,
no web UI.

> **Addendum (2026-10-14): the first server release is live.** The container
> image and the matching desktop beta are published now — see
> [the addendum at the bottom](#addendum--first-server-release-is-live-2026-10-14)
> before you start.

## What gets installed

One Docker container, available in Unraid's Community Applications:

| Component | Role |
| --- | --- |
| rtorrent 0.9.8 | The torrent daemon; downloads and seeds your files |
| nebulae-server bridge | A small authenticated HTTP API the desktop app talks to |

The two run side by side inside the container and talk over a local socket
that is never exposed to the network. Only two things leave the container:
the bridge API port and the BitTorrent port.

## Requirements

- Unraid 6.x with the Docker service enabled
- The Nebulae desktop app installed on your computer (grab the installer for
  your platform from <https://github.com/koppot/Nebulae-torrent_client/releases>,
  where the desktop releases are published)
- Your Unraid server's IP address (e.g. `192.168.1.10`), reachable from the
  computer running the desktop app

## Step 1 — Install the container

1. In Unraid, open the **APPS** (Community Applications) tab.
2. Search for **nebulae-server**.
3. Click the install icon on the *nebulae-server* template.

If it does not show up in your app feed yet, add it manually instead:

1. Go to the **DOCKER** tab and click **ADD CONTAINER**.
2. Set **Name** to `nebulae-server`.
3. Set **Repository** to `ghcr.io/koppot/nebulae-server:beta`.
4. Click **APPLY** once — Unraid pulls the image and complains about missing
   settings. That is expected; fill them in per the next step.

## Step 2 — Configure the container

These are the settings that matter:

| Setting | Value | Notes |
| --- | --- | --- |
| **API Token** | A long random string | Required. This is the password the desktop app presents to the bridge. Generate one with `openssl rand -hex 32` or a password manager. |
| **Data directory** | `/mnt/user/appdata/nebulae-server` | Host path mapped to `/data` in the container. Point it at a share on the array instead (e.g. `/mnt/user/downloads/nebulae`) if you want downloads on the array with parity protection. |
| **Bridge port** | `9174` TCP | The desktop app connects here. Leave as is unless the port is taken. |
| **BitTorrent port** | `6881` TCP | Peer traffic. If you already run another torrent client on 6881, map a different host port. |
| **DHT port** | `6881` UDP | Keep matching the BitTorrent port. |

Inside the data directory the container creates and uses three folders:

```
/mnt/user/appdata/nebulae-server/
├── downloads/   # finished and in-progress downloads
├── session/     # rtorrent session state (do not delete while running)
└── watch/       # drop .torrent files here to auto-start them
```

Click **APPLY** and wait for the image to pull and the container to start.

## Step 3 — Verify the server is running

1. On the **DOCKER** tab, click the container's log icon.
2. You should see, in order:
   - `[entrypoint] starting rtorrent (session: /data/session)`
   - `[entrypoint] rtorrent ready; starting nebulae-server bridge`
3. The log stays quiet after that — that is normal. There is no web UI to
   open on port 9174; the port serves the app's API only.

If you see `FATAL: rtorrent SCGI socket did not appear`, stop the container,
delete only the `watch/`, `downloads/` and `session/` contents if you suspect
corruption, and start it again. If it persists, file an issue with the full log.

## Step 4 — Connect the desktop app

1. Open the Nebulae app on your computer.
2. Go to **Settings → Server**.
3. Enable the rtorrent server bridge.
4. Fill in:
   - **Server URL** — `http://<your-unraid-ip>:9174` (e.g. `http://192.168.1.10:9174`)
   - **API token** — the exact token from Step 2
5. Click **Test connection**. It should report reaching the server with the
   rtorrent engine.
6. Restart the Nebulae app. After the restart the app runs entirely against
   the server: the transfer list, add dialogs, and per-torrent controls all
   operate on rtorrent.

The app remembers you enabled this mode. To go back to downloading locally,
disable the bridge in **Settings → Server** and restart the app.

## Everyday use

- **Add torrents** — paste magnet links or .torrent paths in the app, or drop
  `.torrent` files into the `watch/` folder; rtorrent picks them up within
  seconds and they appear in the app's list.
- **Downloads land in** `<data directory>/downloads` on your Unraid share.
- **Pause / resume / recheck / reannounce / categories** work from the app.
- **Rate limits and per-file priority** work too; on rtorrent versions where a
  given knob is unavailable the app tells you instead of failing silently.

## Updating

The `:beta` image tag is updated with each beta release:

1. **DOCKER** tab → toggle the container **off and on** with *Check for
   Updates*, or use **Update** when Unraid offers it.
2. Session state and downloads live in your data directory and survive
   updates untouched.

## Security notes

- The bridge speaks plain HTTP with bearer-token auth. Keep it on your LAN;
  do not port-forward 9174 to the internet.
- If you expose Unraid services through a reverse proxy or VPN (e.g. WireGuard
  tunnel into your LAN), the same URL and token work through the tunnel.
- Only the two ports above are published; the rtorrent control socket stays
  inside the container.

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| **Test connection** fails with 401 / unauthorized | The token in the app does not match the container's **API Token**. Copy it again exactly; no quotes, no trailing spaces. |
| Connection refused / timeout | Wrong IP or port; the container not started; or a firewall between workstation and server. Confirm the log shows the bridge started and that `http://<ip>:9174` answers on the LAN. |
| Container pull fails (denied / manifest unknown) | The GHCR package is not public yet — a first publish can land as a private package. The owner flips it to public in the package's GitHub settings; until then Unraid cannot pull it anonymously. |
| Container starts, app connects, but no download speed | Open the BitTorrent port: forward/host-firewall TCP 6881, and keep UDP 6881 mapped for DHT. Check that the swarm has seeds. |
| Downloads go to the wrong place | The directory is set by the app per torrent and defaults to `/data/downloads` in the container — which is your mapped **Data directory**. If you moved the mapping, re-add torrents or set the location in the app. |
| Port 6881 already in use by another container | In the container config, change the **host** side of the BitTorrent/DHT port mappings (e.g. `6882:6881`); the app and peers follow the host port automatically. |
| Want to wipe everything | Stop the container and delete the data directory's contents. The container image itself is replaced on update, not your data. |

## Addendum — first server release is live (2026-10-14)

This guide was written before the first container image was published. It is
real now, and the instructions work exactly as written. What changed since:

1. **The image exists and is public.** `ghcr.io/koppot/nebulae-server:beta` is
   published and anonymously pullable (verified). Step 1 — Community
   Applications or manual Add Container — works as written. Sanity-check from
   any machine before opening the Unraid UI:
   `docker pull ghcr.io/koppot/nebulae-server:beta`
2. **Desktop app: use v0.1.0-beta.8 or newer.** Server mode
   (**Settings → Server**) first appeared in beta.8, published on
   <https://github.com/koppot/Nebulae-torrent_client/releases>. beta.7 and
   older have no Server tab and cannot connect — update before Step 4.
3. **Windows is unsigned by design.** The project's free-signing application
   (SignPath Foundation) was declined, so the beta.8 installer is unsigned:
   SmartScreen will warn on first run — choose **More info** → **Run anyway**.
   Verify the download against `SHA256SUMS.txt` (or the per-file `.sha256`)
   before running; every release also publishes `WINDOWS-SIGNING-STATUS.txt`
   stating the signing status in plain text. See
   [docs/code-signing-policy.md](code-signing-policy.md).
4. **Two repositories, two jobs.** Server deployment (this guide) lives in
   `koppot/nebulae-server`; desktop app releases live in
   `koppot/Nebulae-torrent_client`.

Everything else — ports, token, data directory, watch folder, connection
steps — is unchanged from the steps above.
