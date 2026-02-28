# Bash Configuration (used by Codex, Claude Code Bash tool, etc.)
[[ $- != *i* ]] && return

# =============================================================================
# CORE BASH
# =============================================================================

HISTFILE=~/.bash_history
HISTSIZE=40000
HISTFILESIZE=40000
HISTCONTROL=ignoredups:erasedups
shopt -s histappend

# =============================================================================
# SHELL INTEGRATIONS (bash-specific)
# =============================================================================

eval "$(starship init bash)"
eval "$(zoxide init bash)"
eval "$(mise activate bash)"

[[ -f /usr/share/fzf/completion.bash ]] && source /usr/share/fzf/completion.bash
[[ -f /usr/share/fzf/key-bindings.bash ]] && source /usr/share/fzf/key-bindings.bash
[ -s "$HOME/.bun/_bun" ] && source "$HOME/.bun/_bun"
. "$HOME/.cargo/env" 2>/dev/null

# =============================================================================
# SHARED CONFIG (env vars, aliases, functions)
# =============================================================================

source ~/.shell-common.sh

# =============================================================================
# OMARCHY
# =============================================================================

source ~/.local/share/omarchy/default/bash/aliases
source ~/.local/share/omarchy/default/bash/functions
source ~/.local/share/omarchy/default/bash/envs

# Kaiser CLI auto-venv
if [[ "$PWD" == "$HOME/dev/kaiser-cli"* ]] && [[ -f "$HOME/dev/kaiser-cli/.venv/bin/activate" ]]; then
  source "$HOME/dev/kaiser-cli/.venv/bin/activate"
fi
