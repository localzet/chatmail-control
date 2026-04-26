# chatmail-control

`chatmail-control` is a lightweight self-hosted admin/control-plane panel for a chatmail server stack. It is a thin admin UI around existing chatmail/cmdeploy components such as Dovecot, Postfix, doveauth, and chatmail-metadata.

This is not webmail, not a mailbox client, and not a Mailcow/PostfixAdmin replacement.

## Features

- admin login/logout with cookie sessions and Argon2 password hashes;
- dashboard with service state, queue size, user count, active bans, and recent audit events;
- user/mailbox listing through external commands defined in config;
- block/unblock address, domain, IP, and subnet bans with file export and reload commands;
- registration settings stored in SQLite and exported into a generated policy file;
- invite management with token export;
- logs viewer backed by configured host commands;
- health page with service, port, DNS, TLS, queue, and disk checks;
- audit log for admin actions;
- CLI bootstrap for the initial admin user.

## Stack

- Rust
- Axum
- Tokio
- Askama + HTMX
- SQLite via SQLx
- TOML config
- tracing + tracing-subscriber

## Project Decisions

- Session storage is persisted in SQLite via the `sessions` table.
- Shell integration is always argv-based. Commands are never executed through a shell.
- The UI degrades to `unavailable` when an external command is missing or fails.
- Health checks tolerate missing local tools such as `systemctl`, `postqueue`, or `openssl` and render warnings instead of crashing.
- Invite handling is storage/export only. Real auth-side invite enforcement is left as an integration hook and documented below.
- The supported deployment model is a native host install on the mail server, managed by systemd.


## Build From Source

Install Rust, SQLite headers/runtime, and OpenSSL. Then:

```bash
cargo build
cp config.example.toml config.toml
```

Edit `config.toml` before first run:

- set `server.public_url`;
- set `auth.session_secret` to a long random secret;
- set correct file paths for bans, settings, and invite exports;
- adapt command arrays for your actual chatmail deployment.

Run the server:

```bash
chatmail-control serve --config ./config.toml
```

## Create the First Admin

```bash
chatmail-control admin create --config ./config.toml --username admin --password 'CHANGE_ME'
```

Reset password:

```bash
chatmail-control admin reset-password --config ./config.toml --username admin --password 'NEW_SECRET'
```

## Deployment Model

The supported deployment model is a native binary on the same host as Postfix, Dovecot, and other chatmail services, managed by systemd.

You run exactly one process on the server:

```bash
/usr/local/bin/chatmail-control serve --config /etc/chatmail-control/config.toml
```

Why this is the primary model:

- the app needs access to host-level commands such as `doveadm`, `postqueue`, `journalctl`, `systemctl`, and local service state;
- ban, settings, and invite exports are intended to live on the mail host filesystem;
- reload commands are expected to operate on host services such as Postfix, Dovecot, and doveauth.

Recommended server install flow:

1. Download the release tarball from GitHub Releases.
2. Extract it on the mail host.
3. Install the binary to `/usr/local/bin/chatmail-control`.
4. Install the config to `/etc/chatmail-control/config.toml`.
5. Create writable directories:

```bash
sudo useradd --system --home /var/lib/chatmail-control --shell /usr/sbin/nologin chatmail-control || true
sudo install -d -o chatmail-control -g chatmail-control /var/lib/chatmail-control /etc/chatmail-control
```

6. Create the first admin:

```bash
sudo -u chatmail-control /usr/local/bin/chatmail-control admin create \
  --config /etc/chatmail-control/config.toml \
  --username admin \
  --password 'CHANGE_ME'
```

7. Enable the service:

```bash
sudo install -m 0644 systemd/chatmail-control.service /etc/systemd/system/chatmail-control.service
sudo systemctl daemon-reload
sudo systemctl enable --now chatmail-control
```

### Installer Script

For a rustup-style one-liner install flow, use the bundled installer script from the repository:

```bash
curl -fsSL https://raw.githubusercontent.com/localzet/chatmail-control/main/scripts/install.sh | sudo bash
```

Install a specific version:

```bash
curl -fsSL https://raw.githubusercontent.com/localzet/chatmail-control/main/scripts/install.sh | \
  sudo bash -s -- --version v0.1.0
```

What the installer does:

- resolves the requested GitHub release;
- downloads the `*-bundle.tar.gz` release bundle and its `.sha256`;
- verifies the checksum;
- installs the binary to `/usr/local/bin/chatmail-control`;
- installs static, templates, and migrations under `/opt/chatmail-control`;
- installs `config.example.toml` and creates `config.toml` if missing;
- installs and reloads the systemd unit;
- enables and starts the service by default.

