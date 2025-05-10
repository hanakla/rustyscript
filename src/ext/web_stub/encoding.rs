use std::any::Any;
use std::borrow::Cow;
use std::fmt::Debug;
// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.
use deno_core::{op2, v8, ByteString, ToJsBuffer};
use deno_error::JsErrorClass;

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum WebError {
    #[error("Failed to decode base64")]
    Base64Decode,
    #[error("The encoding label provided ('{0}') is invalid.")]
    InvalidEncodingLabel(String),
    #[error("buffer exceeds maximum length")]
    BufferTooLong,
    #[error("Value too large to decode")]
    ValueTooLarge,
    #[error("Provided buffer too small")]
    BufferTooSmall,
    #[error("The encoded data is not valid")]
    DataInvalid,
    #[error(transparent)]
    DataError(#[from] v8::DataError),
}

impl JsErrorClass for WebError {
    fn get_class(&self) -> Cow<'static, str> {
        Cow::from("Error")
    }

    fn get_message(&self) -> Cow<'static, str> {
        match self {
            WebError::Base64Decode => Cow::from(format!("{}", self)),
            WebError::InvalidEncodingLabel(label) => Cow::from(format!("{}", self)),
            WebError::BufferTooLong => Cow::from(format!("{}", self)),
            WebError::ValueTooLarge => Cow::from(format!("{}", self)),
            WebError::BufferTooSmall => Cow::from(format!("{}", self)),
            WebError::DataInvalid => Cow::from(format!("{}", self)),
            WebError::DataError(e) => Cow::from(format!("{}", e)),
        }
    }

    fn get_additional_properties(&self) -> Vec<(Cow<'static, str>, Cow<'static, str>)> {
        vec![]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[op2]
#[serde]
pub fn op_base64_decode(#[string] input: String) -> Result<ToJsBuffer, WebError> {
    let mut s = input.into_bytes();
    let decoded_len = forgiving_base64_decode_inplace(&mut s)?;
    s.truncate(decoded_len);
    Ok(s.into())
}

#[op2]
#[serde]
pub fn op_base64_atob(#[serde] mut s: ByteString) -> Result<ByteString, WebError> {
    let decoded_len = forgiving_base64_decode_inplace(&mut s)?;
    s.truncate(decoded_len);
    Ok(s)
}

#[op2]
#[string]
pub fn op_base64_encode(#[buffer] s: &[u8]) -> String {
    forgiving_base64_encode(s)
}

#[op2]
#[string]
pub fn op_base64_btoa(#[serde] s: ByteString) -> String {
    forgiving_base64_encode(s.as_ref())
}

/// See <https://infra.spec.whatwg.org/#forgiving-base64>
#[inline]
fn forgiving_base64_decode_inplace(input: &mut [u8]) -> Result<usize, WebError> {
    let decoded =
        base64_simd::forgiving_decode_inplace(input).map_err(|_| WebError::Base64Decode)?;
    Ok(decoded.len())
}

/// See <https://infra.spec.whatwg.org/#forgiving-base64>
#[inline]
fn forgiving_base64_encode(s: &[u8]) -> String {
    base64_simd::STANDARD.encode_to_string(s)
}
