use crate::logging::{run_command_output, CommandLogOptions};
use crate::preferences::Preferences;
use std::process::{Command, Output};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PassImportRequest {
    pub(crate) store_root: String,
    pub(crate) source: String,
    pub(crate) source_path: Option<String>,
    pub(crate) target_path: Option<String>,
}

fn command_error(prefix: &str, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("{prefix}: {}", output.status)
    }
}

fn pass_command() -> Command {
    Preferences::new().command()
}

fn run_pass_command(context: &str, configure: impl FnOnce(&mut Command)) -> Result<Output, String> {
    let mut cmd = pass_command();
    configure(&mut cmd);
    run_command_output(&mut cmd, context, CommandLogOptions::DEFAULT)
        .map_err(|err| format!("Failed to run the host command: {err}"))
}

fn run_store_pass_command(
    store_root: &str,
    context: &str,
    configure: impl FnOnce(&mut Command),
) -> Result<Output, String> {
    let mut cmd = pass_command();
    cmd.env("PASSWORD_STORE_DIR", store_root);
    configure(&mut cmd);
    run_command_output(&mut cmd, context, CommandLogOptions::DEFAULT)
        .map_err(|err| format!("Failed to run the host command: {err}"))
}

fn strip_ansi_escape_sequences(text: &str) -> String {
    let mut clean = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            clean.push(ch);
            continue;
        }

        if matches!(chars.clone().next(), Some('[')) {
            let _ = chars.next();
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
        }
    }
    clean
}

fn parse_import_sources(output: &str) -> Vec<String> {
    strip_ansi_escape_sequences(output)
        .lines()
        .filter_map(|line| {
            let remainder = line.trim_start().strip_prefix('.')?;
            remainder
                .split_whitespace()
                .next()
                .map(str::trim)
                .filter(|source| !source.is_empty())
                .map(str::to_string)
        })
        .collect()
}

pub(crate) fn normalize_optional_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

pub(crate) fn available_pass_import_sources() -> Result<Vec<String>, String> {
    let output = run_pass_command("Read pass import sources", |cmd| {
        cmd.arg("import").arg("--list");
    })?;
    if !output.status.success() {
        return Err(command_error("pass import --list failed", &output));
    }

    let sources = parse_import_sources(&String::from_utf8_lossy(&output.stdout));
    if sources.is_empty() {
        Err("pass import is not available.".to_string())
    } else {
        Ok(sources)
    }
}

pub(crate) fn run_pass_import(request: &PassImportRequest) -> Result<(), String> {
    let output = run_store_pass_command(
        &request.store_root,
        "Import passwords with pass import",
        |cmd| {
            cmd.arg("import");
            if let Some(target_path) = &request.target_path {
                cmd.arg("--path").arg(target_path);
            }
            cmd.arg(&request.source);
            if let Some(source_path) = &request.source_path {
                cmd.arg(source_path);
            }
        },
    )?;

    if output.status.success() {
        Ok(())
    } else {
        Err(command_error("pass import failed", &output))
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_optional_text, parse_import_sources, strip_ansi_escape_sequences};

    #[test]
    fn ansi_sequences_are_removed_from_import_output() {
        let input = "\u{1b}[1m\u{1b}[92m (*) \u{1b}[0m\u{1b}[32mThe 62 supported password managers are:\u{1b}[0m";
        assert_eq!(
            strip_ansi_escape_sequences(input),
            " (*) The 62 supported password managers are:"
        );
    }

    #[test]
    fn import_sources_are_parsed_from_pass_import_list_output() {
        let output = "\u{1b}[1m  .  \u{1b}[0m\u{1b}[1mbitwarden       \u{1b}[0mcsv, json\n  .  keepassxc       kdbx, csv\n";
        assert_eq!(
            parse_import_sources(output),
            vec!["bitwarden".to_string(), "keepassxc".to_string()]
        );
    }

    #[test]
    fn optional_import_fields_ignore_blank_text() {
        assert_eq!(normalize_optional_text(""), None);
        assert_eq!(normalize_optional_text("   "), None);
        assert_eq!(
            normalize_optional_text(" folder/import "),
            Some("folder/import".to_string())
        );
    }
}
