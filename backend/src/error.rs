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
    error,
    http::{header::ContentType, StatusCode},
    HttpResponse,
};
use derive_more::{Display, Error};

#[derive(Debug, Display, Error)]
pub enum Error {
    #[display(fmt = "An internal error occured. Please try again later")]
    InternalError,
    #[display(fmt = "An account with this email already exists")]
    UserAlreadyExists,
    #[display(fmt = "Wrong username or password")]
    WrongCredentials,
    #[display(fmt = "Not verified")]
    NotVerified,
    #[display(fmt = "Unauthorized")]
    Unauthorized,
    #[display(fmt = "This charger already exists")]
    ChargerAlreadyExists,
    #[display(fmt = "User does not exist")]
    UserDoesNotExist,
    #[display(fmt = "Wg keys do not exist")]
    WgKeysDoNotExist,
    #[display(fmt = "No unused Key left")]
    AllKeysInUse,
    #[display(fmt = "Key already in use")]
    WgKeyAlreadyInUse,
    #[display(fmt = "Charger was not seen yet")]
    ChargerNotSeenYet,
    #[display(fmt = "Logged in user is not the owner of the charger")]
    UserIsNotOwner,
    #[display(fmt = "Request does not contain a valid ip address")]
    NoValidIp,
    #[display(fmt = "Charger is currently not connected to the server")]
    ChargerDisconnected,
    #[display(fmt = "Not an active session")]
    SessionDoesNotExist,
}

impl error::ResponseError for Error {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code())
            .insert_header(ContentType::plaintext())
            .body(self.to_string())
    }

    fn status_code(&self) -> StatusCode {
        match *self {
            Self::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
            Self::UserAlreadyExists => StatusCode::CONFLICT,
            Self::WrongCredentials => StatusCode::BAD_REQUEST,
            Self::NotVerified => StatusCode::UNAUTHORIZED,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::ChargerAlreadyExists => StatusCode::CONFLICT,
            Self::UserDoesNotExist => StatusCode::BAD_REQUEST,
            Self::WgKeysDoNotExist => StatusCode::BAD_REQUEST,
            Self::AllKeysInUse => StatusCode::NOT_FOUND,
            Self::WgKeyAlreadyInUse => StatusCode::CONFLICT,
            Self::ChargerNotSeenYet => StatusCode::NOT_FOUND,
            Self::UserIsNotOwner => StatusCode::UNAUTHORIZED,
            Self::NoValidIp => StatusCode::BAD_REQUEST,
            Self::ChargerDisconnected => StatusCode::BAD_GATEWAY,
            Self::SessionDoesNotExist => StatusCode::UNAUTHORIZED,
        }
    }
}
