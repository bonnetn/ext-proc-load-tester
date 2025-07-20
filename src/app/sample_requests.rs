pub(crate) mod request_headers {
    use crate::generated::envoy::{
        config::core::v3::{HeaderMap, HeaderValue},
        service::ext_proc::v3::{HttpHeaders, ProcessingRequest, processing_request::Request},
    };

    pub(crate) fn create_processing_request() -> ProcessingRequest {
        ProcessingRequest {
            request: Some(Request::RequestHeaders(create_http_headers())),
            ..Default::default()
        }
    }

    fn create_http_headers() -> HttpHeaders {
        HttpHeaders {
            headers: Some(create_header_map()),
            ..Default::default()
        }
    }

    fn create_header_map() -> HeaderMap {
        HeaderMap {
            headers: vec![create_header_value()],
        }
    }

    fn create_header_value() -> HeaderValue {
        HeaderValue {
            key: "test".to_string(),
            raw_value: vec![],
            ..Default::default()
        }
    }
}

pub(crate) mod response_headers {
    use crate::generated::envoy::{
        config::core::v3::{HeaderMap, HeaderValue},
        service::ext_proc::v3::{HttpHeaders, ProcessingRequest, processing_request::Request},
    };

    pub(crate) fn create_processing_request() -> ProcessingRequest {
        ProcessingRequest {
            request: Some(Request::ResponseHeaders(create_http_headers())),
            ..Default::default()
        }
    }

    fn create_http_headers() -> HttpHeaders {
        HttpHeaders {
            headers: Some(create_header_map()),
            ..Default::default()
        }
    }

    fn create_header_map() -> HeaderMap {
        HeaderMap {
            headers: vec![create_header_value()],
        }
    }

    fn create_header_value() -> HeaderValue {
        HeaderValue {
            key: "test".to_string(),
            raw_value: vec![],
            ..Default::default()
        }
    }
}
