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

    checks
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
