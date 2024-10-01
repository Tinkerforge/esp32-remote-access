/* esp32-remote-access
 * Copyright (C) 2024 Frederic Henrichs <frederic@tinkerforge.com>
 *
 * This library is free software; you can redistribute it and/or
 * modify it under the terms of the GNU Lesser General Public
 * License as published by the Free Software Foundation; either
 * version 2 of the License, or (at your option) any later version.
 *
 * This library is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
 * Lesser General Public License for more details.
 *
 * You should have received a copy of the GNU Lesser General Public
 * License along with this library; if not, write to the
 * Free Software Foundation, Inc., 59 Temple Place - Suite 330,
 * Boston, MA 02111-1307, USA.
 */

use actix_web::{
    cookie::{time::Duration, Cookie},
    get, web, HttpRequest, HttpResponse, Responder,
};
use db_connector::models::refresh_tokens::RefreshToken;
use diesel::{prelude::*, result::Error::NotFound};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{
    error::Error,
    middleware::get_token,
    routes::{
        auth::jwt_refresh::{delete_refresh_token, extract_token},
        user::get_user,
    },
    utils::{get_connection, web_block_unpacked},
    AppState,
};

#[derive(Deserialize, IntoParams)]
pub struct LogoutQuery {
    logout_all: bool,
}

/// Logout user
#[utoipa::path(
    context_path = "/user",
    params(
        LogoutQuery
    ),
    responses(
        (status = 200, description = "User logged out")
    ),
    security(
        ("jwt" = [])
    )
)]
#[get("/logout")]
pub async fn logout(
    req: HttpRequest,
    query: web::Query<LogoutQuery>,
    state: web::Data<AppState>,
    user_id: crate::models::uuid::Uuid,
) -> actix_web::Result<impl Responder> {
    if query.logout_all {
        delete_all_refresh_tokens(user_id.into(), &state).await?;
    } else if let Some(token) = get_token(&req, "refresh_token") {
        let (token, _) = extract_token(token, &state.jwt_secret)?;
        delete_refresh_token(token, &state).await?;
    }

    let access_token = Cookie::build("access_token", "")
        .path("/")
        .max_age(Duration::new(-1, 0))
        .http_only(true)
        .finish();
    let refresh_token = Cookie::build("refresh_token", "")
        .path("/")
        .max_age(Duration::new(-1, 0))
        .http_only(true)
        .finish();

    Ok(HttpResponse::Ok()
        .cookie(access_token)
        .cookie(refresh_token)
        .body(""))
}

pub async fn delete_all_refresh_tokens(
    uid: uuid::Uuid,
    state: &web::Data<AppState>,
) -> actix_web::Result<()> {
    let user = get_user(state, uid).await?;

    let mut conn = get_connection(state)?;
    web_block_unpacked(move || {
        match diesel::delete(RefreshToken::belonging_to(&user)).execute(&mut conn) {
            Ok(_) => Ok(()),
            Err(NotFound) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use actix_web::{
        cookie::Cookie,
        test::{self, TestRequest},
        App,
    };
    use db_connector::{models::refresh_tokens::RefreshToken, test_connection_pool};
    use diesel::prelude::*;

    use crate::{
        middleware::jwt::JwtMiddleware,
        routes::user::{me::tests::get_test_user, tests::TestUser},
        tests::configure,
    };

    use super::logout;

    fn get_tokens(mail: &str) -> Vec<RefreshToken> {
        use db_connector::schema::refresh_tokens::dsl::*;
        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        let user = get_test_user(mail);
        let tokens: Vec<RefreshToken> = refresh_tokens
            .filter(user_id.eq(user.id))
            .select(RefreshToken::as_select())
            .load(&mut conn)
            .expect("Failed to load refresh tokens");

        tokens
    }

    #[actix_web::test]
    async fn simple_logout() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await.to_owned();
        let refresh_token = user.get_refresh_token();

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(logout);
        let app = test::init_service(app).await;

        let req = TestRequest::get()
            .uri("/logout?logout_all=false")
            .cookie(Cookie::new("access_token", token))
            .cookie(Cookie::new("refresh_token", refresh_token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn logout_all() {
        let (mut user, mail) = TestUser::random().await;
        let token = user.login().await.to_owned();
        let refresh_token = user.get_refresh_token().to_owned();
        user.additional_login().await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(logout);
        let app = test::init_service(app).await;

        let req = TestRequest::get()
            .uri("/logout?logout_all=true")
            .cookie(Cookie::new("access_token", token))
            .cookie(Cookie::new("refresh_token", refresh_token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        assert_eq!(get_tokens(&mail).len(), 0);
    }

    #[actix_web::test]
    async fn no_tokens() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(logout);
        let app = test::init_service(app).await;

        let req = TestRequest::get()
            .uri("/logout?logout_all=true")
            .to_request();
        let resp = crate::tests::call_service(&app, req).await;
        println!("{}", resp.status());
        assert_eq!(resp.status(), 401);
    }
}
