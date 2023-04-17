use bytes::Bytes;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    pub static ref FILENAME_REGEX: Regex =
        Regex::new(r"^[a-zA-Z0-9_\-][a-zA-Z0-9_\-\.]*?$").unwrap();
}

lazy_static! {
    pub static ref INVALID_ROUTE_ERROR: String = format!(
        "{}{}{}{}",
        *VIEWER_TEMPLATE_STR,
        "<p class=\"warning\">",
        "invalid path, use /executable_name to run executables",
        "</p> </div> </body> </html>"
    );
}

pub static VIEWER_TEMPLATE: &[u8] = include_bytes!("viewer.html");
lazy_static! {
    pub static ref VIEWER_TEMPLATE_STR: String =
        String::from_utf8(VIEWER_TEMPLATE.to_vec()).unwrap();
    pub static ref VIEWER_TEMPLATE_BYTES: Bytes = Bytes::from_static(VIEWER_TEMPLATE);
    pub static ref VIEWER_ENDING_BYTES: Bytes = Bytes::from_static(b"</div> </body> </html>");
}
