#!/usr/bin/env bash
# Builds the `facile` command into bin/facile: run on Heroku by RustConfig once
# the emk/rust buildpack has installed Rust. Works by hand too: ./heroku-build.sh
# The Procfile then serves the site: bin/facile serve site.
set -euo pipefail

# If the git dependencies (Rythme) are private, Cargo fetches them through git
# with the REPOS_TOKEN config var, a GitHub token that can read them (the
# buildpack hands config vars over in ENV_DIR). The token stays out of every URL.
if [ -z "${REPOS_TOKEN:-}" ] && [ -n "${ENV_DIR:-}" ] && [ -f "$ENV_DIR/REPOS_TOKEN" ]; then
  REPOS_TOKEN=$(<"$ENV_DIR/REPOS_TOKEN")
fi
if [ -n "${REPOS_TOKEN:-}" ]; then
  export REPOS_TOKEN CARGO_NET_GIT_FETCH_WITH_CLI=true GIT_CONFIG_COUNT=1 \
    GIT_CONFIG_KEY_0=credential.https://github.com.helper \
    GIT_CONFIG_VALUE_0='!f() { test "$1" = get && echo username=x-access-token && echo "password=$REPOS_TOKEN"; }; f'
fi
cd "$(dirname "$0")"
# On Heroku, Cargo builds into the buildpack's cache (CARGO_TARGET_DIR).
target="${CARGO_TARGET_DIR:-target}"

echo "-----> Building facile"
cargo build --release --locked -p facile --bin facile
mkdir -p bin
cp "$target/release/facile" bin/facile
echo "       bin/facile ready ($(bin/facile version))"
