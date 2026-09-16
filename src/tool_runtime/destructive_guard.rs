use super::ToolCall;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DestructiveGuardDecision {
    pub(crate) code: &'static str,
    pub(crate) reason: &'static str,
    pub(crate) recovery_action: &'static str,
}

const RECURSIVE_DELETE: DestructiveGuardDecision = DestructiveGuardDecision {
    code: "recursive_delete",
    reason: "raw recursive deletion is blocked; use the bounded delete_directory tool for one explicit project subtree",
    recovery_action: "use_delete_directory",
};

const BROAD_DELETE: DestructiveGuardDecision = DestructiveGuardDecision {
    code: "broad_delete",
    reason: "broad or wildcard deletion is blocked; use an exact typed file or directory operation",
    recovery_action: "use_typed_file_operation",
};

const DESTRUCTIVE_GIT: DestructiveGuardDecision = DestructiveGuardDecision {
    code: "destructive_git",
    reason: "broad destructive Git cleanup is blocked; use bounded typed cleanup or inspect the workspace first",
    recovery_action: "use_typed_git_cleanup",
};

const MIRROR_OR_PURGE: DestructiveGuardDecision = DestructiveGuardDecision {
    code: "mirror_or_purge",
    reason: "filesystem mirror or purge mode is blocked because it can delete destination content outside an explicit bounded delete target",
    recovery_action: "use_bounded_copy_or_typed_delete",
};

const DISK_DESTRUCTIVE: DestructiveGuardDecision = DestructiveGuardDecision {
    code: "disk_destructive",
    reason: "disk, volume, or partition destructive commands are blocked from Agent execution",
    recovery_action: "manual_terminal_required",
};

const OPAQUE_ENCODED_COMMAND: DestructiveGuardDecision = DestructiveGuardDecision {
    code: "opaque_encoded_command",
    reason: "opaque encoded PowerShell commands are blocked because their execution semantics are hidden from the pre-spawn mitigation guard",
    recovery_action: "use_visible_shell_or_script",
};

pub(crate) fn inspect_tool_call(call: &ToolCall) -> Option<DestructiveGuardDecision> {
    match call {
        ToolCall::RunShell { command, .. }
        | ToolCall::RunJob { command, .. }
        | ToolCall::SessionShellExec { command, .. } => inspect_command_text(command),
        ToolCall::RunScript { script, .. } => inspect_command_text(script),
        ToolCall::RunProcess {
            executable, args, ..
        }
        | ToolCall::RunDetachedProcess {
            executable, args, ..
        } => inspect_process(executable, args),
        _ => None,
    }
}

fn inspect_process(executable: &str, args: &[String]) -> Option<DestructiveGuardDecision> {
    let executable = executable_name(executable);
    match executable.as_str() {
        "powershell" | "pwsh" => inspect_powershell(args),
        "cmd" => inspect_wrapped_command(args, &["/c", "/k"]),
        "sh" | "bash" | "zsh" => inspect_wrapped_command(args, &["-c"]),
        "rm" | "del" | "erase" | "rd" | "rmdir" | "git" | "robocopy" | "format" | "diskpart"
        | "clear-disk" | "remove-partition" | "initialize-disk" | "format-volume" => {
            inspect_command_tokens(&executable, args)
        }
        _ => None,
    }
}

fn inspect_powershell(args: &[String]) -> Option<DestructiveGuardDecision> {
    if args.iter().any(|arg| powershell_encoded_command_flag(arg)) {
        return Some(OPAQUE_ENCODED_COMMAND);
    }
    inspect_wrapped_command(args, &["-command", "-c"])
}

fn powershell_encoded_command_flag(arg: &str) -> bool {
    matches!(
        arg.to_ascii_lowercase().as_str(),
        "-encodedcommand" | "-enc" | "-ec" | "-e"
    )
}

fn inspect_wrapped_command(
    args: &[String],
    command_flags: &[&str],
) -> Option<DestructiveGuardDecision> {
    let index = args.iter().position(|arg| {
        let arg = arg.to_ascii_lowercase();
        command_flags.iter().any(|flag| arg == *flag)
    })?;
    let command = args.get(index + 1..)?.join(" ");
    inspect_command_text(&command)
}

