#!/usr/bin/env bash
set -euo pipefail

REPO="localzet/chatmail-control"
VERSION="${CHATMAIL_CONTROL_VERSION:-latest}"
INSTALL_ROOT="${CHATMAIL_CONTROL_INSTALL_ROOT:-/opt/chatmail-control}"
BINARY_PATH="${CHATMAIL_CONTROL_BINARY_PATH:-/usr/local/bin/chatmail-control}"
CONFIG_DIR="${CHATMAIL_CONTROL_CONFIG_DIR:-/etc/chatmail-control}"
STATE_DIR="${CHATMAIL_CONTROL_STATE_DIR:-/var/lib/chatmail-control}"
ENABLE_SERVICE="${CHATMAIL_CONTROL_ENABLE_SERVICE:-1}"
START_SERVICE="${CHATMAIL_CONTROL_START_SERVICE:-0}"

usage() {
  cat <<'EOF'
chatmail-control installer

Usage:
  install.sh [--version v0.1.0]

Options:
  --version VALUE         Release tag to install, or "latest" (default)
  --install-root PATH     Runtime root for static/systemd assets
  --binary-path PATH      Final binary path
  --config-dir PATH       Configuration directory
  --state-dir PATH        Writable application state directory
  --no-enable             Do not enable the systemd service
  --start                 Start or restart the systemd service after install
  -h, --help              Show this help

Environment variables:
  CHATMAIL_CONTROL_VERSION
  CHATMAIL_CONTROL_INSTALL_ROOT
  CHATMAIL_CONTROL_BINARY_PATH
  CHATMAIL_CONTROL_CONFIG_DIR
  CHATMAIL_CONTROL_STATE_DIR
  CHATMAIL_CONTROL_ENABLE_SERVICE
  CHATMAIL_CONTROL_START_SERVICE
EOF
}

log() {
  printf '[chatmail-control] %s\n' "$*"
}

fail() {
  printf '[chatmail-control] error: %s\n' "$*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --version)
        VERSION="${2:?missing value for --version}"
        shift 2
        ;;
      --install-root)
        INSTALL_ROOT="${2:?missing value for --install-root}"
        shift 2
        ;;
      --binary-path)
        BINARY_PATH="${2:?missing value for --binary-path}"
        shift 2
        ;;
      --config-dir)
        CONFIG_DIR="${2:?missing value for --config-dir}"
        shift 2
        ;;
      --state-dir)
        STATE_DIR="${2:?missing value for --state-dir}"
        shift 2
        ;;
      --no-enable)
        ENABLE_SERVICE=0
        shift
        ;;
      --start)
        START_SERVICE=1
        shift
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        fail "unknown argument: $1"
        ;;
    esac
  done
}

ensure_root() {
  if [[ "$(id -u)" -ne 0 ]]; then
    fail "run this installer as root"
  fi
}

resolve_version() {
  if [[ -n "${VERSION}" && "${VERSION}" != "latest" ]]; then
    return
  fi

  log "resolving latest release for ${REPO}"
  VERSION="$(
    curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
      | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' \
      | head -n1
  )"
  [[ -n "${VERSION}" ]] || fail "failed to resolve latest release tag"
}

install_release() {
  local tmp_dir asset_base archive_name checksum_name checksum_check_name bundle_dir

  tmp_dir="$(mktemp -d)"
  trap '[[ -n "${tmp_dir:-}" ]] && rm -rf "${tmp_dir}"' EXIT

  asset_base="chatmail-control-${VERSION}-linux-amd64"
  archive_name="${asset_base}-bundle.tar.gz"
  checksum_name="${archive_name}.sha256"

  log "downloading ${archive_name}"
  curl -fsSL \
    -o "${tmp_dir}/${archive_name}" \
    "https://github.com/${REPO}/releases/download/${VERSION}/${archive_name}"

  log "downloading checksum"
  curl -fsSL \
    -o "${tmp_dir}/${checksum_name}" \
    "https://github.com/${REPO}/releases/download/${VERSION}/${checksum_name}"

  checksum_check_name="${tmp_dir}/checksum.check"
  awk '{print $1 "  " $NF}' "${tmp_dir}/${checksum_name}" \
    | sed "s#  .*${archive_name}#  ${archive_name}#" > "${checksum_check_name}"

  log "verifying checksum"
  (
    cd "${tmp_dir}"
    sha256sum -c "$(basename "${checksum_check_name}")"
  )

  log "extracting release bundle"
  tar -C "${tmp_dir}" -xzf "${tmp_dir}/${archive_name}"
  bundle_dir="${tmp_dir}/${asset_base}-bundle"
  [[ -d "${bundle_dir}" ]] || fail "release bundle layout is invalid"

  install -d "${INSTALL_ROOT}" "${CONFIG_DIR}" "${STATE_DIR}"
  install -d "${INSTALL_ROOT}/static" "${INSTALL_ROOT}/templates" "${INSTALL_ROOT}/migrations"

  log "installing binary"
  install -m 0755 "${bundle_dir}/chatmail-control" "${BINARY_PATH}"

  log "installing runtime assets"
  cp -R "${bundle_dir}/static/." "${INSTALL_ROOT}/static/"
  cp -R "${bundle_dir}/templates/." "${INSTALL_ROOT}/templates/"
  cp -R "${bundle_dir}/migrations/." "${INSTALL_ROOT}/migrations/"

  log "installing config example"
  install -m 0644 "${bundle_dir}/config.example.toml" "${CONFIG_DIR}/config.example.toml"
  if [[ ! -f "${CONFIG_DIR}/config.toml" ]]; then
    install -m 0644 "${bundle_dir}/config.example.toml" "${CONFIG_DIR}/config.toml"
    log "created ${CONFIG_DIR}/config.toml from example"
  else
    log "preserved existing ${CONFIG_DIR}/config.toml"
  fi

  log "installing systemd unit"
  install -m 0644 "${bundle_dir}/systemd/chatmail-control.service" /etc/systemd/system/chatmail-control.service
  systemctl daemon-reload

  if [[ "${ENABLE_SERVICE}" == "1" ]]; then
    log "enabling systemd service"
    systemctl enable chatmail-control
  fi

  if [[ "${START_SERVICE}" == "1" ]]; then
    log "starting or restarting systemd service"
    systemctl restart chatmail-control || systemctl start chatmail-control
  fi

  cat <<EOF

chatmail-control ${VERSION} installed successfully.

Next steps:
  1. Edit ${CONFIG_DIR}/config.toml
  2. Create the first admin:
     ${BINARY_PATH} admin create --config ${CONFIG_DIR}/config.toml --username admin --password 'CHANGE_ME'
  3. Start or restart the service:
     sudo systemctl restart chatmail-control

EOF
}

main() {
  parse_args "$@"
  ensure_root
  require_command curl
  require_command tar
  require_command sha256sum
  require_command install
  require_command systemctl

  resolve_version
  install_release
}

main "$@"
