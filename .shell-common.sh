# Shared shell configuration — sourced by both .zshrc and .bashrc
# Keep shell-specific syntax OUT of this file (no setopt, compinit, etc.)

# =============================================================================
# PATH
# =============================================================================

export PATH="$HOME/bin:$HOME/.local/bin:$HOME/.cargo/bin:$HOME/dev/iconics:$HOME/go/bin:$HOME/.deno/bin:$HOME/.bun/bin:$HOME/dev/raley-assistant/.venv/bin:$PATH"
export BUN_INSTALL="$HOME/.bun"

# =============================================================================
# SECRETS
# =============================================================================
# All API keys live in ~/.secrets.env (chmod 600)
[[ -f ~/.secrets.env ]] && source ~/.secrets.env

# GitHub token (lazy-loaded)
gh_token() {
  if [[ -z "$GITHUB_TOKEN" ]]; then
    export GITHUB_TOKEN=$(gh auth token 2>/dev/null)
    export GITHUB_MCP_PAT="$GITHUB_TOKEN"
  fi
  echo "$GITHUB_TOKEN"
}

# =============================================================================
# SSL / PROXY
# =============================================================================

if [[ -f "$HOME/.local/share/proxyforge/combined-ca-bundle.pem" ]]; then
  export NODE_EXTRA_CA_CERTS="$HOME/.local/share/proxyforge/combined-ca-bundle.pem"
  export SSL_CERT_FILE="$HOME/.local/share/proxyforge/combined-ca-bundle.pem"
  export REQUESTS_CA_BUNDLE="$HOME/.local/share/proxyforge/combined-ca-bundle.pem"
fi

# =============================================================================
# SSH / AUTH
# =============================================================================

export SSH_AUTH_SOCK="$XDG_RUNTIME_DIR/ssh-agent.socket"
export SSH_ASKPASS="/usr/bin/ksshaskpass"
export SSH_ASKPASS_REQUIRE=prefer
export SUDO_ASKPASS="/usr/bin/ksshaskpass"

# =============================================================================
# GPU / NVIDIA
# =============================================================================

export LIBVA_DRIVER_NAME=nvidia
export NVD_BACKEND=direct
export MOZ_DISABLE_RDD_SANDBOX=1

# =============================================================================
# CODEX
# =============================================================================

export CODEX_DEBUG=1
export CODEX_VERBOSE=1
export CODEX_INTERNAL_ORIGINATOR_OVERRIDE=developer
export CODEX_MANAGED_BY_NPM=1
alias codex="/home/zack/dev/codex/codex-rs/target/xtreme/codex-bolt"

# =============================================================================
# CLAUDE CODE (CLI) — OTEL only
# =============================================================================
# Token budgets, features, and flags are in ~/.claude/settings.json (canonical).
# Only export OTEL vars here so non-CC tools (budget-cli, warden) can emit spans.

export CLAUDE_CODE_ENABLE_TELEMETRY=1
export OTEL_METRICS_EXPORTER=otlp
export OTEL_LOGS_EXPORTER=otlp
export OTEL_EXPORTER_OTLP_PROTOCOL=grpc
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
export OTEL_METRIC_EXPORT_INTERVAL=60000
export OTEL_LOGS_EXPORT_INTERVAL=50000
export DISABLE_ERROR_REPORTING=1

# =============================================================================
# CLAUDE DESKTOP / COWORK (Electron debug)
# =============================================================================

export ELECTRON_ENABLE_LOGGING=2
export CLAUDE_ENABLE_LOGGING=2
export CLAUDE_SWIFT_DEBUG=1
export CLAUDE_SWIFT_TRACE=1
export CLAUDE_SWIFT_DEV=1
export CLAUDE_NATIVE_TRACE=1
export CLAUDE_VM=1
export CLAUDE_VM_DEBUG=1
export COWORK_VM_DEBUG=1
export CLAUDE_COWORK_DEBUG=1
export CLAUDE_COWORK_TRACE_IO=1
export NODE_ENV=development
export REACT_DEVTOOLS_GLOBAL_HOOK=true

