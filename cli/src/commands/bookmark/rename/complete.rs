use clap_complete::CompletionCandidate;

use crate::cli_util::dyn_completion_state;

pub fn branch(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let Some(current) = current.to_str() else {
        return Vec::new();
    };

    let mut ui = dyn_completion_state::UI.take().unwrap();
    let command = dyn_completion_state::COMMAND_HELPER.take().unwrap();
    let ui = &mut ui;

    let workspace_command = command.workspace_helper(ui).unwrap();
    let repo = workspace_command.repo();
    let view = repo.view();

    view.bookmarks()
        .map(|(local, _)| local)
        .filter(|branch| branch.starts_with(current))
        .map(CompletionCandidate::new)
        .collect()
}
