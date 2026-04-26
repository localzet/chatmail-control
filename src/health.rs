use std::net::{IpAddr, SocketAddr};

use hickory_resolver::{config::*, TokioAsyncResolver};
use serde::Serialize;

use crate::{config::Config, shell::Shell};

#[derive(Debug, Clone, Serialize)]
pub struct HealthCheck {
    pub name: String,
    pub status: String,
    pub details: String,
}

pub async fn run_health_checks(shell: &Shell, config: &Config) -> Vec<HealthCheck> {
    let mut checks = Vec::new();

    for service in &config.health.services {
        let output = shell
            .run(&["systemctl".into(), "is-active".into(), service.to_string()])
            .await;
        checks.push(match output {
            Ok(output) if output.stdout.trim() == "active" => HealthCheck {
                name: format!("service:{service}"),
                status: "ok".into(),
                details: output.stdout,
            },
            Ok(output) => HealthCheck {
                name: format!("service:{service}"),
                status: "warn".into(),
                details: if output.stdout.is_empty() {
                    output.stderr
                } else {
                    output.stdout
                },
            },
            Err(err) => HealthCheck {
                name: format!("service:{service}"),
                status: "error".into(),
                details: err.to_string(),
            },
        });
    }

    for port in &config.health.ports {
        let result =
            tokio::net::TcpStream::connect(SocketAddr::new(IpAddr::from([127, 0, 0, 1]), *port))
                .await;
        checks.push(match result {
            Ok(_) => HealthCheck {
                name: format!("port:{port}"),
                status: "ok".into(),
                details: "listening".into(),
            },
            Err(err) => HealthCheck {
                name: format!("port:{port}"),
                status: "warn".into(),
                details: err.to_string(),
            },
        });
    }

    let resolver = TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default());
    let mx = resolver.mx_lookup(config.health.domain.clone()).await;
    checks.push(match mx {
        Ok(records) if records.iter().next().is_some() => HealthCheck {
            name: "dns:mx".into(),
            status: "ok".into(),
            details: format!("{records:?}"),
        },
        Ok(_) => HealthCheck {
            name: "dns:mx".into(),
            status: "warn".into(),
            details: "no MX records".into(),
        },
        Err(err) => HealthCheck {
            name: "dns:mx".into(),
            status: "error".into(),
            details: err.to_string(),
        },
    });

    checks.push(txt_record_check(&resolver, &config.health.domain, "v=spf1", "dns:spf").await);
    checks.push(
        txt_record_check(
            &resolver,
            &format!("_dmarc.{}", config.health.domain),
            "v=DMARC1",
            "dns:dmarc",
        )
        .await,
    );
    checks.push(
        txt_record_check(
            &resolver,
            &format!(
                "{}._domainkey.{}",
                config.health.dkim_selector, config.health.domain
            ),
            "v=DKIM1",
            "dns:dkim",
        )
        .await,
    );

    let cert_output = shell
        .run(&[
            "openssl".into(),
            "s_client".into(),
            "-connect".into(),
            format!("{}:443", config.health.domain),
            "-servername".into(),
            config.health.domain.clone(),
            "-brief".into(),
        ])
        .await;
    checks.push(match cert_output {
        Ok(output) if output.status == 0 => HealthCheck {
            name: "tls:cert".into(),
            status: "ok".into(),
            details: output.stderr,
        },
        Ok(output) => HealthCheck {
            name: "tls:cert".into(),
            status: "warn".into(),
            details: output.stderr,
        },
        Err(err) => HealthCheck {
            name: "tls:cert".into(),
            status: "warn".into(),
            details: err.to_string(),
        },
    });

    let queue_output = shell.run(&["postqueue".into(), "-p".into()]).await;
    checks.push(match queue_output {
        Ok(output) => HealthCheck {
            name: "queue".into(),
            status: "ok".into(),
            details: output.stdout,
        },
        Err(err) => HealthCheck {
            name: "queue".into(),
            status: "warn".into(),
            details: err.to_string(),
        },
    });

    let disk_output = shell.run(&["df".into(), "-h".into()]).await;
    checks.push(match disk_output {
        Ok(output) => HealthCheck {
            name: "disk".into(),
            status: "ok".into(),
            details: output.stdout,
        },
        Err(err) => HealthCheck {
            name: "disk".into(),
            status: "warn".into(),
            details: err.to_string(),
        },
    });

    checks.extend(check_postfix_ban_integration(shell, config).await);

    checks
}

