#!/usr/bin/env bash
# Postern Community — one-shot installer.
#
# Installs Docker (if missing), checks out or updates the
# postern-community repo, builds the image, and starts the stack.
# Designed to run on a personal machine or VM. Binds to 127.0.0.1
# only — no remote-access story in the community build.
#
# Tested on Fedora 39+, Debian 12, Ubuntu 22.04+, Rocky/Alma 9, Arch.
# Qubes OS: run inside a standalone Qube based on Fedora or Debian.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/dazller4554328/postern-community/main/install.sh | bash
# or:
#   git clone https://github.com/dazller4554328/postern-community.git
#   cd postern-community && bash install.sh
#
# Flags:
#   --dir <path>    Install location (default: ~/postern-community)
#   --update        Pull latest, rebuild, restart — no docker install
#   --uninstall     Stop stack + delete volumes (asks to confirm)
#   --no-build      Skip image build (useful if a prebuilt image exists)
#   --service       Install systemd unit (postern-community.service)
#   --no-service    Skip the systemd prompt (non-interactive installs)
#   -h | --help     Show this text

set -euo pipefail

REPO_URL="${POSTERN_COMMUNITY_REPO:-https://github.com/dazller4554328/postern-community.git}"
INSTALL_DIR="${POSTERN_INSTALL_DIR:-$HOME/postern-community}"
COMPOSE_REL="deploy/docker/docker-compose.yml"
DOCKERFILE_REL="deploy/docker/Dockerfile"
BIND_URL="http://127.0.0.1:8080"
HEALTH_TIMEOUT=120   # seconds to wait for /health after `up`

C_RESET='\033[0m'; C_CYAN='\033[36m'; C_YELLOW='\033[33m'; C_RED='\033[31m'; C_GREEN='\033[32m'
log()  { printf '%b[postern]%b %s\n' "${C_CYAN}" "${C_RESET}" "$*"; }
ok()   { printf '%b[postern]%b %s\n' "${C_GREEN}" "${C_RESET}" "$*"; }
warn() { printf '%b[postern]%b %s\n' "${C_YELLOW}" "${C_RESET}" "$*" >&2; }
err()  { printf '%b[postern]%b %s\n' "${C_RED}"    "${C_RESET}" "$*" >&2; exit 1; }

need_cmd() { command -v "$1" >/dev/null 2>&1; }

usage() { sed -n '2,/^set -euo/p' "$0" | sed 's/^# \{0,1\}//; /^set -euo/d'; exit 0; }

# ─── OS detection ──────────────────────────────────────────────────
detect_os() {
  [[ -r /etc/os-release ]] || err "cannot read /etc/os-release"
  # shellcheck disable=SC1091
  . /etc/os-release
  OS_ID="${ID:-unknown}"
  OS_ID_LIKE="${ID_LIKE:-}"
  OS_PRETTY="${PRETTY_NAME:-${OS_ID}}"
  log "detected: ${OS_PRETTY}"

  # Fold into a package-manager family.
  case "${OS_ID}" in
    debian|ubuntu|raspbian|linuxmint|pop) PKG_FAMILY=debian ;;
    fedora|rhel|rocky|almalinux|centos)   PKG_FAMILY=rhel   ;;
    arch|manjaro|endeavouros)             PKG_FAMILY=arch   ;;
    *)
      case " ${OS_ID_LIKE} " in
        *" debian "*|*" ubuntu "*)  PKG_FAMILY=debian ;;
        *" rhel "*|*" fedora "*)    PKG_FAMILY=rhel   ;;
        *" arch "*)                 PKG_FAMILY=arch   ;;
        *) warn "unknown distro '${OS_ID}'; will try get.docker.com fallback"
           PKG_FAMILY=unknown ;;
      esac ;;
  esac
}

# ─── sudo wrapper (prompts once, works non-interactively if root) ──
SUDO=""
ensure_sudo() {
  if [[ $EUID -eq 0 ]]; then SUDO=""; return; fi
  need_cmd sudo || err "not running as root and 'sudo' is not installed"
  # Prime sudo so the user doesn't see a surprise prompt mid-build.
  sudo -v || err "sudo authentication failed"
  SUDO="sudo"
  # Keep sudo alive in the background for long builds.
  ( while true; do sudo -n true; sleep 60; kill -0 "$$" 2>/dev/null || exit; done ) 2>/dev/null &
  SUDO_KEEPALIVE=$!
  trap 'kill "${SUDO_KEEPALIVE}" 2>/dev/null || true' EXIT
}

