//! Integration tests for text2svg CLI agent-friendliness.
//!
//! These tests verify observable CLI behavior: stdout/stderr separation,
//! exit codes, stdin/stdout piping, and help output.

use std::process::Command;

fn text2svg_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_text2svg"))
}

fn test_font() -> &'static str {
    "Arial"
}

fn tmp_svg(name: &str) -> String {
    format!("/tmp/text2svg_test_{}.svg", name)
}

// ============================================================
// Task 1: stdout/stderr discipline
// ============================================================

#[test]
fn list_fonts_stdout_has_no_decorative_header() {
    let output = text2svg_cmd()
        .arg("--list-fonts")
        .output()
        .expect("failed to run text2svg");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("Installed Font Families:"),
        "stdout should not contain decorative header, got:\n{}",
        &stdout[..stdout.len().min(200)]
    );
    for line in stdout.lines() {
        assert!(
            !line.starts_with("- "),
            "font name should not have '- ' prefix, got: {:?}",
            line
        );
    }
    assert!(output.status.success());
}

#[test]
fn list_themes_stdout_has_no_decorative_header() {
    let output = text2svg_cmd()
        .arg("--list-theme")
        .output()
        .expect("failed to run text2svg");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("Available Themes:"),
        "stdout should not contain decorative header, got:\n{}",
        &stdout[..stdout.len().min(200)]
    );
    for line in stdout.lines() {
        assert!(
            !line.starts_with("- "),
            "theme name should not have '- ' prefix, got: {:?}",
            line
        );
    }
    assert!(output.status.success());
}

#[test]
fn list_syntax_stdout_has_no_decorative_header() {
    let output = text2svg_cmd()
        .arg("--list-syntax")
        .output()
        .expect("failed to run text2svg");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("Supported Syntaxes"),
        "stdout should not contain decorative header, got:\n{}",
        &stdout[..stdout.len().min(200)]
    );
    assert!(output.status.success());
}

#[test]
fn render_text_stdout_is_empty() {
    let out = tmp_svg("render_empty_stdout");
    let output = text2svg_cmd()
        .args(["Hello", "--font", test_font(), "--output", &out])
        .output()
        .expect("failed to run text2svg");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.is_empty(),
        "stdout should be empty during file rendering, got:\n{}",
        stdout
    );
    assert!(output.status.success());
    let _ = std::fs::remove_file(&out);
}

#[test]
fn debug_output_goes_to_stderr_not_stdout() {
    let out = tmp_svg("debug_stderr");
    let output = text2svg_cmd()
        .args(["Hello", "--font", test_font(), "--debug", "--output", &out])
        .output()
        .expect("failed to run text2svg");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stdout.is_empty(),
        "stdout should be empty even in debug mode, got:\n{}",
        stdout
    );
    assert!(
        stderr.contains("Debug"),
        "debug output should appear on stderr, stderr was:\n{}",
        &stderr[..stderr.len().min(500)]
    );
    assert!(output.status.success());
    let _ = std::fs::remove_file(&out);
}

// ============================================================
// Task 2/3: --output - (stdout SVG)
// ============================================================

#[test]
fn output_dash_writes_svg_to_stdout() {
    let output = text2svg_cmd()
        .args(["Hello", "--font", test_font(), "--output", "-"])
        .output()
        .expect("failed to run text2svg");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("<svg") && stdout.contains("</svg>"),
        "stdout should contain SVG when --output is -, got:\n{}",
        &stdout[..stdout.len().min(300)]
    );
    assert!(output.status.success());
}

// ============================================================
// Task 4: stdin support
// ============================================================

#[test]
fn stdin_pipe_as_text_input() {
    let out = tmp_svg("stdin_pipe");
    let output = text2svg_cmd()
        .args(["--font", test_font(), "--output", &out])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(b"Hello from stdin")
                .unwrap();
            child.wait_with_output()
        })
        .expect("failed to run text2svg");

    assert!(
        output.status.success(),
        "should succeed with piped stdin, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        std::path::Path::new(&out).exists(),
        "output SVG file should be created from stdin input"
    );
    let _ = std::fs::remove_file(&out);
}

// ============================================================
// Task 5: actionable errors + exit codes
// ============================================================

#[test]
fn missing_font_error_suggests_list_fonts() {
    let output = text2svg_cmd()
        .args(["Hello", "--font", "NonExistentFontXYZ123"])
        .output()
        .expect("failed to run text2svg");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        stderr.contains("--list-fonts"),
        "error should suggest --list-fonts, got:\n{}",
        stderr
    );
}

#[test]
fn missing_font_flag_error_suggests_list_fonts() {
    let out = tmp_svg("no_font_flag");
    let output = text2svg_cmd()
        .args(["Hello", "--output", &out])
        .output()
        .expect("failed to run text2svg");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        stderr.contains("--list-fonts"),
        "error for missing --font should suggest --list-fonts, got:\n{}",
        stderr
    );
}

#[test]
fn bad_theme_fails_instead_of_silent_fallback() {
    let out = tmp_svg("bad_theme");
    let output = text2svg_cmd()
        .args([
            "--file",
            "Cargo.toml",
            "--font",
            test_font(),
            "--highlight",
            "--theme",
            "nonexistent-theme-xyz",
            "--output",
            &out,
        ])
        .output()
        .expect("failed to run text2svg");

    assert!(
        !output.status.success(),
        "should fail on nonexistent theme instead of silent fallback"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--list-theme"),
        "error should suggest --list-theme, got:\n{}",
        stderr
    );
    let _ = std::fs::remove_file(&out);
}

#[test]
fn file_not_found_has_distinct_exit_code() {
    let output = text2svg_cmd()
        .args([
            "--file",
            "/tmp/nonexistent_file_xyz.txt",
            "--font",
            test_font(),
        ])
        .output()
        .expect("failed to run text2svg");

    assert!(!output.status.success());
    // Exit code should be 3 (IO error), not 1 (generic)
    let code = output.status.code().unwrap();
    assert_eq!(
        code, 3,
        "file not found should exit with code 3, got {}",
        code
    );
}

#[test]
fn font_not_found_has_distinct_exit_code() {
    let output = text2svg_cmd()
        .args(["Hello", "--font", "NonExistentFontXYZ123"])
        .output()
        .expect("failed to run text2svg");

    assert!(!output.status.success());
    let code = output.status.code().unwrap();
    assert_eq!(code, 2, "font error should exit with code 2, got {}", code);
}

// ============================================================
// Task 6: help examples
// ============================================================

#[test]
fn help_contains_examples() {
    let output = text2svg_cmd()
        .arg("--help")
        .output()
        .expect("failed to run text2svg");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Examples:")
            || stdout.contains("EXAMPLES:")
            || stdout.contains("examples:"),
        "--help should contain usage examples section, got:\n{}",
        &stdout[..stdout.len().min(500)]
    );
    // Should contain at least one concrete invocation
    assert!(
        stdout.contains("text2svg"),
        "--help examples should contain concrete command invocations"
    );
}