fn inspect_command_text(command: &str) -> Option<DestructiveGuardDecision> {
    for segment in command_segments(command) {
        let tokens = shell_words(&segment);
        if tokens.is_empty() {
            continue;
        }
        if let Some(decision) = inspect_segment_tokens(&tokens) {
            return Some(decision);
        }
    }
    None
}

fn inspect_segment_tokens(tokens: &[String]) -> Option<DestructiveGuardDecision> {
    let command_index = command_token_index(tokens)?;
    let command = executable_name(&tokens[command_index]);
    let args = &tokens[command_index + 1..];

    match command.as_str() {
        "powershell" | "pwsh" => inspect_powershell(args),
        "cmd" => inspect_wrapped_command(args, &["/c", "/k"]),
        "sh" | "bash" | "zsh" => inspect_wrapped_command(args, &["-c"]),
        _ => inspect_command_tokens(&command, args),
    }
}

fn inspect_command_tokens(command: &str, args: &[String]) -> Option<DestructiveGuardDecision> {
    match command {
        "rm" => inspect_rm(args),
        "remove-item" | "ri" => inspect_remove_item(args),
        "del" | "erase" => inspect_cmd_delete(args),
        "rd" | "rmdir" => inspect_cmd_rmdir(args),
        "git" => inspect_git(args),
        "robocopy" => args
            .iter()
            .any(|arg| matches!(arg.to_ascii_lowercase().as_str(), "/mir" | "/purge"))
            .then_some(MIRROR_OR_PURGE),
        "format" | "diskpart" | "clear-disk" | "remove-partition" | "initialize-disk"
        | "format-volume" => Some(DISK_DESTRUCTIVE),
        "find" if args.iter().any(|arg| arg.eq_ignore_ascii_case("-delete")) => Some(BROAD_DELETE),
        _ => None,
    }
}

fn inspect_rm(args: &[String]) -> Option<DestructiveGuardDecision> {
    if args.iter().any(|arg| unix_recursive_flag(arg)) {
        return Some(RECURSIVE_DELETE);
    }
    delete_targets(args, |arg| arg.starts_with('-'))
        .any(is_broad_delete_target)
        .then_some(BROAD_DELETE)
}

fn inspect_remove_item(args: &[String]) -> Option<DestructiveGuardDecision> {
    if args.iter().any(|arg| {
        let arg = arg.to_ascii_lowercase();
        arg == "-r" || arg.starts_with("-rec")
    }) {
        return Some(RECURSIVE_DELETE);
    }
    delete_targets(args, |arg| arg.starts_with('-'))
        .any(is_broad_delete_target)
        .then_some(BROAD_DELETE)
}

fn inspect_cmd_delete(args: &[String]) -> Option<DestructiveGuardDecision> {
    if args.iter().any(|arg| arg.eq_ignore_ascii_case("/s")) {
        return Some(RECURSIVE_DELETE);
    }
    delete_targets(args, |arg| arg.starts_with('/'))
        .any(is_broad_delete_target)
        .then_some(BROAD_DELETE)
}

fn inspect_cmd_rmdir(args: &[String]) -> Option<DestructiveGuardDecision> {
    args.iter()
        .any(|arg| arg.eq_ignore_ascii_case("/s"))
        .then_some(RECURSIVE_DELETE)
}

