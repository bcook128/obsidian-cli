use std::path::PathBuf;

use anyhow::{bail, Context};
use clap::Args;

use crate::tui;
use crate::util::{get_current_vault, should_enable_interactivity};

#[derive(Args, Debug, Clone)]
pub struct BrowseCommand {
    /// Override the active vault with a specific vault name
    #[arg(long, short = 'v')]
    vault: Option<String>,
}

pub fn entry(cmd: &BrowseCommand) -> anyhow::Result<Option<String>> {
    if !should_enable_interactivity() {
        bail!("browse requires an interactive terminal");
    }

    let vault = get_current_vault(cmd.vault.clone())?;
    let vault_path = PathBuf::from(vault.path);

    tui::run(vault_path).context("failed to launch interactive browser")?;

    Ok(None)
}
