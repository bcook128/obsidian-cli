use assert_cmd::prelude::*;
use assert_fs::fixture::PathChild;
use serde_yaml::Value;
use std::fs;

mod utils;
use utils::Obx;

mod config {
    use super::*;

    #[test]
    fn set_editor_updates_config() {
        let mut cmd = Obx::from_command("config set --editor /usr/bin/nvim");
        let config_file = cmd.temp_dir.child("./config/obx/config.yml");

        cmd.cmd.assert().success().stdout("Configuration updated\n");

        let contents = fs::read_to_string(config_file.path()).unwrap();
        let value: Value = serde_yaml::from_str(&contents).unwrap();

        assert_eq!(
            value.get("editor").and_then(Value::as_str),
            Some("/usr/bin/nvim"),
            "expected editor to be set in config",
        );
    }

    #[test]
    fn set_theme_updates_config() {
        let mut cmd = Obx::from_command("config set --theme gruvbox-dark");
        let config_file = cmd.temp_dir.child("./config/obx/config.yml");

        cmd.cmd.assert().success().stdout("Configuration updated\n");

        let contents = fs::read_to_string(config_file.path()).unwrap();
        let value: Value = serde_yaml::from_str(&contents).unwrap();

        assert_eq!(
            value.get("theme").and_then(Value::as_str),
            Some("gruvbox-dark"),
            "expected theme to be persisted",
        );
    }

    #[test]
    fn clear_editor_removes_setting() {
        let mut set_cmd = Obx::from_command("config set --editor nvim");
        let config_dir = set_cmd.temp_dir.child("./config/obx/");
        let config_file = config_dir.child("config.yml");

        set_cmd
            .cmd
            .assert()
            .success()
            .stdout("Configuration updated\n");

        let mut clear_cmd = Obx::from_command("config set --clear-editor");
        clear_cmd
            .env("OBX_CONFIG_DIR", config_dir.display().to_string())
            .cmd
            .assert()
            .success()
            .stdout("Configuration updated\n");

        let contents = fs::read_to_string(config_file.path()).unwrap();
        let value: Value = serde_yaml::from_str(&contents).unwrap();

        assert!(value.get("editor").is_none());
    }

    #[test]
    fn set_without_changes_is_noop() {
        Obx::from_command("config set")
            .cmd
            .assert()
            .success()
            .stdout("Nothing to update\n");
    }
}
