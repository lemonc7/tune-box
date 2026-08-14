use axum::{
    body::Body,
    extract::Path,
    http::{self, HeaderMap, HeaderValue, header},
    response::{IntoResponse, Response},
};
use rust_embed::Embed;
use std::fmt::Write;

#[derive(Embed)]
#[folder = "assets/"]
struct Assets;

pub async fn assets(
    Path(path): Path<String>,
    request_headers: HeaderMap,
) -> Result<Response, http::StatusCode> {
    let file = Assets::get(&path).ok_or(http::StatusCode::NOT_FOUND)?;
    let mime = mime_guess::from_path(&path).first_or_octet_stream();
    let etag = etag(file.metadata.sha256_hash());

    cfg_select! {
        debug_assertions => {
            let _ = request_headers;
        }
        _ => {
            if request_headers
                .get(header::IF_NONE_MATCH)
                .and_then(|value| value.to_str().ok())
                == Some(etag.as_str())
            {
                let mut resp = http::StatusCode::NOT_MODIFIED.into_response();
                set_cache_headers(resp.headers_mut(), &etag);
                return Ok(resp);
            }
        }
    }

    let mut resp = Body::from(file.data.into_owned()).into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(mime.as_ref())
            .map_err(|_| http::StatusCode::INTERNAL_SERVER_ERROR)?,
    );
    set_cache_headers(resp.headers_mut(), &etag);

    Ok(resp)
}

fn set_cache_headers(headers: &mut HeaderMap, etag: &str) {
    let cache_control = cfg_select! {
        debug_assertions => HeaderValue::from_static("no-store"),
        _ => HeaderValue::from_static("public, no-cache"),
    };

    headers.insert(header::CACHE_CONTROL, cache_control);
    headers.insert(
        header::ETAG,
        HeaderValue::from_str(etag).expect("SHA-256 ETag must be a valid header value"),
    );
}

fn etag(hash: [u8; 32]) -> String {
    let mut value = String::with_capacity(66);
    value.push('"');
    for byte in hash {
        write!(&mut value, "{byte:02x}").expect("writing to a String cannot fail");
    }
    value.push('"');
    value
}