fn inspect_git(args: &[String]) -> Option<DestructiveGuardDecision> {
    let Some((subcommand_index, subcommand)) = args
        .iter()
        .enumerate()
        .find(|(_, arg)| !arg.starts_with('-'))
        .map(|(index, arg)| (index, arg.to_ascii_lowercase()))
    else {
        return None;
    };
    let tail = &args[subcommand_index + 1..];
    match subcommand.as_str() {
        "clean" => {
            if tail.iter().any(|arg| git_clean_is_dry_run(arg)) {
                None
            } else {
                Some(DESTRUCTIVE_GIT)
            }
        }
        "reset" if tail.iter().any(|arg| arg.eq_ignore_ascii_case("--hard")) => {
            Some(DESTRUCTIVE_GIT)
        }
        "restore" if git_has_broad_target(tail) => Some(DESTRUCTIVE_GIT),
        "checkout"
            if tail
                .iter()
                .any(|arg| matches!(arg.to_ascii_lowercase().as_str(), "-f" | "--force"))
                || git_checkout_has_broad_target(tail) =>
        {
            Some(DESTRUCTIVE_GIT)
        }
        "switch"
            if tail
                .iter()
                .any(|arg| arg.eq_ignore_ascii_case("--discard-changes")) =>
        {
            Some(DESTRUCTIVE_GIT)
        }
        _ => None,
    }
}

fn git_clean_is_dry_run(arg: &str) -> bool {
    let lower = arg.to_ascii_lowercase();
    lower == "--dry-run"
        || lower == "-n"
        || (lower.starts_with('-')
            && !lower.starts_with("--")
            && lower[1..].bytes().any(|byte| byte == b'n'))
}

fn git_has_broad_target(args: &[String]) -> bool {
    args.iter()
        .filter(|arg| !arg.starts_with('-') && arg.as_str() != "--")
        .any(|arg| is_broad_git_target(arg))
}

fn git_checkout_has_broad_target(args: &[String]) -> bool {
    let Some(separator) = args.iter().position(|arg| arg == "--") else {
        return false;
    };
    args[separator + 1..]
        .iter()
        .any(|arg| is_broad_git_target(arg))
}

fn is_broad_git_target(target: &str) -> bool {
    matches!(target.trim(), "." | "./" | ".\\" | ":/" | ":\\" | "*")
        || target.contains('*')
        || target.contains('?')
}

fn unix_recursive_flag(arg: &str) -> bool {
    let lower = arg.to_ascii_lowercase();
    if lower == "--recursive" {
        return true;
    }
    lower.starts_with('-')
        && !lower.starts_with("--")
        && lower[1..].bytes().any(|byte| byte == b'r')
}

fn delete_targets<'a>(
    args: &'a [String],
    is_option: impl Fn(&str) -> bool + 'a,
) -> impl Iterator<Item = &'a str> + 'a {
    args.iter()
        .map(String::as_str)
        .filter(move |arg| !is_option(arg))
}

fn is_broad_delete_target(target: &str) -> bool {
    let target = target.trim();
    matches!(
        target,
        "." | "./" | ".\\" | ".." | "../" | "..\\" | "/" | "\\"
    ) || target.contains('*')
        || target.contains('?')
        || is_drive_root(target)
}

fn is_drive_root(target: &str) -> bool {
    let bytes = target.as_bytes();
    bytes.len() == 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
}

fn command_token_index(tokens: &[String]) -> Option<usize> {
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index].trim_matches(|ch: char| matches!(ch, '(' | ')' | '{' | '}'));
        if token.is_empty() {
            index += 1;
            continue;
        }
        let lower = token.to_ascii_lowercase();
        if matches!(lower.as_str(), "sudo" | "command" | "call") || looks_like_env_assignment(token)
        {
            index += 1;
            continue;
        }
        return Some(index);
    }
    None
}