# ─── docker install (per family, with get.docker.com fallback) ─────
install_docker() {
  if need_cmd docker && docker compose version >/dev/null 2>&1; then
    ok "docker + compose plugin already installed"
    # Docker binaries present but daemon might be stopped (common on a
    # fresh Qube / after reboot). Start it so later `docker build` works.
    if ! docker info >/dev/null 2>&1 && need_cmd systemctl; then
      log "docker daemon not running — starting"
      ensure_sudo
      $SUDO systemctl enable --now docker || warn "could not start docker daemon"
    fi
    return
  fi

  log "installing Docker (this may take a minute)…"
  ensure_sudo

  case "${PKG_FAMILY}" in
    debian)
      $SUDO apt-get update -y
      $SUDO apt-get install -y ca-certificates curl gnupg
      $SUDO install -m 0755 -d /etc/apt/keyrings
      curl -fsSL "https://download.docker.com/linux/${OS_ID}/gpg" \
        | $SUDO gpg --dearmor --yes -o /etc/apt/keyrings/docker.gpg
      $SUDO chmod a+r /etc/apt/keyrings/docker.gpg
      CODENAME="$(. /etc/os-release && echo "${VERSION_CODENAME}")"
      echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] \
https://download.docker.com/linux/${OS_ID} ${CODENAME} stable" \
        | $SUDO tee /etc/apt/sources.list.d/docker.list >/dev/null
      $SUDO apt-get update -y
      $SUDO apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
      ;;
    rhel)
      $SUDO dnf -y install dnf-plugins-core || true
      # Fedora uses its own repo URL; RHEL/Rocky/Alma use the centos one.
      if [[ "${OS_ID}" == "fedora" ]]; then
        $SUDO dnf config-manager addrepo --from-repofile=https://download.docker.com/linux/fedora/docker-ce.repo 2>/dev/null \
          || $SUDO dnf config-manager --add-repo https://download.docker.com/linux/fedora/docker-ce.repo
      else
        $SUDO dnf config-manager --add-repo https://download.docker.com/linux/centos/docker-ce.repo
      fi
      $SUDO dnf -y install docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
      ;;
    arch)
      $SUDO pacman -Sy --noconfirm --needed docker docker-compose docker-buildx
      ;;
    *)
      warn "falling back to get.docker.com convenience script"
      curl -fsSL https://get.docker.com | $SUDO sh
      ;;
  esac

  $SUDO systemctl enable --now docker

  # Add invoking user to docker group so they don't need sudo for `docker …`.
  # Takes effect on next login; we still work this session by prefixing with sudo.
  if [[ $EUID -ne 0 ]] && ! id -nG "$USER" | tr ' ' '\n' | grep -qx docker; then
    $SUDO usermod -aG docker "$USER"
    NEED_RELOGIN=1
  fi

  ok "docker installed"
}

# ─── repo checkout (idempotent) ────────────────────────────────────
checkout_repo() {
  if [[ -d "${INSTALL_DIR}/.git" ]]; then
    log "updating existing checkout at ${INSTALL_DIR}"
    git -C "${INSTALL_DIR}" fetch --quiet origin
    # Refuse to blow away local modifications — tell the user what happened.
    if ! git -C "${INSTALL_DIR}" diff --quiet || ! git -C "${INSTALL_DIR}" diff --cached --quiet; then
      warn "local changes in ${INSTALL_DIR}; keeping them — update skipped"
    else
      git -C "${INSTALL_DIR}" reset --hard --quiet origin/main
    fi
  elif [[ -d "${INSTALL_DIR}" ]] && [[ -n "$(ls -A "${INSTALL_DIR}" 2>/dev/null)" ]]; then
    # Directory exists and is non-empty but not a git repo — maybe the user
    # curl'd just the compose file there. Don't clobber; fail loudly so they
    # can decide what to do.
    err "${INSTALL_DIR} exists and is not a git checkout. Move or delete it, or pass --dir <path>."
  else
    log "cloning ${REPO_URL} → ${INSTALL_DIR}"
    mkdir -p "$(dirname "${INSTALL_DIR}")"
    git clone --depth 1 "${REPO_URL}" "${INSTALL_DIR}"
  fi
}

