// Copyright 2024 The Jujutsu Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use clap::FromArgMatches as _;
use clap_complete::CompletionCandidate;
use config::Config;
use jj_lib::workspace::DefaultWorkspaceLoaderFactory;
use jj_lib::workspace::WorkspaceLoaderFactory as _;

use crate::cli_util::expand_args;
use crate::cli_util::find_workspace_dir;
use crate::cli_util::GlobalArgs;
use crate::command_error::internal_error;
use crate::command_error::user_error;
use crate::command_error::CommandError;
use crate::config::default_config;
use crate::config::LayeredConfigs;
use crate::ui::Ui;

pub fn local_bookmarks() -> Vec<CompletionCandidate> {
    with_jj(|mut jj, _| {
        jj.arg("bookmark")
            .arg("list")
            .arg("--template")
            .arg(r#"if(!remote, name ++ "\n")"#)
            .output()
            .map(|output| output.stdout)
            .map_err(user_error)
            .and_then(|stdout| String::from_utf8(stdout).map_err(internal_error))
            .map(|stdout| stdout.lines().map(CompletionCandidate::new).collect())
    })
}

pub fn aliases() -> Vec<CompletionCandidate> {
    with_jj(|_, config| {
        Ok(config
            .get_table("aliases")?
            .into_keys()
            // This is opinionated, but many people probably have several
            // single- or two-letter aliases they use all the time. These
            // aliases don't need to be completed and they would only clutter
            // the output of `jj <TAB>`.
            .filter(|alias| alias.len() > 2)
            .map(CompletionCandidate::new)
            .collect())
    })
}

/// Shell out to jj during dynamic completion generation
///
/// In case of errors, print them and early return an empty vector.
fn with_jj<F>(completion_fn: F) -> Vec<CompletionCandidate>
where
    F: FnOnce(std::process::Command, Config) -> Result<Vec<CompletionCandidate>, CommandError>,
{
    get_jj_command()
        .and_then(|(jj, config)| completion_fn(jj, config))
        .unwrap_or_else(|e| {
            eprintln!("{}", e.error);
            Vec::new()
        })
}

/// Shell out to jj during dynamic completion generation
///
/// This is necessary because dynamic completion code needs to be aware of
/// global configuration like custom storage backends. Dynamic completion
/// code via clap_complete doesn't accept arguments, so they cannot be passed
/// that way. Another solution would've been to use global mutable state, to
/// give completion code access to custom backends. Shelling out was chosen as
/// the preferred method, because it's more maintainable and the performance
/// requirements of completions aren't very high.
fn get_jj_command() -> Result<(std::process::Command, Config), CommandError> {
    let current_exe = std::env::current_exe().map_err(user_error)?;
    let mut command = std::process::Command::new(current_exe);

    // Snapshotting could make completions much slower in some situations
    // and be undesired by the user.
    command.arg("--ignore-working-copy");

    // Parse some of the global args we care about for passing along to the
    // child process. This shouldn't fail, since none of the global args are
    // required.
    let app = crate::commands::default_app();
    let config = config::Config::builder()
        .add_source(default_config())
        .build()
        .expect("default config should be valid");
    let mut layered_configs = LayeredConfigs::from_environment(config);
    let ui = Ui::with_config(&layered_configs.merge()).expect("default config should be valid");
    let cwd = std::env::current_dir()
        .and_then(|cwd| cwd.canonicalize())
        .map_err(user_error)?;
    let maybe_cwd_workspace_loader = DefaultWorkspaceLoaderFactory.create(find_workspace_dir(&cwd));
    layered_configs.read_user_config().map_err(user_error)?;
    if let Ok(loader) = &maybe_cwd_workspace_loader {
        layered_configs
            .read_repo_config(loader.repo_path())
            .map_err(user_error)?;
    }
    let config = layered_configs.merge();
    // skip 2 because of the clap_complete prelude: jj -- jj <actual args...>
    let args = std::env::args_os().skip(2);
    let args = expand_args(&ui, &app, args, &config)?;
    let args = app
        .clone()
        .disable_version_flag(true)
        .disable_help_flag(true)
        .ignore_errors(true)
        // Here, allow_external_subcommands fixes a weird issue. Without it,
        // parsing GlobalArgs will fail with the message that a required arg
        // is missing, where the required arg is a boolean flag. This seems
        // unexpected, because missing boolean flags are usually treated as
        // false. It is also not clear to me why allow_external_subcommands
        // changes this behavior. See the discussion in the clap repo:
        // https://github.com/clap-rs/clap/discussions/5812
        .allow_external_subcommands(true)
        .try_get_matches_from(args)?;
    let args: GlobalArgs = GlobalArgs::from_arg_matches(&args)?;

    if let Some(repository) = args.repository {
        // TODO: load repo config here so repo aliases are also completed
        command.arg("--repository");
        command.arg(repository);
    }
    if let Some(at_operation) = args.at_operation {
        command.arg("--at-operation");
        command.arg(at_operation);
    }
    for config_toml in args.early_args.config_toml {
        command.arg("--config-toml");
        command.arg(config_toml);
    }

    Ok((command, config))
}
