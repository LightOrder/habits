const ZSH_INIT: &str = r#"# habits Ctrl-R history selector
habits-history-widget() {
  local original_buffer="$BUFFER"
  local selected select_status
  selected="$(habits select --query "$BUFFER")"
  select_status=$?

  if (( select_status == 0 )); then
    if [[ "$selected" != "$original_buffer" ]]; then
      BUFFER="$selected"
      CURSOR=${#BUFFER}
    fi
    zle accept-line
  else
    zle redisplay
  fi
}
zle -N habits-history-widget
bindkey '^R' habits-history-widget
"#;

pub fn zsh_init() -> &'static str {
    ZSH_INIT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zsh_widget_quotes_buffer_and_assigns_confirmed_selection_as_data() {
        let script = zsh_init();
        assert!(script.contains(r#"--query "$BUFFER""#));
        assert!(script.contains(r#"BUFFER="$selected""#));
        assert!(script.contains("select_status == 0"));
        assert!(script.contains(r#""$selected" != "$original_buffer""#));
        assert!(!script.contains("eval "));
        assert!(!script.contains("source "));
        assert!(!script.contains(".zshrc"));
        assert!(script.contains("zle accept-line"));
    }

    #[test]
    fn zsh_widget_only_defines_and_binds_a_zle_widget() {
        let script = zsh_init();
        assert!(script.contains("zle -N habits-history-widget"));
        assert!(script.contains("bindkey '^R' habits-history-widget"));
        assert!(!script.contains(">>"));
        assert!(!script.contains("tee "));
    }
}