# Some docker invocations need sudo until the user re-logs in post-usermod.
# Prefer `docker` but fall back to `sudo docker` if the daemon refuses us.
docker_cmd() {
  if docker info >/dev/null 2>&1; then
    docker "$@"
  else
    $SUDO docker "$@"
  fi
}

# Compose wrapper that matches whichever docker the user can actually reach.
compose() {
  if docker info >/dev/null 2>&1; then
    docker compose -f "${INSTALL_DIR}/${COMPOSE_REL}" "$@"
  else
    $SUDO docker compose -f "${INSTALL_DIR}/${COMPOSE_REL}" "$@"
  fi
}

# Figure out the image tag the compose file expects, and the path to the
# Dockerfile in this checkout (the monorepo keeps it at deploy/community/
# but the synced community repo keeps it at deploy/docker/).
resolve_image_and_dockerfile() {
  local compose_file="${INSTALL_DIR}/${COMPOSE_REL}"
  [[ -f "${compose_file}" ]] || err "compose file not found at ${compose_file}"

  IMAGE_TAG=$(awk '/^    image:/ {print $2; exit}' "${compose_file}")
  [[ -n "${IMAGE_TAG}" ]] || err "couldn't find 'image:' line in ${compose_file}"

  for candidate in \
      "${INSTALL_DIR}/deploy/docker/Dockerfile" \
      "${INSTALL_DIR}/deploy/community/Dockerfile"; do
    if [[ -f "${candidate}" ]]; then
      DOCKERFILE_PATH="${candidate}"
      return
    fi
  done
  err "no Dockerfile found under ${INSTALL_DIR}/deploy/"
}

# ─── build + up + healthcheck ──────────────────────────────────────
build_and_up() {
  local do_build="${1:-yes}"
  resolve_image_and_dockerfile

  if [[ "${do_build}" == "yes" ]]; then
    # Build directly with the tag the compose file expects. After this,
    # `compose up` finds the image locally and never tries to pull from
    # GHCR (which may not have a published image yet). No compose-file
    # surgery needed.
    log "building ${IMAGE_TAG} (first run takes 5–15 min — Rust release build)…"
    docker_cmd build \
      -t "${IMAGE_TAG}" \
      -f "${DOCKERFILE_PATH}" \
      "${INSTALL_DIR}"
  fi
  log "starting stack"
  compose up -d

  log "waiting for http://127.0.0.1:8080/health (up to ${HEALTH_TIMEOUT}s)…"
  local deadline=$(( $(date +%s) + HEALTH_TIMEOUT ))
  while (( $(date +%s) < deadline )); do
    if curl -fsS --max-time 2 "${BIND_URL}/health" 2>/dev/null | grep -q '"status":"ok"'; then
      ok "Postern Community is up → ${BIND_URL}"
      return 0
    fi
    sleep 2
  done

  warn "health check timed out. Logs:"
  compose logs --tail=40 || true
  err "startup did not become healthy in ${HEALTH_TIMEOUT}s"
}

# ─── systemd service (optional) ────────────────────────────────────
# Docker's `restart: unless-stopped` already makes the container come
# back after crashes and after a docker daemon restart — but a
# dedicated systemd unit gives the user a normal ops surface
# (`systemctl status postern-community`, journal integration, an
# explicit stop/start command that doesn't depend on remembering the
# compose file path). We install it on request, not by default.
SERVICE_NAME="postern-community.service"
SERVICE_PATH="/etc/systemd/system/${SERVICE_NAME}"

service_is_installed() {
  [[ -f "${SERVICE_PATH}" ]]
}

# Write and enable the unit. Idempotent — re-writing the file is
# cheap, and `enable --now` is a no-op if the unit is already running.
install_service() {
  need_cmd systemctl || { warn "systemd not available; skipping service install"; return; }
  ensure_sudo

  local docker_bin
  docker_bin=$(command -v docker)
  [[ -n "${docker_bin}" ]] || err "cannot locate 'docker' binary for service ExecStart"

  local compose_abs="${INSTALL_DIR}/${COMPOSE_REL}"
  [[ -f "${compose_abs}" ]] || err "compose file missing at ${compose_abs}"

  local unit
  unit=$(cat <<EOF
[Unit]
Description=Postern Community — self-hosted mail
Documentation=https://github.com/dazller4554328/postern-community
Requires=docker.service
After=docker.service network-online.target
Wants=network-online.target

[Service]
Type=oneshot
RemainAfterExit=yes
WorkingDirectory=${INSTALL_DIR}
# Always re-apply `up -d` on start so config changes to the compose
# file (e.g. after --update) are picked up at the next systemctl start
# without a separate docker compose step.
ExecStart=${docker_bin} compose -f ${compose_abs} up -d
ExecStop=${docker_bin} compose -f ${compose_abs} down
TimeoutStartSec=10min

[Install]
WantedBy=multi-user.target
EOF
  )

  log "writing ${SERVICE_PATH}"
  echo "${unit}" | $SUDO tee "${SERVICE_PATH}" >/dev/null
  $SUDO systemctl daemon-reload
  $SUDO systemctl enable --now "${SERVICE_NAME}"
  ok "service active → systemctl status ${SERVICE_NAME}"
}

