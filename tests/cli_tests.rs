use std::fs;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn run_cli(args: &[&str]) -> (Output, std::path::PathBuf) {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("spindle-cli-test-{}-{stamp}", std::process::id()));
    let work_dir = root.join("work");
    let local_app_data = root.join("local-app-data");
    fs::create_dir_all(&work_dir).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_spindle"))
        .args(args)
        .current_dir(&work_dir)
        .env("LOCALAPPDATA", &local_app_data)
        .output()
        .unwrap();
    (output, root)
}

#[test]
fn version_flags_print_the_package_version_without_project_setup() {
    for flag in ["--version", "-V"] {
        let (output, root) = run_cli(&[flag]);

        assert!(output.status.success(), "{flag} should succeed");
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("spindle {}\n", env!("CARGO_PKG_VERSION"))
        );
        assert!(!root.join("local-app-data").exists());
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn help_forms_print_usage_without_project_setup() {
    for flag in ["help", "--help", "-h"] {
        let (output, root) = run_cli(&[flag]);

        assert!(output.status.success(), "{flag} should succeed");
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains("Usage: spindle"));
        assert!(help.contains("--version"));
        assert!(help.contains("report-agent-session"));
        assert!(!root.join("local-app-data").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
