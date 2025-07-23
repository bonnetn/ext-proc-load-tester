use crate::app::error::Result;
use crate::generated::envoy::config::core::v3::{HeaderMap, HeaderValue};
use crate::generated::envoy::service::ext_proc::v3::processing_request::Request;
use crate::generated::envoy::service::ext_proc::v3::{HttpHeaders, ProcessingRequest};

use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error("failed to parse JSON: {0}")]
    FailedToParseJson(serde_json::Error),

    #[error("request headers and response headers cannot both be present")]
    RequestHeadersAndResponseHeadersCannotBothBePresent,
}

pub(crate) fn json_to_processing_request(json: &str) -> Result<ProcessingRequest, Error> {
    let processing_request =
        serde_json::from_str::<json::ProcessingRequest>(json).map_err(Error::FailedToParseJson)?;

    match (
        processing_request.request_headers,
        processing_request.response_headers,
    ) {
        (Some(request_headers), None) => {
            let headers = map_http_headers(request_headers)?;
            Ok(ProcessingRequest {
                request: Some(Request::RequestHeaders(headers)),
                ..Default::default()
            })
        }

        (None, Some(response_headers)) => {
            let headers = map_http_headers(response_headers)?;
            Ok(ProcessingRequest {
                request: Some(Request::ResponseHeaders(headers)),
                ..Default::default()
            })
        }
        _ => {
            return Err(Error::RequestHeadersAndResponseHeadersCannotBothBePresent);
        }
    }
}

fn map_http_headers(data: json::HttpHeaders) -> Result<HttpHeaders, Error> {
    let headers = map_header_map(data.headers)?;
    let end_of_stream = data.end_of_stream;

    Ok(HttpHeaders {
        headers: Some(headers),
        end_of_stream,
        ..Default::default()
    })
}

fn map_header_map(data: json::HeaderMap) -> Result<HeaderMap, Error> {
    let headers = data.headers;

    let headers = headers
        .into_iter()
        .map(map_header_value)
        .collect::<Result<Vec<HeaderValue>, Error>>()?;

    Ok(HeaderMap { headers })
}

fn map_header_value(data: json::HeaderValue) -> Result<HeaderValue, Error> {
    let key = data.key;
    let value = data.value.unwrap_or_default();
    let raw_value = data.raw_value.unwrap_or_default();

    Ok(HeaderValue {
        key,
        value,
        raw_value,
    })
}

mod json {
    use base64::{Engine, engine::general_purpose};
    use serde::{Deserialize, Deserializer};

    #[derive(Deserialize, Debug, Default)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct ProcessingRequest {
        #[serde(default)]
        pub(crate) request_headers: Option<HttpHeaders>,

        #[serde(default)]
        pub(crate) response_headers: Option<HttpHeaders>,
    }

    #[derive(Deserialize, Debug, Default)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct HttpHeaders {
        #[serde(default)]
        pub(crate) headers: HeaderMap,

        #[serde(default)]
        pub(crate) end_of_stream: bool,
    }

    #[derive(Deserialize, Debug, Default)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct HeaderMap {
        #[serde(default)]
        pub(crate) headers: Vec<HeaderValue>,
    }

    #[derive(Deserialize, Debug, Default)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct HeaderValue {
        pub(crate) key: String,

        #[serde(default)]
        pub(crate) value: Option<String>,

        #[serde(default, deserialize_with = "deserialize_raw_value")]
        pub(crate) raw_value: Option<Vec<u8>>,
    }

    fn deserialize_raw_value<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let decoded = general_purpose::STANDARD
            .decode(s)
            .map_err(serde::de::Error::custom)?;
        Ok(Some(decoded))
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use paste::paste;

    macro_rules! happy_path_json_to_processing_request {
        ($($name:ident: $value:expr,)*) => {
        $(
            paste! {
                #[test]
                fn [<test_json_to_processing_request_ $name>]() {
                    let (input, expected) = $value;
                    assert_eq!(expected, json_to_processing_request(input).unwrap());
                }
            }
        )*
        }
    }

    happy_path_json_to_processing_request! {
        with_header_value: (
            r#"
            {
                "request_headers": {
                    "headers": {
                        "headers": [
                            {
                                "key": "Host",
                                "value": "localhost:8080"
                            }
                        ]
                    }
                }
            }
            "#,
            ProcessingRequest {
                request: Some(Request::RequestHeaders(HttpHeaders {
                    headers: Some(HeaderMap {
                        headers: vec![HeaderValue {
                            key: "Host".to_string(),
                            value: "localhost:8080".to_string(),
                            ..Default::default()
                        }],
                    }),
                    end_of_stream: false,
                    ..Default::default()
                })),
                ..Default::default()
            },
        ),
        with_header_raw_value: (
            r#"
            {
                "request_headers": {
                    "headers": {
                        "headers": [
                            {
                                "key": "Host",
                                "raw_value": "bG9jYWxob3N0OjgwODA="
                            }
                        ]
                    }
                }
            }
            "#,
            ProcessingRequest {
                request: Some(Request::RequestHeaders(HttpHeaders {
                    headers: Some(HeaderMap {
                        headers: vec![HeaderValue {
                            key: "Host".to_string(),
                            raw_value: b"localhost:8080".to_vec(),
                            ..Default::default()
                        }],
                    }),
                    end_of_stream: false,
                    ..Default::default()
                })),
                ..Default::default()
            },
        ),
    }

    macro_rules! error_json_to_processing_request {
        ($($name:ident: $value:expr,)*) => {
        $(
            paste! {
                #[test]
                fn [<test_json_to_processing_request_error_ $name>]() {
                    let input = $value;
                    assert!(json_to_processing_request(input).is_err());
                }
            }
        )*
        }
    }

    error_json_to_processing_request! {
        invalid_json: r#"invalid json"#,
        request_headers_and_response_headers_cannot_both_be_present: r#"
        {
            "request_headers": {},
            "response_headers": {}
        }
        "#,
        no_headers_set: r#"{}"#,
    }
}
