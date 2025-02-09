use std::{io::SeekFrom, path::Path, time::{SystemTime, UNIX_EPOCH}};

use axum::{body::Bytes, extract::Query, http::StatusCode, response::{IntoResponse, Response}, Json};
use md5::Digest;
use ring::rand::SecureRandom;
use tokio::{fs::{rename, File}, io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt}};

use crate::proto;

#[derive(serde::Serialize)]
struct UploadLimit {
    file_size_limit: u64,
}

pub async fn upload_get_limit() -> Response {
    Json(UploadLimit {
        file_size_limit: 1024 * 1024 * 1024,
    }).into_response()
}

#[derive(serde::Deserialize)]
pub struct UploadStartQuery {
    file_size: u64,
}

#[derive(serde::Serialize)]
struct UploadStartResponse {
    token: String,
    chunk_size: u64,
}

#[tracing::instrument]
pub async fn upload_start(Query(UploadStartQuery { file_size }): Query<UploadStartQuery>) -> Response {
    let mut rndbuf = vec![0u8; 16];
    ring::rand::SystemRandom::new().fill(&mut rndbuf).unwrap();
    let token = proto::FileRef {
        version: Some(proto::file_ref::Version::V1(proto::FileRefV1 {
            created_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
            random: rndbuf,
            size: file_size,
        })),
    };

    let path = token.to_path(true).unwrap();

    // make directory
    let dirpath = Path::new(&path).parent().unwrap();
    match tokio::fs::create_dir_all(dirpath).await {
        Ok(_) => (),
        Err(err) => {
            tracing::error!("Failed to create directory at {}: {:?}", dirpath.display(), err);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response()
        },
    }

    let file = File::options().create_new(true).write(true).open(&path).await;
    match file {
        Ok(file) => {
            drop(file);
        },
        Err(err) => {
            tracing::error!("Failed to create file at {}: {:?}", path, err);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response()
        },
    };

    Json(UploadStartResponse {
        token: token.to_ref_string().to_string(),
        chunk_size: 0,
    }).into_response()
}

#[derive(serde::Deserialize)]
pub struct UploadChunkQuery {
    token: String,
    offset: u64,
}

#[tracing::instrument]
pub async fn upload_chunk(Query(UploadChunkQuery { token, offset }): Query<UploadChunkQuery>, bytes: Bytes) -> Response {
    let token = proto::FileRef::from_ref_string(token);
    let token = match token {
        Some(token) => token,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };
    let path = token.to_path(true).unwrap();
    let token = match token {
        proto::FileRef {
            version: Some(proto::file_ref::Version::V1(token)),
        } => token,
        _ => {
            tracing::warn!("Unsupported token version: {:?}", token);
            return StatusCode::BAD_REQUEST.into_response()
        },
    };

    let max_size = offset + bytes.len() as u64;
    if max_size > token.size {
        tracing::warn!("Chunk size exceeds file size: {} > {} + {}", max_size, offset, token.size);
        return StatusCode::BAD_REQUEST.into_response()
    }
    
    let file = File::options().write(true).open(&path).await;
    let mut file = match file {
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
    
    match file.write_all(&bytes).await {
        Ok(_) => (),
        Err(err) => {
            tracing::error!("Failed to write chunk to file at {}: {:?}", path, err);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response()
        },
    };

    StatusCode::NO_CONTENT.into_response()
}

#[derive(serde::Deserialize)]
pub struct UploadFinalizeQuery {
    token: String,
}

#[derive(serde::Deserialize)]
pub struct UploadFinalizeBody {
    name: String,
    md5: String,
}

#[derive(serde::Serialize)]
struct UploadFinalizeResponse {
    r#ref: String,
}

#[tracing::instrument]
pub async fn upload_finalize(Query(UploadFinalizeQuery { token }): Query<UploadFinalizeQuery>, Json(UploadFinalizeBody { name, md5 }): Json<UploadFinalizeBody>) -> Response {
    let token = proto::FileRef::from_ref_string(token);
    let token = match token {
        Some(token) => token,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };
    let path = match token.to_path(true) {
        Some(path) => path,
        None => {
            tracing::warn!("Unsupported token version: {:?}", token);
            return StatusCode::BAD_REQUEST.into_response()
        },
    };
    let dst = match token.to_path(false) {
        Some(path) => path,
        None => {
            tracing::warn!("Unsupported token version: {:?}", token);
            return StatusCode::BAD_REQUEST.into_response()
        },
    };

    let file = File::options().read(true).open(&path).await;
    let mut file = match file {
        Ok(file) => file,
        Err(err) => {
            tracing::error!("Failed to open file at {}: {:?}", path, err);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response()
        },
    };

    let mut hash = md5::Md5::new();
    let mut buf = vec![0u8; 524288];
    loop {
        let n = match file.read(&mut buf).await {
            Ok(n) => n,
            Err(err) => {
                tracing::error!("Failed to read from file at {}: {:?}", path, err);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response()
            },
        };
        if n == 0 {
            break
        }
        hash.update(&buf[..n]);
    }

    let hash = hash.finalize();
    let hash = hex::encode(hash);
    let md5 = md5.to_lowercase();
    if hash != md5 {
        tracing::warn!("MD5 mismatch: {} != {}", hash, md5);
        return StatusCode::BAD_REQUEST.into_response()
    }
    
    drop(file);
    match rename(&path, &dst).await {
        Ok(_) => (),
        Err(err) => {
            tracing::error!("Failed to rename file from {} to {}: {:?}", path, dst, err);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response()
        },
    }
    
    Json(UploadFinalizeResponse {
        r#ref: token.to_ref_string().to_string(),
    }).into_response()
}
