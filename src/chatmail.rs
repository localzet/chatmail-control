#[derive(Debug, Clone, Copy)]
pub struct LogSource {
    pub name: &'static str,
    pub unit: &'static str,
}

pub const COMMAND_TIMEOUT_SECONDS: u64 = 10;

pub const LOG_SOURCES: &[LogSource] = &[
    LogSource {
        name: "dovecot",
        unit: "dovecot",
    },
    LogSource {
        name: "postfix",
        unit: "postfix",
    },
    LogSource {
        name: "doveauth",
        unit: "doveauth",
    },
    LogSource {
        name: "chatmail-metadata",
        unit: "chatmail-metadata",
    },
    LogSource {
        name: "chatmail-expire",
        unit: "chatmail-expire",
    },
    LogSource {
        name: "lastlogin",
        unit: "lastlogin",
    },
];

pub fn users_list_command() -> Vec<String> {
    vec!["doveadm".into(), "user".into(), "*".into()]
}

pub fn user_size_command(address: &str) -> Vec<String> {
    vec![
        "doveadm".into(),
        "quota".into(),
        "get".into(),
        "-u".into(),
        address.into(),
    ]
}

pub fn user_message_count_command(address: &str) -> Vec<String> {
    vec![
        "doveadm".into(),
        "mailbox".into(),
        "status".into(),
        "-u".into(),
        address.into(),
        "messages".into(),
        "INBOX".into(),
    ]
}

pub fn user_metadata_command(address: &str) -> Vec<String> {
    vec![
        "doveadm".into(),
        "user".into(),
        "-u".into(),
        address.into(),
        "*".into(),
    ]
}

pub fn user_delete_mailbox_command(address: &str) -> Vec<String> {
    vec![
        "doveadm".into(),
        "mailbox".into(),
        "delete".into(),
        "-u".into(),
        address.into(),
        "-s".into(),
        "INBOX".into(),
    ]
}

pub fn bans_reload_commands() -> Vec<Vec<String>> {
    vec![
        vec!["systemctl".into(), "reload".into(), "postfix".into()],
        vec!["systemctl".into(), "reload".into(), "dovecot".into()],
    ]
}

pub fn settings_reload_commands() -> Vec<Vec<String>> {
    vec![vec!["systemctl".into(), "reload".into(), "doveauth".into()]]
}

pub fn log_source_by_name(name: Option<&str>) -> LogSource {
    if let Some(name) = name {
        if let Some(source) = LOG_SOURCES.iter().find(|source| source.name == name) {
            return *source;
        }
    }
    LOG_SOURCES[0]
}

pub fn log_source_command(source: LogSource, limit: usize) -> Vec<String> {
    vec![
        "journalctl".into(),
        "-u".into(),
        source.unit.into(),
        "-n".into(),
        limit.to_string(),
        "--no-pager".into(),
    ]
}
