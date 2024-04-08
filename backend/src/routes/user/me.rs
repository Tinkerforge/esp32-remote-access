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

use actix_web::{get, web, HttpResponse, Responder};

use crate::{
    models::{filtered_user::FilteredUser, uuid},
    routes::user::get_user,
    AppState,
};

/// Get information about the currently logged in user.
#[utoipa::path(
    context_path = "/user",
    responses(
        (status = 200, description = "", body = FilteredUser),
        (status = 400, description = "The jwt token was somehow valid but contained a non valid uuid.")
    ),
    security(
        ("jwt" = [])
    )
)]
#[get("/me")]
async fn me(
    state: web::Data<AppState>,
    id: uuid::Uuid,
) -> Result<impl Responder, actix_web::Error> {
    let user = get_user(&state, id.into()).await?;

    Ok(HttpResponse::Ok().json(FilteredUser::from(user)))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use actix_web::{cookie::Cookie, test, App};
    use db_connector::models::users::User;
    use diesel::{prelude::*, SelectableHelper};

    use crate::{
        defer,
        routes::auth::{
            login::tests::verify_and_login_user,
            register::tests::{create_user, delete_user},
        },
        tests::configure,
    };

    pub fn get_test_user(username: &str) -> User {
        use db_connector::schema::users::dsl::*;

        let pool = db_connector::test_connection_pool();
        let mut conn = pool.get().unwrap();
        users
            .filter(name.eq(username))
            .select(User::as_select())
            .get_result(&mut conn)
            .unwrap()
    }

    pub fn get_test_user_by_email(mail: &str) -> User {
        use db_connector::schema::users::dsl::*;

        let pool = db_connector::test_connection_pool();
        let mut conn = pool.get().unwrap();
        users
            .filter(email.eq(mail))
            .select(User::as_select())
            .get_result(&mut conn)
            .unwrap()
    }

    #[actix_web::test]
    async fn test_me() {
        let mail = "me@test.invalid";
        let username = "test_me_user";
        let key = create_user(mail, username).await;
        defer!(delete_user(mail));

        let app = App::new()
            .configure(configure)
            .service(me)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        let token = verify_and_login_user(username, key).await;
        let req = test::TestRequest::get()
            .uri("/me")
            .cookie(Cookie::new("access_token", token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let body: FilteredUser = test::read_body_json(resp).await;
        assert_eq!(body.email, mail);
    }
}
