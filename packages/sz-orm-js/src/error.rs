//! DbError → napi::Error 映射

use napi::bindgen_prelude::Error;
use napi_derive::napi;
use sz_orm_core::DbError;

#[napi(object)]
#[allow(dead_code)]
pub struct DbErrorInfo {
    pub message: String,
    pub code: String,
}

#[allow(dead_code)]
pub fn db_error_to_napi(err: DbError) -> Error {
    let code = err.error_code().to_string();
    let msg = err.to_string();
    Error::from_reason(format!("[{}] {}", code, msg))
}
