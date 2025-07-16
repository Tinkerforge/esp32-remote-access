use std::time::Duration;

use actix_web::web;
use anyhow::Error;
use askama::Template;
use backend::utils;
use backend::{utils::get_connection, AppState};
use diesel::prelude::*;
use diesel::{
    r2d2::{ConnectionManager, PooledConnection},
    PgConnection, QueryDsl,
};

#[derive(Template)]
#[template(path = "monitoring.html")]
struct MonitoringMail<'a> {
    num_users: i64,
    num_chargers: i64,
    server_name: &'a str,
}

fn get_numbers(
    mut conn: PooledConnection<ConnectionManager<PgConnection>>,
) -> Result<(i64, i64), Error> {
    use db_connector::schema::chargers::dsl::*;
    use db_connector::schema::users::dsl::*;

    let num_users: i64 = users.count().get_result(&mut conn)?;
    let num_chargers: i64 = chargers.count().get_result(&mut conn)?;

    Ok((num_users, num_chargers))
}

fn send_mail(state: &web::Data<AppState>, num_users: i64, num_chargers: i64) -> Result<(), Error> {
    let body = MonitoringMail {
        num_users,
        num_chargers,
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

    std::thread::spawn(move || loop {
        match get_connection(&state) {
            Ok(conn) => {
                let (num_users, num_chargers) = match get_numbers(conn) {
                    Ok(v) => v,
                    Err(err) => {
                        log::error!("Failed to get monitoring statistics from database: {err}");
                        std::thread::sleep(Duration::from_secs(60 * 60 * 24));
                        continue;
                    }
                };
                match send_mail(&state, num_users, num_chargers) {
                    Ok(()) => {
                        log::info!(
                            "Monitoring email sent successfully. Users: {num_users}, Chargers: {num_chargers}"
                        );
                    }
                    Err(err) => {
                        log::error!("Failed to send monitoring mail: {err}");
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to get database connection for monitoring: {e:?}");
            }
        }

        std::thread::sleep(Duration::from_secs(60 * 60 * 24));
    });
}
