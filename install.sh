#!/usr/bin/env bash
set -euo pipefail

OWNER="${OWNER:-JhonaCodes}"
REPO="${REPO:-env-craft}"
BIN_NAME="${BIN_NAME:-envcraft}"
INSTALL_DIR="${INSTALL_DIR:-${HOME}/.local/bin}"
VERSION="${VERSION:-latest}"

os="$(uname -s)"
arch="$(uname -m)"

case "${os}" in
  Linux) platform="linux" ;;
  Darwin) platform="macos" ;;
  *)
    echo "Unsupported OS: ${os}" >&2
    exit 1
    ;;
esac

case "${arch}" in
  x86_64|amd64) target_arch="x86_64" ;;
  arm64|aarch64) target_arch="aarch64" ;;
  *)
    echo "Unsupported architecture: ${arch}" >&2
    exit 1
    ;;
esac

asset="envcraft-${platform}-${target_arch}.tar.gz"

if [[ "${VERSION}" == "latest" ]]; then
  download_url="https://github.com/${OWNER}/${REPO}/releases/latest/download/${asset}"
else
  download_url="https://github.com/${OWNER}/${REPO}/releases/download/${VERSION}/${asset}"
fi

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "${tmp_dir}"
}
trap cleanup EXIT

mkdir -p "${INSTALL_DIR}"

echo "Downloading ${download_url}"
curl -fsSL "${download_url}" -o "${tmp_dir}/${asset}"
tar -xzf "${tmp_dir}/${asset}" -C "${tmp_dir}"
install -m 0755 "${tmp_dir}/${BIN_NAME}" "${INSTALL_DIR}/${BIN_NAME}"

echo "Installed ${BIN_NAME} to ${INSTALL_DIR}/${BIN_NAME}"
echo "Run '${BIN_NAME} --version' to verify the installation."
echo "If needed, add ${INSTALL_DIR} to your PATH."
