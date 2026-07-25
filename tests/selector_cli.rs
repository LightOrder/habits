use std::process::Command;

fn habits() -> Command {
    Command::new(env!("CARGO_BIN_EXE_habits"))
}

#[test]
fn shell_init_zsh_emits_sourceable_widget_without_writing_configuration() {
    let fixture =
        std::env::temp_dir().join(format!("habits-shell-init-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&fixture);
    std::fs::create_dir_all(&fixture).unwrap();
    let zshrc = fixture.join(".zshrc");
    std::fs::write(&zshrc, "# unchanged\n").unwrap();

    let output = habits()
        .env("HOME", &fixture)
        .args(["shell-init", "zsh"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let script = String::from_utf8(output.stdout).unwrap();
    assert!(script.contains(r#"habits select --query "$BUFFER""#));
    assert!(script.contains("select_status == 0"));
    assert!(script.contains(r#"BUFFER="$selected""#));
    assert!(script.contains("zle accept-line"));
    assert!(!script.contains("eval "));
    assert_eq!(std::fs::read_to_string(&zshrc).unwrap(), "# unchanged\n");
    std::fs::remove_dir_all(fixture).unwrap();
}

#[test]
fn noninteractive_select_fails_without_emitting_history_content() {
    let fixture =
        std::env::temp_dir().join(format!("habits-select-nontty-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&fixture);
    std::fs::create_dir_all(&fixture).unwrap();
    let secret = "HABITS_SELECT_PRIVATE_SENTINEL_47cc";
    std::fs::write(fixture.join(".zsh_history"), format!("{secret}\n")).unwrap();

    let output = habits()
        .env("HOME", &fixture)
        .args(["select", "--query", "HABITS"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).contains(secret));
    assert!(String::from_utf8_lossy(&output.stderr).contains("interactive terminal"));
    std::fs::remove_dir_all(fixture).unwrap();
}

#[test]
#[ignore = "requires a real interactive TTY and human Enter/Escape input"]
fn interactive_tty_smoke() {
    // Run explicitly from a terminal with:
    // cargo test --test selector_cli interactive_tty_smoke -- --ignored --nocapture
    let status = Command::new(env!("CARGO_BIN_EXE_habits"))
        .args(["select", "--query", "echo"])
        .status()
        .unwrap();
    assert!(status.success() || status.code() == Some(1));
}
