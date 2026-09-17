use std::time::Duration;

use actix_web::web;
use anyhow::Error;
use askama::Template;
use backend::utils;
use backend::{utils::get_connection, AppState};
use diesel::QueryDsl;
use diesel_async::RunQueryDsl as _;

#[derive(Template)]
#[template(path = "monitoring.html")]
struct MonitoringMail<'a> {
    num_users: i64,
    num_devices: i64,
    server_name: &'a str,
}

async fn get_numbers(conn: &mut diesel_async::AsyncPgConnection) -> Result<(i64, i64), Error> {
    use db_connector::schema::chargers::dsl::*;
    use db_connector::schema::users::dsl::*;

    let num_users: i64 = users.count().get_result(conn).await?;
    let num_devices: i64 = chargers.count().get_result(conn).await?;

    Ok((num_users, num_devices))
}

fn send_mail(state: &web::Data<AppState>, num_users: i64, num_devices: i64) -> Result<(), Error> {
    let body = MonitoringMail {
        num_users,
        num_devices,
        server_name: &std::env::var("SERVER_NAME")?,
    };
    let body = body.render()?;

    utils::send_email(
        &std::env::var("MONITORING_EMAIL")?,
        "Monitoring mail",
        body,
        state,
    );

    Ok(())
}

pub fn start_monitoring(state: web::Data<AppState>) {
    if std::env::var("SERVER_NAME").is_err() {
        log::info!("Monitoring Mailer disabled");
        return;
    }
    if std::env::var("MONITORING_EMAIL").is_err() {
        log::info!("Monitoring Mailer disabled");
        return;
    }

    actix::spawn(async move {
        loop {
            match get_connection(&state).await {
                Ok(mut conn) => match get_numbers(&mut conn).await {
                    Ok((num_users, num_devices)) => {
                        let mail_state = state.clone();
                        match tokio::task::spawn_blocking(move || {
                            send_mail(&mail_state, num_users, num_devices)
                        })
                        .await
                        {
                            Ok(Ok(())) => {
                                log::info!(
                                    "Monitoring email sent successfully. Users: {num_users}, Chargers: {num_devices}"
                                );
                            }
                            Ok(Err(err)) => {
                                log::error!("Failed to send monitoring mail: {err}");
                            }
                            Err(err) => {
                                log::error!("Monitoring mail task failed: {err}");
                            }
                        }
                    }
                    Err(err) => {
                        log::error!("Failed to get monitoring statistics from database: {err}");
                    }
                },
                Err(err) => {
                    log::error!("Failed to get database connection for monitoring: {err:?}");
                }
            }

            tokio::time::sleep(Duration::from_secs(60 * 60 * 24)).await;
        }
    });
}