fn looks_like_env_assignment(token: &str) -> bool {
    let Some((name, _)) = token.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn executable_name(executable: &str) -> String {
    let basename = executable
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(executable)
        .trim_matches(|ch: char| matches!(ch, '"' | '\'' | '(' | ')' | '{' | '}'))
        .to_ascii_lowercase();
    for suffix in [".exe", ".cmd", ".bat", ".com"] {
        if let Some(stripped) = basename.strip_suffix(suffix) {
            return stripped.to_string();
        }
    }
    basename
}

fn command_segments(command: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for ch in command.chars() {
        match quote {
            Some(active) => {
                current.push(ch);
                if ch == active {
                    quote = None;
                }
            }
            None if matches!(ch, '\'' | '"') => {
                quote = Some(ch);
                current.push(ch);
            }
            None if matches!(ch, '\n' | '\r' | ';' | '|' | '&') => {
                if !current.trim().is_empty() {
                    segments.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
            }
            None => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        segments.push(current);
    }
    segments
}

fn shell_words(segment: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for ch in segment.chars() {
        match quote {
            Some(active) if ch == active => quote = None,
            Some(_) => current.push(ch),
            None if matches!(ch, '\'' | '"') => quote = Some(ch),
            None if ch.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            None => current.push(ch),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(command: &str) -> Option<&'static str> {
        inspect_command_text(command).map(|decision| decision.code)
    }

    #[test]
    fn recursive_delete_is_blocked_across_common_shells() {
        for command in [
            "rm -rf target",
            "rm --recursive target",
            "Remove-Item target -Recurse -Force",
            "rd /s /q target",
            "del /s /q *.tmp",
            "cd build && rm -r old",
        ] {
            assert_eq!(code(command), Some("recursive_delete"), "{command}");
        }
    }

    #[test]
    fn exact_single_file_delete_remains_allowed() {
        for command in [
            "rm old.log",
            "Remove-Item old.log",
            "del old.log",
            "erase build.log",
        ] {
            assert_eq!(code(command), None, "{command}");
        }
    }

    #[test]
    fn wildcard_and_root_delete_are_blocked() {
        for command in ["rm *.tmp", "del *.*", "Remove-Item *", "rm C:\\"] {
            assert_eq!(code(command), Some("broad_delete"), "{command}");
        }
    }

    #[test]
    fn destructive_git_is_blocked_but_read_only_or_narrow_git_is_allowed() {
        for command in [
            "git clean -fd",
            "git clean -fdx",
            "git reset --hard",
            "git restore .",
            "git checkout -- .",
            "git checkout -f main",
        ] {
            assert_eq!(code(command), Some("destructive_git"), "{command}");
        }
        for command in [
            "git status",
            "git diff",
            "git clean -nfd",
            "git restore src/lib.rs",
            "git checkout -- src/lib.rs",
        ] {
            assert_eq!(code(command), None, "{command}");
        }
    }

    #[test]
    fn mirror_purge_and_disk_commands_are_blocked() {
        assert_eq!(code("robocopy src dst /MIR"), Some("mirror_or_purge"));
        assert_eq!(code("robocopy src dst /PURGE"), Some("mirror_or_purge"));
        assert_eq!(code("format C:"), Some("disk_destructive"));
        assert_eq!(code("diskpart"), Some("disk_destructive"));
        assert_eq!(code("Clear-Disk -Number 1"), Some("disk_destructive"));
    }

    #[test]
    fn quoted_examples_are_not_mistaken_for_commands() {
        assert_eq!(code("echo 'rm -rf target'"), None);
        assert_eq!(code("Write-Output \"Remove-Item target -Recurse\""), None);
    }

    #[test]
    fn process_wrappers_are_inspected_without_blocking_ordinary_interpreters() {
        let args = vec![
            "-Command".to_string(),
            "Remove-Item target -Recurse".to_string(),
        ];
        assert_eq!(
            inspect_process("powershell.exe", &args).map(|decision| decision.code),
            Some("recursive_delete")
        );
        for flag in ["-EncodedCommand", "-enc", "-ec", "-e"] {
            let encoded = vec![
                flag.to_string(),
                "ZQBjAGgAbwAgAG8AcABhAHEAdQBlAA==".to_string(),
            ];
            assert_eq!(
                inspect_process("powershell.exe", &encoded).map(|decision| decision.code),
                Some("opaque_encoded_command"),
                "{flag}"
            );
            assert_eq!(
                code(&format!(
                    "powershell {flag} ZQBjAGgAbwAgAG8AcABhAHEAdQBlAA=="
                )),
                Some("opaque_encoded_command"),
                "{flag}"
            );
        }
        let python = vec!["script.py".to_string()];
        assert_eq!(inspect_process("python.exe", &python), None);
    }
}