Supported flags:

- `--version vX.Y.Z` or `latest`
- `--install-root /opt/chatmail-control`
- `--binary-path /usr/local/bin/chatmail-control`
- `--config-dir /etc/chatmail-control`
- `--state-dir /var/lib/chatmail-control`
- `--service-user chatmail-control`
- `--service-group chatmail-control`
- `--no-enable`
- `--no-start`

## systemd

Binary path in the provided unit:

- `/usr/local/bin/chatmail-control`
- config: `/etc/chatmail-control/config.toml`
- writable paths: `/var/lib/chatmail-control`, `/etc/chatmail-control`

Install:

```bash
sudo install -m 0755 target/release/chatmail-control /usr/local/bin/chatmail-control
sudo install -d -o chatmail-control -g chatmail-control /var/lib/chatmail-control /etc/chatmail-control
sudo install -m 0644 systemd/chatmail-control.service /etc/systemd/system/chatmail-control.service
sudo systemctl daemon-reload
sudo systemctl enable --now chatmail-control
```

## Example Config

Use [config.example.toml](./config.example.toml) as the baseline. The application expects command arrays, not command strings.

Example:

```toml
[users]
list_command = ["doveadm", "user", "*"]
size_command = ["doveadm", "quota", "get", "-u", "{address}"]
message_count_command = ["doveadm", "mailbox", "status", "-u", "{address}", "messages", "INBOX"]
delete_command = ["doveadm", "mailbox", "delete", "-u", "{address}", "-s", "INBOX"]
metadata_command = ["doveadm", "user", "-u", "{address}", "*"]
```

## Reverse Proxy Example

```nginx
server {
    listen 443 ssl http2;
    server_name admin.example.com;

    ssl_certificate     /etc/letsencrypt/live/admin.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/admin.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8088;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto https;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
```

## Bans Integration

The app writes active bans into files configured under `[bans]`:

- `blocked_addresses.txt`
- `blocked_domains.txt`
- `blocked_ips.txt`

Expected line formats:

```text
bad@example.com REJECT blocked by admin
example.org REJECT domain blocked by admin
192.0.2.1 REJECT ip blocked by admin
198.51.100.0/24 REJECT subnet blocked by admin
```

Typical integration path:

1. Point your Postfix restriction maps or policy loader to these generated files.
2. Keep `reload_commands` configured so Postfix/Dovecot reload after admin changes.
3. Validate file ownership and permissions so the service user can update files safely.

## Settings Integration

Registration settings are stored in SQLite and exported to the file configured in `[settings].generated_policy_file`.

The generated file is a TOML snapshot of:

- `registration_mode`
- `max_accounts_per_ip_per_day`
- `max_accounts_per_day`
- `cleanup_empty_mailboxes_after_days`
- `notes`

The configured reload commands are executed after every save. If reload fails, settings still persist, a warning is logged, and an audit event is written.

## Invites Integration Hook

The MVP stores invites and exports active tokens to `[invites].export_file`.

To enforce invite-only registration in your auth pipeline:

1. Read the exported token list from the auth component handling registration.
2. Reject registrations when `registration_mode = "invite_only"` and the token is absent or inactive.
3. Increment `used_count` in the application database from your integration hook if you need hard enforcement.

The current MVP does not decrement or enforce invite usage from the chatmail auth path by itself.

## Health Page

The health page performs:

- `systemctl is-active` checks for configured services;
- local TCP checks for configured ports;
- DNS MX lookup;
- TXT checks for SPF, DMARC, and the DKIM selector;
- TLS probe through `openssl s_client`;
- `postqueue -p`;
- `df -h`.

If one of these tools is unavailable, the page still opens and shows a warning or error row.

## Security Notes

- Default bind is `127.0.0.1:8088`.
- Do not expose this panel directly to the internet without HTTPS, a reverse proxy, and an allowlist.
- Replace `auth.session_secret` before production use.
- Keep `secure_cookies = true` when served behind HTTPS.
- Login rate limiting is in-memory only in MVP scope.
- Passwords are hashed with Argon2 and never logged.
- Command execution is argv-only with placeholder substitution and timeout protection.
- Askama templates escape values by default.
- Audit log stores login success/failure and admin actions.

## Troubleshooting

- Login returns `401`: verify that the admin exists and the password was set with the CLI.
- Users page is empty: check `users.list_command` output manually on the host.
- Mailbox metrics show `unavailable`: the optional command failed or returned unsupported output.
- Health page shows warnings: verify that required host tools and services are available on the mail server.
- Bans were saved but Postfix/Dovecot did not reload: inspect `audit_log` and configured `reload_commands`.
