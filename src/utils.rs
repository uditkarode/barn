use std::path::PathBuf;

use actix_web::{
    dev::ServiceRequest, error::InternalError, http::StatusCode, web, Error, HttpResponse,
};
use actix_web_httpauth::extractors::basic::BasicAuth;
use anyhow::{anyhow, Context, Result};
use bytes::Bytes;

use crate::{
    constants::{FILENAME_REGEX, VIEWER_TEMPLATE_STR},
    BarnState,
};

pub fn transform_bytes(bytes: Bytes, class: &str) -> Bytes {
    let str = String::from_utf8(bytes.into()).unwrap();
    let modified = str
        .lines()
        .map(|line| format!("<pre class=\"{}\">{}</pre>\n", class, line))
        .collect::<Vec<_>>()
        .join("");
    Bytes::from(modified)
}

pub fn templated_error(message: &str, status_code: StatusCode) -> Error {
    let response = HttpResponse::build(status_code)
        .content_type("text/html; charset=utf-8")
        .body(message.to_string());

    let msg = format!(
        "{}{}{}{}",
        *VIEWER_TEMPLATE_STR, "<p class=\"warning\">", message, "</p> </div> </body> </html>"
    );

    InternalError::from_response(msg, response).into()
}

pub trait IntoHttpError<T> {
    fn http_error(
        self,
        message: &str,
        status_code: StatusCode,
    ) -> core::result::Result<T, actix_web::Error>;

    fn templated_error(
        self,
        message: &str,
        status_code: StatusCode,
    ) -> core::result::Result<T, actix_web::Error>
    where
        Self: std::marker::Sized,
    {
        self.http_error(
            format!(
                "{}{}{}{}",
                *VIEWER_TEMPLATE_STR,
                "<p class=\"warning\">",
                message,
                "</p> </div> </body> </html>"
            )
            .as_str(),
            status_code,
        )
    }

    fn generic_error(self) -> core::result::Result<T, actix_web::Error>
    where
        Self: std::marker::Sized,
    {
        self.templated_error("Something went wrong!", StatusCode::INTERNAL_SERVER_ERROR)
    }
}

impl<T, E: std::fmt::Debug> IntoHttpError<T> for core::result::Result<T, E> {
    fn http_error(
        self,
        message: &str,
        status_code: StatusCode,
    ) -> core::result::Result<T, actix_web::Error> {
        match self {
            Ok(val) => Ok(val),
            Err(_) => {
                let response = HttpResponse::build(status_code)
                    .content_type("text/html; charset=utf-8")
                    .body(message.to_string());
                Err(InternalError::from_response(message.to_string(), response).into())
            }
        }
    }
}

impl<T> IntoHttpError<T> for core::option::Option<T> {
    fn http_error(
        self,
        message: &str,
        status_code: StatusCode,
    ) -> core::result::Result<T, actix_web::Error> {
        match self {
            Some(val) => Ok(val),
            None => {
                let response = HttpResponse::build(status_code)
                    .content_type("text/html; charset=utf-8")
                    .body(message.to_string());
                Err(InternalError::from_response(message.to_string(), response).into())
            }
        }
    }
}

pub fn check_executables_root(root: &PathBuf) -> Result<()> {
    // check if the executables root folder exists and is a dir
    if !root.exists() || !root.is_dir() {
        Err(anyhow!(
            "'{}' either doesn't exist or isn't a directory",
            root.display()
        ))?
    }

    // check if we have execute permissions in the said dir
    let metadata =
        std::fs::metadata(root).with_context(|| "Unable to get metadata for root dir")?;
    let permissions = metadata.permissions();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = permissions.mode();

        // 0o100 is the executable bit for user
        if mode & 0o100 == 0 {
            Err(anyhow::anyhow!(
                "No execute permission inside '{}'",
                root.display()
            ))?
        }
    }

    Ok(())
}

pub async fn request_validator(
    req: ServiceRequest,
    creds: BasicAuth,
) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    let config = &req.app_data::<web::Data<BarnState>>().unwrap().config;
    let executable = req.path().trim_start_matches("/");
    let program_path = config.options.root.join(&executable);

    if !FILENAME_REGEX.is_match(&executable) {
        return Err((
            templated_error("Disallowed filename", StatusCode::BAD_REQUEST),
            req,
        ));
    }

    if !program_path.exists() || !program_path.is_file() {
        return Err((
            templated_error("Non-existent executable", StatusCode::BAD_REQUEST),
            req,
        ));
    }

    // if this script belongs to the 'passwordless' group, no auth should be done
    let is_passwordless = config
        .group
        .iter()
        .any(|entry| entry.name == "passwordless" && entry.regex.is_match(executable));

    if is_passwordless {
        return Ok(req);
    }

    // check if creds were provided and obtain them
    let username = creds.user_id();
    let password_res = creds.password();
    let password = match password_res {
        Some(p) => p,
        None => {
            return Err((
                templated_error("No password provided", StatusCode::BAD_REQUEST),
                req,
            ))
        }
    };

    // check if a user with the given creds exists
    let user_opt = config
        .user
        .iter()
        .find(|entry| entry.username == username && entry.password == password);

    let user = match user_opt {
        Some(user) => user,
        None => {
            return Err((
                templated_error("Invalid credentials", StatusCode::BAD_REQUEST),
                req,
            ))
        }
    };

    // check if said user has access to the script group
    let has_access = config
        .group
        .iter()
        .filter(|entry| user.groups.contains(&entry.name))
        .any(|entry| entry.regex.is_match(executable));

    if has_access {
        Ok(req)
    } else {
        return Err((
            templated_error(
                "You don't have access to this executable",
                StatusCode::UNAUTHORIZED,
            ),
            req,
        ));
    }
}
