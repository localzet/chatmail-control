#[derive(Debug, Clone, Copy)]
pub struct LogSource {
    pub name: &'static str,
    pub unit: &'static str,
    pub identifiers: &'static [&'static str],
}

pub const COMMAND_TIMEOUT_SECONDS: u64 = 10;

pub const LOG_SOURCES: &[LogSource] = &[
    LogSource {
        name: "dovecot",
        unit: "dovecot",
        identifiers: &[],
    },
    LogSource {
        name: "postfix",
        unit: "postfix",
        identifiers: &[
            "postfix/anvil",
            "postfix/cleanup",
            "postfix/lmtp",
            "postfix/lmtp-filtermail",
            "postfix/local",
            "postfix/master",
            "postfix/pickup",
            "postfix/postscreen",
            "postfix/qmgr",
            "postfix/reinject",
            "postfix/reinject_incoming",
            "postfix/scache",
            "postfix/smtp",
            "postfix/smtpd",
            "postfix/smtps",
            "postfix/submission",
            "postfix/tlsmgr",
            "postfix/trivial-rewrite",
        ],
    },
    LogSource {
        name: "filtermail",
        unit: "filtermail",
        identifiers: &[],
    },
    LogSource {
        name: "filtermail-incoming",
        unit: "filtermail-incoming",
        identifiers: &[],
    },
    LogSource {
        name: "filtermail-transport",
        unit: "filtermail-transport",
        identifiers: &[],
    },
    LogSource {
        name: "doveauth",
        unit: "doveauth",
        identifiers: &[],
    },
    LogSource {
        name: "chatmail-metadata",
        unit: "chatmail-metadata",
        identifiers: &[],
    },
    LogSource {
        name: "chatmail-expire",
        unit: "chatmail-expire",
        identifiers: &[],
    },
    LogSource {
        name: "lastlogin",
        unit: "lastlogin",
        identifiers: &[],
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

pub fn user_mailbox_create_command(address: &str, mailbox: &str) -> Vec<String> {
    vec![
        "doveadm".into(),
        "mailbox".into(),
        "create".into(),
        "-u".into(),
        address.into(),
        mailbox.into(),
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

pub fn user_kick_command(address: &str) -> Vec<String> {
    vec!["doveadm".into(), "kick".into(), "-u".into(), address.into()]
}

pub fn user_home_command(address: &str) -> Vec<String> {
    vec![
        "doveadm".into(),
        "user".into(),
        "-u".into(),
        address.into(),
        "-f".into(),
        "home".into(),
    ]
}

pub fn user_auth_test_command(address: &str, password: &str) -> Vec<String> {
    vec![
        "doveadm".into(),
        "auth".into(),
        "test".into(),
        "-x".into(),
        "service=imap".into(),
        address.into(),
        password.into(),
    ]
}

pub fn user_mailbox_list_command(address: &str) -> Vec<String> {
    vec![
        "doveadm".into(),
        "mailbox".into(),
        "list".into(),
        "-u".into(),
        address.into(),
    ]
}

pub fn user_mailbox_expunge_command(address: &str, mailbox: &str) -> Vec<String> {
    vec![
        "doveadm".into(),
        "expunge".into(),
        "-u".into(),
        address.into(),
        "mailbox".into(),
        mailbox.into(),
        "all".into(),
    ]
}

pub fn user_quota_recalc_command(address: &str) -> Vec<String> {
    vec![
        "doveadm".into(),
        "quota".into(),
        "recalc".into(),
        "-u".into(),
        address.into(),
    ]
}

pub fn user_force_resync_command(address: &str) -> Vec<String> {
    vec![
        "doveadm".into(),
        "force-resync".into(),
        "-u".into(),
        address.into(),
        "*".into(),
    ]
}

pub fn password_hash_command(password: &str) -> Vec<String> {
    vec![
        "doveadm".into(),
        "pw".into(),
        "-s".into(),
        "SHA512-CRYPT".into(),
        "-p".into(),
        password.into(),
    ]
}

pub fn bans_reload_commands() -> Vec<Vec<String>> {
    vec![
        vec!["systemctl".into(), "reload".into(), "postfix".into()],
        vec!["systemctl".into(), "reload".into(), "dovecot".into()],
    ]
}

pub fn systemctl_command(action: &str, unit: &str) -> Vec<String> {
    vec!["systemctl".into(), action.into(), unit.into()]
}

pub fn log_source_by_name(name: Option<&str>) -> LogSource {
    if let Some(name) = name {
        if let Some(source) = LOG_SOURCES.iter().find(|source| source.name == name) {
            return *source;
        }
    }
    LOG_SOURCES[0]
}
