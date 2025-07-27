use crate::app::error::Error;
use crate::app::error::Result;
use crate::generated::envoy::config::core::v3::{HeaderMap, HeaderValue};
use crate::generated::envoy::service::ext_proc::v3::processing_request::Request;
use crate::generated::envoy::service::ext_proc::v3::{HttpHeaders, ProcessingRequest};

impl TryInto<ProcessingRequest> for json::ProcessingRequest {
    type Error = Error;

    fn try_into(self) -> Result<ProcessingRequest, Self::Error> {
        match (self.request_headers, self.response_headers) {
            (Some(request_headers), None) => {
                let headers = map_http_headers(request_headers);
                Ok(ProcessingRequest {
                    request: Some(Request::RequestHeaders(headers)),
                    ..Default::default()
                })
            }

            (None, Some(response_headers)) => {
                let headers = map_http_headers(response_headers);
                Ok(ProcessingRequest {
                    request: Some(Request::ResponseHeaders(headers)),
                    ..Default::default()
                })
            }
            _ => Err(Error::ExactlyOneOfRequestHeadersOrResponseHeadersMustBePresent),
        }
    }
}

fn map_http_headers(data: json::HttpHeaders) -> HttpHeaders {
    let headers = map_header_map(data.headers);
    let end_of_stream = data.end_of_stream;

    HttpHeaders {
        headers: Some(headers),
        end_of_stream,
        ..Default::default()
    }
}

fn map_header_map(data: json::HeaderMap) -> HeaderMap {
    let headers = data.headers;

    let headers = headers
        .into_iter()
        .map(map_header_value)
        .collect::<Vec<HeaderValue>>();

    HeaderMap { headers }
}

fn map_header_value(data: json::HeaderValue) -> HeaderValue {
    let key = data.key;
    let value = data.value.unwrap_or_default();
    let raw_value = data.raw_value.unwrap_or_default();

    HeaderValue {
        key,
        value,
        raw_value,
    }
}

// NOTE: Until Prost supports deserializing JSON protobuf, we are definining a subset of the model using serde JSON
// and bridge the two libraries manually.
// This is to not import an additional 3rd party library.
pub(crate) mod json {
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
                    let dto: json::ProcessingRequest = serde_json::from_str(input).unwrap();
                    let result: Result<ProcessingRequest, Error> = dto.try_into();
                    assert_eq!(expected, result.unwrap());
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
                    let dto: json::ProcessingRequest = serde_json::from_str(input).unwrap();
                    let result: Result<ProcessingRequest, Error> = dto.try_into();
                    assert!(result.is_err());
                }
            }
        )*
        }
    }

    error_json_to_processing_request! {
        request_headers_and_response_headers_cannot_both_be_present: r#"
        {
            "request_headers": {},
            "response_headers": {}
        }
        "#,
        no_headers_set: "{}",
    }
}
