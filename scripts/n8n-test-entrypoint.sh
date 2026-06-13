#!/bin/sh
# Prepare community-node symlink for integration tests, then start n8n as the node user.
set -e

NODES_DIR="/home/node/.n8n/nodes"
MODULES_DIR="${NODES_DIR}/node_modules"
PACKAGE_LINK="${MODULES_DIR}/n8n-nodes-unihook-zoom-trigger"

mkdir -p "$MODULES_DIR"

if [ ! -f "${NODES_DIR}/package.json" ]; then
	echo '{"name":"installed-nodes","private":true,"dependencies":{}}' >"${NODES_DIR}/package.json"
fi

if [ -d /opt/zoom-node-package ]; then
	ln -sfn /opt/zoom-node-package "$PACKAGE_LINK"
fi

chown -R node:node /home/node/.n8n

exec su node -s /bin/sh -c 'exec tini -- /docker-entrypoint.sh'
