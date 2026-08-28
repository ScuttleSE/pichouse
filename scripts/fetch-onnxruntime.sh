#!/usr/bin/env bash
# Fetch the ONNX Runtime shared library that the face feature needs at run time.
#
# The face feature uses `ort` with the `load-dynamic` feature. That feature
# loads `libonnxruntime.so` at run time with dlopen. The library is not built
# into the pichouse binary. The build does not need it. The run needs it.
#
# This script downloads the official Microsoft prebuilt ONNX Runtime for
# Linux x64, pinned to one version with a checked SHA-256, into
# vendor/onnxruntime/. The build script and the app read the library from
# there. A release must ship the same library beside the binary.
#
# Run this script once before the first run, and in CI before the build.

set -euo pipefail

ORT_VERSION="1.22.0"
ORT_SHA256="3da6146e14e7b8aaec625dde11d6114c7457c87a5f93d744897da8781e35c673"
ORT_URL="https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-x64-${ORT_VERSION}.tgz"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${ROOT}/vendor/onnxruntime"
SOFILE="${DEST}/lib/libonnxruntime.so.${ORT_VERSION}"

# Skip the download if the library is present and the hash matches.
if [ -f "${SOFILE}" ]; then
  if echo "${ORT_SHA256}  ${SOFILE}" | sha256sum -c - >/dev/null 2>&1; then
    echo "ONNX Runtime ${ORT_VERSION} is present and verified."
    exit 0
  fi
  echo "ONNX Runtime present but hash mismatch. Re-downloading."
fi

TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT

echo "Downloading ONNX Runtime ${ORT_VERSION} ..."
curl -sL -o "${TMP}/ort.tgz" "${ORT_URL}"

echo "Verifying archive ..."
tar xzf "${TMP}/ort.tgz" -C "${TMP}" --strip-components=1

DL_SO="${TMP}/lib/libonnxruntime.so.${ORT_VERSION}"
echo "${ORT_SHA256}  ${DL_SO}" | sha256sum -c -

mkdir -p "${DEST}"
# Copy the library, its symlinks, and the license only. Skip headers and cmake.
rm -rf "${DEST}/lib"
mkdir -p "${DEST}/lib"
cp -a "${TMP}/lib/libonnxruntime.so"* "${DEST}/lib/"
cp -a "${TMP}/LICENSE" "${DEST}/LICENSE" 2>/dev/null || true
cp -a "${TMP}/VERSION_NUMBER" "${DEST}/VERSION_NUMBER" 2>/dev/null || true

echo "ONNX Runtime ${ORT_VERSION} is ready at ${DEST}/lib."