remove_service() {
  service_is_installed || return
  ensure_sudo
  $SUDO systemctl disable --now "${SERVICE_NAME}" 2>/dev/null || true
  $SUDO rm -f "${SERVICE_PATH}"
  $SUDO systemctl daemon-reload
  log "removed ${SERVICE_NAME}"
}

# Decide whether to install the service:
# - explicit --service or --no-service wins
# - already installed: skip (idempotent no-ask)
# - interactive TTY: ask
# - non-interactive (curl | bash, CI): skip with a hint
maybe_install_service() {
  case "${SERVICE_MODE:-ask}" in
    install) install_service; return ;;
    skip)    return ;;
  esac

  if service_is_installed; then
    log "systemd unit already installed — reloading"
    install_service    # rewrite file so paths track the current --dir
    return
  fi

  if [[ ! -t 0 ]] || [[ ! -t 1 ]]; then
    echo
    log "tip: for boot-persistent ops, re-run with --service to install a systemd unit"
    return
  fi

  echo
  read -r -p "Install systemd service so Postern starts on boot? [y/N]: " reply
  case "${reply:-N}" in
    y|Y|yes|YES) install_service ;;
    *)           log "skipping systemd service install" ;;
  esac
}

# ─── actions ───────────────────────────────────────────────────────
do_install() {
  need_cmd curl || err "please install curl first"
  need_cmd git  || err "please install git first"
  need_cmd python3 || err "please install python3 (needed for compose patching)"
  detect_os
  install_docker
  checkout_repo
  build_and_up "yes"
  maybe_install_service

  echo
  ok "next steps:"
  printf '  1. open %s in a browser on this machine\n' "${BIND_URL}"
  printf '  2. set a master password — this derives the DB + blob encryption keys\n'
  printf '  3. add a mail account\n'
  if [[ "${NEED_RELOGIN:-0}" -eq 1 ]]; then
    echo
    warn "you were added to the 'docker' group — log out & back in to use 'docker' without sudo"
  fi
}

do_update() {
  need_cmd git || err "git missing"
  [[ -d "${INSTALL_DIR}/.git" ]] || err "not a git checkout: ${INSTALL_DIR}"
  # Prime sudo so docker_cmd's fallback has something to fall back TO
  # when the user isn't in the docker group yet (common right after
  # a fresh install that hasn't re-logged in).
  ensure_sudo
  log "pulling latest"
  git -C "${INSTALL_DIR}" fetch --quiet origin
  git -C "${INSTALL_DIR}" reset --hard --quiet origin/main
  build_and_up "yes"
}

do_uninstall() {
  [[ -d "${INSTALL_DIR}" ]] || err "nothing at ${INSTALL_DIR}"
  read -r -p "Delete stack + volumes at ${INSTALL_DIR}? (type 'yes'): " confirm
  [[ "${confirm}" == "yes" ]] || { log "cancelled"; exit 0; }
  remove_service
  compose down -v || true
  log "volumes removed. The checkout at ${INSTALL_DIR} is left in place."
}

# ─── arg parse ─────────────────────────────────────────────────────
MODE=install
NO_BUILD=no
SERVICE_MODE=ask   # ask | install | skip
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dir)        INSTALL_DIR="$2"; shift 2 ;;
    --update)     MODE=update; shift ;;
    --uninstall)  MODE=uninstall; shift ;;
    --no-build)   NO_BUILD=yes; shift ;;
    --service)    SERVICE_MODE=install; shift ;;
    --no-service) SERVICE_MODE=skip; shift ;;
    -h|--help)    usage ;;
    *) err "unknown flag: $1 (see --help)" ;;
  esac
done

case "${MODE}" in
  install)   do_install ;;
  update)    do_update ;;
  uninstall) do_uninstall ;;
esac
