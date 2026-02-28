# ZSH Configuration (Interactive Shell)
# Note: Codex uses bash (configured in ~/.config/codex/config.toml)
[[ $- != *i* ]] && return

# =============================================================================
# CORE ZSH
# =============================================================================

HISTFILE=~/.zsh_history
HISTSIZE=40000
SAVEHIST=40000
setopt SHARE_HISTORY INC_APPEND_HISTORY HIST_IGNORE_DUPS HIST_FIND_NO_DUPS HIST_REDUCE_BLANKS

autoload -Uz compinit && compinit
zstyle ':completion:*' matcher-list 'm:{a-zA-Z}={A-Za-z}' 'r:|[._-]=* r:|=*' 'l:|=* r:|=*'
zstyle ':completion:*' menu select
zstyle ':completion:*' list-colors ''
zstyle ':completion:*:descriptions' format '%B%d%b'

typeset -U path

# =============================================================================
# SHELL INTEGRATIONS (zsh-specific)
# =============================================================================

eval "$(starship init zsh)"
eval "$(zoxide init zsh)"
eval "$(mise activate zsh)"

source /usr/share/zsh/plugins/zsh-autosuggestions/zsh-autosuggestions.zsh
source /usr/share/zsh/plugins/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh
[[ -f /usr/share/fzf/completion.zsh ]] && source /usr/share/fzf/completion.zsh
[[ -f /usr/share/fzf/key-bindings.zsh ]] && source /usr/share/fzf/key-bindings.zsh
[ -s "$HOME/.bun/_bun" ] && source "$HOME/.bun/_bun"
. "$HOME/.cargo/env" 2>/dev/null

# Auto-activate venvs on cd
autoload -U add-zsh-hook
_auto_venv() {
  [[ -z "$VIRTUAL_ENV" && -d .venv ]] && source .venv/bin/activate
}
add-zsh-hook chpwd _auto_venv

# =============================================================================
# SHARED CONFIG (env vars, aliases, functions)
# =============================================================================

source ~/.shell-common.sh

# =============================================================================
# OMARCHY (sourced last to allow overrides above)
# =============================================================================

source ~/.local/share/omarchy/default/bash/aliases
source ~/.local/share/omarchy/default/bash/functions
source ~/.local/share/omarchy/default/bash/envs

# Kaiser CLI auto-venv
if [[ "$PWD" == "$HOME/dev/kaiser-cli"* ]] && [[ -f "$HOME/dev/kaiser-cli/.venv/bin/activate" ]]; then
  source "$HOME/dev/kaiser-cli/.venv/bin/activate"
fi