async fn check_postfix_ban_integration(shell: &Shell, config: &Config) -> Vec<HealthCheck> {
    let mut checks = Vec::new();
    let postconf = shell.run(&["postconf".into()]).await;

    let output = match postconf {
        Ok(output) if output.status == 0 => output.stdout,
        Ok(output) => {
            let details = if output.stderr.is_empty() {
                output.stdout
            } else {
                output.stderr
            };
            return vec![HealthCheck {
                name: "postfix:bans".into(),
                status: "warn".into(),
                details: format!("postconf unavailable or failed: {details}"),
            }];
        }
        Err(err) => {
            return vec![HealthCheck {
                name: "postfix:bans".into(),
                status: "warn".into(),
                details: format!("postconf unavailable or failed: {err}"),
            }];
        }
    };

    checks.push(restriction_check(
        &output,
        "smtpd_recipient_restrictions",
        &[("check_recipient_access", &config.bans.address_file)],
        "postfix:bans:addresses",
        "sudo postconf -e 'smtpd_recipient_restrictions = check_recipient_access texthash:/etc/chatmail-control/blocked_addresses.txt, reject_unauth_destination'",
    ));

    checks.push(restriction_check(
        &output,
        "smtpd_sender_restrictions",
        &[
            ("check_sender_access", &config.bans.address_file),
            ("check_sender_access", &config.bans.domain_file),
        ],
        "postfix:bans:senders",
        "sudo postconf -e 'smtpd_sender_restrictions = check_sender_access texthash:/etc/chatmail-control/blocked_addresses.txt, check_sender_access texthash:/etc/chatmail-control/blocked_domains.txt'",
    ));

    checks.push(restriction_check(
        &output,
        "smtpd_client_restrictions",
        &[("check_client_access", &config.bans.ip_file)],
        "postfix:bans:ips",
        "sudo postconf -e 'smtpd_client_restrictions = check_client_access texthash:/etc/chatmail-control/blocked_ips.txt'",
    ));

    checks
}

fn restriction_check(
    postconf: &str,
    setting_name: &str,
    expected_checks: &[(&str, &str)],
    check_name: &str,
    remediation: &str,
) -> HealthCheck {
    let line = postconf
        .lines()
        .find(|line| line.starts_with(&format!("{setting_name} =")))
        .unwrap_or_default()
        .to_string();
    let is_ok = expected_checks.iter().all(|(expected_check, file_path)| {
        line.contains(expected_check) && line.contains(file_path)
    });

    if is_ok {
        HealthCheck {
            name: check_name.into(),
            status: "ok".into(),
            details: line,
        }
    } else {
        let expected = expected_checks
            .iter()
            .map(|(expected_check, file_path)| format!("{expected_check} + {file_path}"))
            .collect::<Vec<_>>()
            .join(", ");
        HealthCheck {
            name: check_name.into(),
            status: "warn".into(),
            details: format!(
                "missing Postfix integration for {expected}. Current: {line}. Apply: {remediation}"
            ),
        }
    }
}

async fn txt_record_check(
    resolver: &TokioAsyncResolver,
    name: &str,
    expected_prefix: &str,
    check_name: &str,
) -> HealthCheck {
    match resolver.txt_lookup(name).await {
        Ok(records) => {
            let joined = records
                .iter()
                .flat_map(|record| record.txt_data().iter())
                .map(|txt| String::from_utf8_lossy(txt).to_string())
                .collect::<Vec<_>>();
            if joined.iter().any(|line| line.starts_with(expected_prefix)) {
                HealthCheck {
                    name: check_name.into(),
                    status: "ok".into(),
                    details: joined.join(" | "),
                }
            } else {
                HealthCheck {
                    name: check_name.into(),
                    status: "warn".into(),
                    details: joined.join(" | "),
                }
            }
        }
        Err(err) => HealthCheck {
            name: check_name.into(),
            status: "error".into(),
            details: err.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::restriction_check;

    #[test]
    fn validates_bidirectional_address_ban_wiring() {
        let postconf = concat!(
            "smtpd_sender_restrictions = check_sender_access texthash:/etc/chatmail-control/blocked_addresses.txt, ",
            "check_sender_access texthash:/etc/chatmail-control/blocked_domains.txt\n",
        );

        let check = restriction_check(
            postconf,
            "smtpd_sender_restrictions",
            &[
                (
                    "check_sender_access",
                    "/etc/chatmail-control/blocked_addresses.txt",
                ),
                (
                    "check_sender_access",
                    "/etc/chatmail-control/blocked_domains.txt",
                ),
            ],
            "postfix:bans:senders",
            "fix it",
        );

        assert_eq!(check.status, "ok");
    }
}
