use std::io::SeekFrom;

use axum::{body::Bytes, extract::Path, http::StatusCode, response::{IntoResponse, Response}, Json};
use tokio::{fs::metadata, io::{AsyncReadExt, AsyncSeekExt}};

use crate::proto;

#[derive(serde::Serialize)]
struct FileMeta {
    file_size: u64,
}

#[tracing::instrument]
pub async fn file_meta(Path(token): Path<String>) -> Response {
    let token = proto::FileRef::from_ref_string(token);
    let token = match token {
        Some(token) => token,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    let path = token.to_path(false);
    let path = match path {
        Some(path) => path,
        None => {
            tracing::warn!("Unsupported token version: {:?}", token);
            return StatusCode::BAD_REQUEST.into_response()
        },
    };

    let file_size = match metadata(&path).await {
        Ok(meta) => meta.len(),
        Err(err) => {
            tracing::error!("Failed to get metadata for file at {}: {:?}", path, err);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response()
        },
    };

    Json(FileMeta {
        file_size,
    }).into_response()
}

#[tracing::instrument]
pub async fn file_chunk(Path((token, offset)): Path<(String, u64)>) -> Response {
    if (offset % 524288) != 0 {
        return StatusCode::BAD_REQUEST.into_response()
    }

    let token = proto::FileRef::from_ref_string(token);
    let token = match token {
        Some(token) => token,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    let path = token.to_path(false);
    let path = match path {
        Some(path) => path,
        None => {
            tracing::warn!("Unsupported token version: {:?}", token);
            return StatusCode::BAD_REQUEST.into_response()
        },
    };

    let mut file = match tokio::fs::File::open(&path).await {
        Ok(file) => file,
        Err(err) => {
            tracing::error!("Failed to open file at {}: {:?}", path, err);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response()
        },
    };

    match file.seek(SeekFrom::Start(offset)).await {
        Ok(_) => (),
        Err(err) => {
            tracing::error!("Failed to seek to offset {} in file at {}: {:?}", offset, path, err);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response()
        },
    };

    let mut buf = vec![0; 524288];
    let mut i = 0;
    while i < buf.len() {
        match file.read(&mut buf[i..]).await {
            Ok(0) => break,
            Ok(n) => i += n,
            Err(err) => {
                tracing::error!("Failed to read from file at {}: {:?}", path, err);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response()
            },
        };
    }
    buf.truncate(i);

    Bytes::from_owner(buf).into_response()
}