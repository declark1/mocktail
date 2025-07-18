//! Mock request
use url::Url;

use crate::{body::Body, headers::Headers};

/// Represents a HTTP request.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub method: Method,
    pub url: Url,
    pub headers: Headers,
    pub body: Body,
}

impl Request {
    pub fn new(method: Method, url: Url) -> Self {
        Self {
            method,
            url,
            headers: Headers::default(),
            body: Body::default(),
        }
    }

    pub fn from_parts(parts: http::request::Parts) -> Self {
        let url: Url = if parts.uri.authority().is_some() {
            parts.uri.to_string()
        } else {
            format!("http://localhost{}", parts.uri)
        }
        .parse()
        .unwrap();
        Self {
            method: parts.method.into(),
            url,
            headers: parts.headers.into(),
            body: Body::default(),
        }
    }

    pub fn with_headers(mut self, headers: Headers) -> Self {
        self.headers = headers;
        self
    }

    pub fn with_body(mut self, body: impl Into<Body>) -> Self {
        self.body = body.into();
        self
    }

    pub fn method(&self) -> &Method {
        &self.method
    }

    pub fn url(&self) -> &Url {
        &self.url
    }

    pub fn path(&self) -> &str {
        self.url.path()
    }

    pub fn query(&self) -> Option<&str> {
        self.url.query()
    }

    pub fn query_pairs(&self) -> url::form_urlencoded::Parse<'_> {
        self.url.query_pairs()
    }

    pub fn headers(&self) -> &Headers {
        &self.headers
    }

    pub fn body(&self) -> &Body {
        &self.body
    }
}

/// Represents a HTTP method.
#[allow(clippy::upper_case_acronyms)]
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Method {
    #[default]
    GET,
    HEAD,
    POST,
    PUT,
    DELETE,
    CONNECT,
    OPTIONS,
    TRACE,
    PATCH,
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

impl std::str::FromStr for Method {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_uppercase().as_str() {
            "GET" => Ok(Method::GET),
            "HEAD" => Ok(Method::HEAD),
            "POST" => Ok(Method::POST),
            "PUT" => Ok(Method::PUT),
            "DELETE" => Ok(Method::DELETE),
            "CONNECT" => Ok(Method::CONNECT),
            "OPTIONS" => Ok(Method::OPTIONS),
            "TRACE" => Ok(Method::TRACE),
            "PATCH" => Ok(Method::PATCH),
            _ => Err(format!("Invalid HTTP method {value}")),
        }
    }
}

impl TryFrom<&str> for Method {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "POST" => Ok(Self::POST),
            "GET" => Ok(Self::GET),
            "HEAD" => Ok(Self::HEAD),
            "PUT" => Ok(Self::PUT),
            "DELETE" => Ok(Self::DELETE),
            "CONNECT" => Ok(Self::CONNECT),
            "OPTIONS" => Ok(Self::OPTIONS),
            "TRACE" => Ok(Self::TRACE),
            "PATCH" => Ok(Self::PATCH),
            _ => Err(format!("Invalid HTTP method {value}")),
        }
    }
}

impl From<http::Method> for Method {
    fn from(value: http::Method) -> Self {
        match value.as_str() {
            "GET" => Self::GET,
            "HEAD" => Self::HEAD,
            "POST" => Self::POST,
            "PUT" => Self::PUT,
            "DELETE" => Self::DELETE,
            "CONNECT" => Self::CONNECT,
            "OPTIONS" => Self::OPTIONS,
            "TRACE" => Self::TRACE,
            "PATCH" => Self::PATCH,
            _ => unimplemented!(),
        }
    }
}
