use crate::{cli_config, theme::ThemeName, util::CommandResult};
use anyhow::Context;
use clap::{Args, Subcommand};

#[derive(Args, Debug, Clone)]
#[command(args_conflicts_with_subcommands = true)]
#[command(arg_required_else_help = true)]
pub struct ConfigCommand {
    #[command(subcommand)]
    command: Option<Subcommands>,
}

#[derive(Debug, Subcommand, Clone)]
enum Subcommands {
    /// Print the current configuration
    Print(PrintArgs),

    /// Print the absolute path to your config file
    Path,

    /// Update editor or theme preferences
    Set(SetArgs),
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum PrintFormats {
    Yaml,
    Json,
}

#[derive(Args, Debug, Clone)]
struct PrintArgs {
    #[arg(long, short = 'f', default_value = "yaml")]
    format: PrintFormats,
}

pub fn entry(cmd: &ConfigCommand) -> anyhow::Result<Option<String>> {
    match &cmd.command {
        Some(Subcommands::Print(PrintArgs { format })) => print(format),
        Some(Subcommands::Path) => path(),
        Some(Subcommands::Set(args)) => set(args),
        None => todo!(),
    }
}

fn print(format: &PrintFormats) -> CommandResult {
    let config = cli_config::read()?;

    let res = match format {
        PrintFormats::Yaml => serde_yaml::to_string(&config)?,
        PrintFormats::Json => serde_json::to_string(&config)?,
    };

    Ok(Some(res))
}

fn path() -> CommandResult {
    let config_path = cli_config::get_config_path()
        .to_str()
        .context("failed to stringify config path")?
        .to_string();

    Ok(Some(config_path))
}

#[derive(Args, Debug, Clone)]
struct SetArgs {
    #[arg(long)]
    editor: Option<String>,
    #[arg(long, value_enum)]
    theme: Option<ThemeName>,
    #[arg(long, conflicts_with = "editor")]
    clear_editor: bool,
}

fn set(args: &SetArgs) -> CommandResult {
    if args.editor.is_none() && args.theme.is_none() && !args.clear_editor {
        return Ok(Some("Nothing to update".to_string()));
    }

    let mut config = cli_config::read()?;

    if args.clear_editor {
        config.editor = None;
    } else if let Some(editor) = &args.editor {
        config.editor = Some(editor.clone());
    }

    if let Some(theme) = args.theme {
        config.theme = theme;
    }

    cli_config::write(&config)?;

    Ok(Some("Configuration updated".to_string()))
}
