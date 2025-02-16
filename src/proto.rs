use std::env;

use base64::Engine;
use chrono::{DateTime, Utc};
use prost::Message;

include!(concat!(env!("OUT_DIR"), "/_.rs"));

impl FileRef {
    pub fn to_ref_string(&self) -> String {
        base64::prelude::BASE64_URL_SAFE_NO_PAD.encode(self.encode_to_vec())
    }

    pub fn from_ref_string(input: String) -> Option<FileRef> {
        let decoded = base64::prelude::BASE64_URL_SAFE_NO_PAD.decode(input);
        let decoded = match decoded {
            Err(_) => return None,
            Ok(v) => v,
        };
        let decoded = FileRef::decode(&decoded[..]);
        match decoded {
            Ok(v) => Some(v),
            Err(_) => None,
        }
    }

    pub fn to_path(&self, tmp: bool) -> Option<String> {
        match self.version {
            Some(file_ref::Version::V1(ref v1)) => {
                let created_at = v1.created_at;
                let created_at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(created_at);
                let created_at: DateTime<Utc> = created_at.into();
                let sizestr = match v1.size {
                    Some(v) => v.to_string(),
                    None => "unknown".to_string(),
                };
                Some(format!("{}/v1/{}_s{}_{}.{}", env::var("DATA_DIR").unwrap_or("./data".to_string()), created_at.format("%Y/%m/%d_%H/%Y%m%d_%H%M%S"), sizestr, hex::encode(&v1.random), if tmp { "tmp" } else { "bin" }))
            },
            None => None,
        }
    }
}
