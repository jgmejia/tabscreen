#!/usr/bin/env bash
# Reverses install.sh: stops and removes the systemd user service, the
# installed binary, the udev rule, and the vkms module-load config. Run
# as your normal user (calls sudo itself for the /etc changes).
set -euo pipefail

if [ "$(id -u)" -eq 0 ]; then
    echo "Run this as your normal user, not root -- it calls sudo itself where needed." >&2
    exit 1
fi

BIN_PATH="$HOME/.local/bin/tabscreen-host"
SERVICE_PATH="$HOME/.config/systemd/user/tabscreen.service"
UDEV_RULE_DEST="/etc/udev/rules.d/69-tabscreen-aoa.rules"
VKMS_MODULE_CONF="/etc/modules-load.d/vkms.conf"

echo "==> Stopping and disabling systemd service"
systemctl --user disable --now tabscreen.service 2>/dev/null || true
rm -f "$SERVICE_PATH"
systemctl --user daemon-reload

echo "==> Removing installed binary"
rm -f "$BIN_PATH"

echo "==> Removing udev rule (needs sudo)"
if [ -f "$UDEV_RULE_DEST" ]; then
    sudo rm -f "$UDEV_RULE_DEST"
    sudo udevadm control --reload-rules
    sudo udevadm trigger
fi

echo "==> Removing vkms module-load config (needs sudo)"
if [ -f "$VKMS_MODULE_CONF" ] && grep -qx "vkms" "$VKMS_MODULE_CONF"; then
    sudo rm -f "$VKMS_MODULE_CONF"
fi
echo "(vkms module itself is left loaded until next reboot -- harmless idle otherwise)"

echo "==> Done."
