use axum::{
    body::Body,
    extract::Path,
    http::{self, header},
    response::{IntoResponse, Response},
};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "assets/"]
struct Assets;

pub async fn assets(Path(path): Path<String>) -> Result<Response, http::StatusCode> {
    let file = Assets::get(&path).ok_or(http::StatusCode::NOT_FOUND)?;
    let mime = mime_guess::from_path(&path).first_or_octet_stream();
    let mut resp = Body::from(file.data.into_owned()).into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        http::HeaderValue::from_str(mime.as_ref())
            .map_err(|_| http::StatusCode::INTERNAL_SERVER_ERROR)?,
    );
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        http::HeaderValue::from_static("public, max-age=86400"),
    );

    Ok(resp)
}