alias claude-cowork='command claude-cowork --disable-gpu-compositing'
alias claude-desktop='command claude-desktop --disable-gpu-compositing'
alias claude-logs='tail -f "$(ls -t ~/.claude/debug/*.txt 2>/dev/null | head -1)"'
alias claude-env="python $HOME/Downloads/analyze-claude-env.py"

# =============================================================================
# TOKEN BUDGET (warden/budget-cli runtime)
# =============================================================================

export CONTEXT_BUDGET_TOTAL=280000
export CONTEXT_BUDGET_PARENT=40000
export CONTEXT_BUDGET_SUBAGENT=20000
export CONTEXT_BUDGET_TOOL=10001
export CLAUDE_STATE_DIR="$HOME/.claude/.budget"
export BUDGET_CLI_PATH="$HOME/.cargo/bin/budget-cli"
export MCP_WRAPPER_THRESHOLD=20001
export MCP_WRAPPER_CHUNK_SIZE=2001

alias cbm='claude-budget-monitor'
alias cbm-watch='claude-budget-monitor watch'
alias cbm-report='claude-budget-monitor report'
alias cbm-log='claude-budget-monitor log'
alias cstats='claude-agent-stats'
alias cstats-summary='claude-agent-stats summary'

# =============================================================================
# PROJECT ALIASES
# =============================================================================

# File management
alias ls='eza -F --group-directories-first --color=auto'
alias ll='eza -lh --group-directories-first'
alias la='eza -lah --group-directories-first'
unalias lt 2>/dev/null
lt() { local level="${1:-2}"; shift || true; eza -lT --group-directories-first --level="$level" "$@"; }

alias strings='/usr/bin/strings'
alias files="filearchy"
alias filesd="RUST_LOG=filearchy=debug RUST_BACKTRACE=1 filearchy"
alias filearchy="$HOME/dev/filearchy/scripts/filearchy-dev"

# PDF/Marker
alias marker-digest="$HOME/dev/marker/.venv/bin/python $HOME/dev/marker/digest_pdf.py"
alias marker="$HOME/dev/marker/.venv/bin/python $HOME/dev/marker/digest_pdf.py"

# Iconics
alias iconics-tui="$HOME/dev/iconics/tui2/target/release/iconics-tui2"

# SpecHO
alias specho='uv run --project $HOME/dev/specho-v2 specho'
alias specho-classify='uv run --project $HOME/dev/specho-v2 specho-classify'

# Gemini
alias gemini="node $HOME/dev/gemini-sharp/bundle/gemini.js"
alias gsharp="node $HOME/dev/gemini-sharp/bundle/gemini.js"

# definitelynot.ai
alias defnot='vectorhit'
alias defnot-scan='vectorhit scan'
alias defnot-audit='vectorhit audit'
alias defnot-check='vectorhit check'
alias defnot-clean='vectorhit clean'

# Misc tools
alias alienware="$HOME/dev/alienware-monitor/aw-monitor-control/target/release/aw-monitor-control"
alias kaiser="$HOME/dev/kaiser-cli/.venv/bin/kaiser"
alias codex-patcher="$HOME/dev/codex-patcher/target/release/codex-patcher"
alias bartender="$HOME/dev/bartender/run.sh"
hotbar() { "$HOME/dev/hotbar/run.sh" "$@"; }
hotbar-debug() { "$HOME/dev/hotbar/run.sh" --debug "$@"; }
hotbar-log() { "$HOME/dev/hotbar/run.sh" --debug --file="${1:-$HOME/.cache/hotbar/debug.log}"; }
hotbar-tail() { tail -f "${1:-$HOME/.cache/hotbar/debug.log}"; }
alias tidal='LIBVA_DRIVER_NAME= tidal-hifi --ozone-platform=x11'
alias gimp-ai="$HOME/.local/share/gimp-ai"

export GHIDRA_INSTALL_DIR="/opt/ghidra"
export WINEPREFIX=~/.wine-monitor-control

