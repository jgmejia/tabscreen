#!/usr/bin/env bash
# Installs the tabscreen host daemon as a per-user systemd service:
# builds the release binary, installs the udev rule granting non-root USB
# access to the tablet (confirmed live 2026-08-20 that this is genuinely
# needed only for a *fresh* AOA handshake, not once already in accessory
# mode -- but a fresh handshake is the common case), ensures vkms loads at
# boot (ADR 0021), and registers+starts the systemd user service.
#
# Run as your normal user, not root -- this script calls `sudo` itself
# for the specific steps that need it (the udev rule and the vkms
# module-load config, both system-wide files under /etc).
set -euo pipefail

if [ "$(id -u)" -eq 0 ]; then
    echo "Run this as your normal user, not root -- it calls sudo itself where needed." >&2
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_INSTALL_DIR="$HOME/.local/bin"
BIN_PATH="$BIN_INSTALL_DIR/tabscreen-host"
SERVICE_DIR="$HOME/.config/systemd/user"
SERVICE_PATH="$SERVICE_DIR/tabscreen.service"
UDEV_RULE_DEST="/etc/udev/rules.d/69-tabscreen-aoa.rules"
VKMS_MODULE_CONF="/etc/modules-load.d/vkms.conf"

if [ -z "${DISPLAY:-}" ] || [ -z "${XAUTHORITY:-}" ]; then
    echo "DISPLAY and/or XAUTHORITY aren't set in this shell -- run this" >&2
    echo "installer from a real logged-in graphical session (a terminal" >&2
    echo "opened from your desktop), not a TTY or a bare SSH session." >&2
    exit 1
fi

echo "==> Building release binary"
(cd "$SCRIPT_DIR" && cargo build --release)

echo "==> Installing binary to $BIN_PATH"
mkdir -p "$BIN_INSTALL_DIR"
cp "$SCRIPT_DIR/target/release/host" "$BIN_PATH"

echo "==> Installing udev rule (needs sudo)"
sudo cp "$SCRIPT_DIR/resources/udev/69-tabscreen-aoa.rules" "$UDEV_RULE_DEST"
sudo udevadm control --reload-rules
sudo udevadm trigger

echo "==> Ensuring vkms loads at boot (needs sudo)"
if [ ! -f "$VKMS_MODULE_CONF" ] || ! grep -qx "vkms" "$VKMS_MODULE_CONF"; then
    echo "vkms" | sudo tee "$VKMS_MODULE_CONF" > /dev/null
fi
if ! lsmod | grep -q "^vkms"; then
    sudo modprobe vkms
fi

echo "==> Installing systemd user service"
mkdir -p "$SERVICE_DIR"
sed -e "s#__DISPLAY__#$DISPLAY#" \
    -e "s#__XAUTHORITY__#$XAUTHORITY#" \
    -e "s#__BIN_PATH__#$BIN_PATH#" \
    "$SCRIPT_DIR/resources/systemd/tabscreen.service.template" > "$SERVICE_PATH"

systemctl --user daemon-reload
systemctl --user enable --now tabscreen.service

echo "==> Done. Status:"
systemctl --user status tabscreen.service --no-pager || true
echo
echo "Logs: journalctl --user -u tabscreen.service -f"
echo "Uninstall: $SCRIPT_DIR/uninstall.sh"
