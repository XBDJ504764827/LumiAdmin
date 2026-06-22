#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CSGO_DIR="$ROOT_DIR/csgo"
SOURCE_DIR="$CSGO_DIR/addons/sourcemod/scripting"
PLUGIN_DIR="$CSGO_DIR/addons/sourcemod/plugins"
PROJECT_INCLUDE_DIR="$SOURCE_DIR/include"
SOURCEMOD_DIR="$CSGO_DIR/sourcemod-1.11.0-git6970-linux/addons/sourcemod"
SOURCEMOD_INCLUDE_DIR="$SOURCEMOD_DIR/scripting/include"

SPCOMP="$SOURCEMOD_DIR/scripting/spcomp64"
if [[ ! -x "$SPCOMP" ]]; then
  SPCOMP="$SOURCEMOD_DIR/scripting/spcomp"
fi

if [[ ! -x "$SPCOMP" ]]; then
  echo "SourceMod compiler not found under $SOURCEMOD_DIR/scripting" >&2
  exit 1
fi

mkdir -p "$PLUGIN_DIR"

compile_plugin() {
  local source_file="$1"
  local output_file="$2"

  "$SPCOMP" \
    -i "$PROJECT_INCLUDE_DIR" \
    -i "$SOURCEMOD_INCLUDE_DIR" \
    -o "$PLUGIN_DIR/$output_file" \
    "$SOURCE_DIR/$source_file"
}

compile_plugin "manger_online_reporter.sp" "manger_online_reporter.smx"
compile_plugin "manger_edge_sync.sp" "manger_edge_sync.smx"