# =============================================================================
# NETWORK / DNS / FIREWALL
# =============================================================================

[[ -f ~/.config/netsec-aliases.sh ]] && source ~/.config/netsec-aliases.sh

alias network-privacy='sudo $HOME/.local/bin/omarchy-network-privacy'
alias network-privacy-dry-run="$HOME/.local/bin/network-privacy-dry-run"
alias firewall-setup='sudo $HOME/.local/bin/omarchy-firewall-setup'
alias firewall-status='sudo ufw status verbose'
alias firewall-enable='sudo ufw enable'
alias firewall-disable='sudo ufw disable'

alias dns='sudo $HOME/.local/bin/omarchy-dns'
alias dns-status="$HOME/.local/bin/omarchy-dns --status"
alias dns-diag='omarchy-dns --diag'
alias dns-rollback='sudo $HOME/.local/bin/omarchy-dns --rollback'
alias dns-backups="$HOME/.local/bin/omarchy-dns --list-backups"
alias dns-direct='sudo $HOME/.local/bin/omarchy-dns direct-dnscrypt'
alias dns-layered='sudo $HOME/.local/bin/omarchy-dns layered-stack'
alias dns-dhcp='sudo $HOME/.local/bin/omarchy-dns dhcp-fallback'

# =============================================================================
# PYTHON HELPERS
# =============================================================================

alias pyg='source ~/.local/share/python-global/bin/activate'

_pyg_packages="transformers|open_clip|faiss|sklearn|scipy|pandas|matplotlib|emoji"
python() {
  if [[ -z "$VIRTUAL_ENV" ]] && [[ "$*" =~ -c.*import.*($_pyg_packages) || "$*" =~ ($_pyg_packages) ]]; then
    echo -e "\033[33mHint: Type 'pyg' to activate the global Python environment with ML packages\033[0m"
  fi
  command python "$@"
}
pip() {
  if [[ -z "$VIRTUAL_ENV" ]]; then
    echo "[Hint: Type 'pyg' to activate the global Python environment, or use 'uv pip' for project venvs"
  fi
  command pip "$@"
}

venv() {
  local dir="$PWD"
  while [[ "$dir" != "/" ]]; do
    if [[ -d "$dir/.venv" ]]; then
      echo "Activating venv: $dir/.venv"
      source "$dir/.venv/bin/activate"
      return
    fi
    dir="$(dirname "$dir")"
  done
  echo "No .venv found in current directory or parents"
  echo "Create one with: python -m venv .venv"
  return 1
}

# =============================================================================
# FUNCTIONS
# =============================================================================

clawback() {
  local repo="$HOME/dev/clawback"
  (cd "$repo" && cargo build) || return
  "$repo/target/debug/clawback" "$@"
}
clawback_release() {
  local repo="$HOME/dev/clawback"
  (cd "$repo" && cargo build --release) || return
  "$repo/target/release/clawback" "$@"
}
alias clawback-release='clawback_release'

bartender-restart() {
  pkill -9 gjs 2>/dev/null
  sleep 0.5
  "$HOME/dev/bartender/run.sh" &
  disown
}

anthropic-jobs() {
  local app_dir="$HOME/jobs/app"
  local pid_file="$app_dir/var/server.pid"
  if ! [[ -f "$pid_file" ]] || ! kill -0 "$(cat "$pid_file")" 2>/dev/null; then
    frankenphp run --config "$app_dir/Caddyfile" >> "$app_dir/var/server.log" 2>&1 &
    echo $! > "$pid_file"
    sleep 2
  fi
  xdg-open http://localhost:8081 2>/dev/null &
}

# =============================================================================
# MISC
# =============================================================================

alias tarpit='/home/zack/dev/tarpit/tarpit'
alias codex-app='/home/zack/dev/codex-mac-app/codex-linux/start-codex.sh'
alias yc="$HOME/.local/yandex-cloud/bin/yc"
alias claude-wiki="python3 /home/zack/dev/claude-wiki/tools/update_claude_docs.py"
