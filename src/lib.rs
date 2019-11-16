use crate::cache::LruCache;
use crate::cache::MemorySizable;
use crate::errors::ResultExt;
use crate::errors::*;
use error_chain::bail;
#[cfg(test)]
use fake_clock::FakeClock as Instant;
use futures::executor::block_on;
use futures_util::try_stream::TryStreamExt;
use http::Method;
use hyper::header::HeaderName;
use hyper::header::{HeaderValue, CACHE_CONTROL, COOKIE, SERVER, VIA};
use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use hyper::StatusCode;
use hyper::Version;
use hyper::{Body, HeaderMap, Request, Response, Result};
use hyper::{Client, Error, Server};
use regex::Regex;
use std::mem::size_of_val;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
#[cfg(not(test))]
use std::time::Instant;

mod cache;

mod errors {
    use error_chain::*;

    // Create the Error, ErrorKind, ResultExt, and Result types
    error_chain! {}
}

struct CachedResponse {
    status: StatusCode,
    version: Version,
    headers: HeaderMap<HeaderValue>,
    body: Vec<u8>,
}

/// Calculates the memory space that is used up by a cached HTTP response.
/// This is an imprecise approximation.
impl MemorySizable for CachedResponse {
    fn get_memory_size(&self) -> usize {
        // Memory usage of the struct itself.
        let mut memory_size = size_of_val(self);

        // Memory usage of the header key value pairs.
        for (key, value) in self.headers.iter() {
            memory_size += key.as_str().as_bytes().len() + value.len();
        }
        // Memory usage of the body bytes.
        memory_size += self.body.capacity();

        memory_size
    }
}

#[derive(Clone)]
struct Cache {
    lru_cache: Arc<Mutex<LruCache<String, CachedResponse>>>,
}

impl Cache {
    /// Convert an incoming request into a cache key that we can then lookup.
    fn cache_key(&self, request: &Request<Body>) -> Option<String> {
        // Only GET requests are cachable.
        if request.method() != Method::GET {
            return None;
        }
        // Requests with a session cookie cannot be cached.
        if let Some(cookie_header) = request.headers().get(COOKIE) {
            if let Ok(cookie_string) = cookie_header.to_str() {
                let regex = Regex::new(r"SESS[A-Za-z0-9_]+=").unwrap();
                if regex.is_match(cookie_string) {
                    return None;
                }
            }
        }
        Some(request.uri().to_string())
    }

    /// Check if we have a response for this request in memory.
    fn lookup(&mut self, cache_key: &Option<String>) -> Option<Response<Body>> {
        match cache_key {
            None => None,
            Some(cache_key) => {
                let mut inner_cache = self.lru_cache.lock().unwrap();
                match inner_cache.get(cache_key) {
                    Some(entry) => {
                        let mut response = Response::builder()
                            .status(entry.status)
                            .version(entry.version)
                            .body(Body::from(entry.body.clone()))
                            .unwrap();
                        *response.headers_mut() = entry.headers.clone();
                        Some(response)
                    }
                    None => None,
                }
            }
        }
    }

    // @todo should we take the cache key as option or not?
    fn store(&mut self, cache_key: Option<String>, response: Response<Body>) -> Response<Body> {
        match cache_key {
            None => response,
            Some(key) => {
                // Only cache the response if it has a max-age.
                match self.get_max_age(&response) {
                    None => response,
                    Some(max_age) => {
                        // In order to be able to cache the response we have to fully
                        // consume it, clone it and rebuild it. Super ugly, any better
                        // ideas?
                        let (header_part, body) = response.into_parts();
                        let body_bytes = response.body_mut().try_concat();

                        let mut inner_cache = self.lru_cache.lock().unwrap();
                        let entry = CachedResponse {
                            status: header_part.status,
                            version: header_part.version,
                            headers: header_part.headers.clone(),
                            body: body_bytes.clone(),
                        };
                        // Store an expiry date for this response. After
                        // that point in time we need to discard it.
                        inner_cache.insert(
                            key,
                            entry,
                            Instant::now() + Duration::from_secs(max_age),
                        );

                        Response::from_parts(header_part, Body::from(body_bytes))
                    }
                }
            }
        }
    }

    fn get_max_age(&self, response: &Response<Body>) -> Option<u64> {
        let mut public = false;
        let mut max_age: u64 = 0;
        {
            // Make sure that the response is cachable.
            let cache_control = response.headers().get_all(CACHE_CONTROL);
            for header_value in cache_control {
                if let Ok(header_string) = header_value.to_str() {
                    let comma_values = header_string.split(',');
                    for comma_value in comma_values {
                        if comma_value == "public" {
                            public = true;
                            continue;
                        }
                        let equal_value: Vec<&str> = comma_value.split('=').collect();
                        if equal_value.len() == 2 && equal_value[0] == "max-age" {
                            max_age = match equal_value[1].parse() {
                                Ok(value) => value,
                                Err(_) => 0,
                            };
                        }
                    }
                }
            }
        }

        if public && max_age > 0 {
            return Some(max_age);
        }
        None
    }
}

pub fn start_server_blocking(port: u16, upstream_port: u16) {
    // 256 MB memory cache as a default.
    start_server_background_memory(port, upstream_port, 256 * 1024 * 1024);
}

pub async fn start_server_background_memory(
    port: u16,
    upstream_port: u16,
    memory_size: usize,
) -> Result<()> {
    let address: SocketAddr = ([127, 0, 0, 1], port).into();

    let client_main = Client::new();

    let inner_cache = LruCache::<String, CachedResponse>::with_memory_size(memory_size);
    let cache_main = Cache {
        lru_cache: Arc::new(Mutex::new(inner_cache)),
    };

    // The closure inside `make_service_fn` is run for each connection,
    // creating a 'service' to handle requests for that specific connection.
    let make_service = make_service_fn(move |socket: &AddrStream| {
        let remote_addr = socket.remote_addr();
        let client = client_main.clone();
        let cache = cache_main.clone();

        async move {
            // This is the `Service` that will handle the connection.
            // `service_fn` is a helper to convert a function that
            // returns a Response into a `Service`.
            Ok::<_, Error>(service_fn(move |mut request: Request<Body>| {
                async move {
                    let cache_key = cache.cache_key(&request);

                    if let Some(response) = cache.lookup(&cache_key) {
                        return Ok(response);
                    }

                    let upstream_uri = {
                        // 127.0.0.1 is hard coded here for now because we assume that upstream
                        // is on the same host. Should be made configurable later.
                        let mut upstream_uri =
                            format!("http://127.0.0.1:{}{}", upstream_port, request.uri().path());
                        if let Some(query) = request.uri().query() {
                            upstream_uri.push('?');
                            upstream_uri.push_str(query);
                        }
                        match upstream_uri.parse() {
                            Ok(u) => u,
                            _ => {
                                // We can't actually test this because parsing the URI never
                                // fails. However, should that change at any point this is the
                                // right thing to do.
                                return Ok(Response::builder()
                                    .status(StatusCode::BAD_REQUEST)
                                    .body("Invalid upstream URI".into())
                                    .unwrap());
                            }
                        }
                    };

                    *request.uri_mut() = upstream_uri;

                    {
                        let headers = request.headers_mut();
                        headers.append(
                            HeaderName::from_static("x-forwarded-for"),
                            remote_addr.ip().to_string().parse().unwrap(),
                        );
                        headers.append(
                            HeaderName::from_static("x-forwarded-port"),
                            port.to_string().parse().unwrap(),
                        );
                    }

                    let mut cloned_cache = cache.clone();

                    let result = client.request(request).await;
                    let our_response = match result {
                        Ok(mut response) => {
                            let version = match response.version() {
                                Version::HTTP_09 => "0.9",
                                Version::HTTP_10 => "1.0",
                                Version::HTTP_11 => "1.1",
                                Version::HTTP_2 => "2.0",
                            };
                            {
                                let headers = response.headers_mut();

                                headers.append(
                                    VIA,
                                    format!("{} rustnish-0.0.1", version).parse().unwrap(),
                                );

                                // Append a "Server" header if not already present.
                                if !headers.contains_key(SERVER) {
                                    headers.insert(SERVER, "rustnish".parse().unwrap());
                                }
                            }

                            // Put the response into the cache if possible.
                            cloned_cache.store(cache_key, response)
                        }
                        Err(_) => {
                            // For security reasons do not show the exact error to end users.
                            // @todo Log the error.
                            Response::builder()
                                .status(StatusCode::BAD_GATEWAY)
                                .body("Something went wrong, please try again later.".into())
                                .unwrap()
                        }
                    };
                    Ok::<_, Error>(our_response)
                }
            }))
        }
    });

    let server = Server::bind(&address).serve(make_service);

    println!("Listening on http://{}", address);

    server.await
}

#[cfg(test)]
mod tests {

    use crate::cache::MemorySizable;
    use crate::CachedResponse;
    use hyper::header::HeaderValue;
    use hyper::{HeaderMap, StatusCode, Version};

    fn example_cache_entry() -> CachedResponse {
        CachedResponse {
            status: StatusCode::OK,
            version: Version::HTTP_11,
            headers: HeaderMap::new(),
            body: "a".into(),
        }
    }

    #[test]
    fn cache_memory_size() {
        let cache_entry = example_cache_entry();
        assert_eq!(129, cache_entry.get_memory_size());
    }

    #[test]
    fn body_100_bytes() {
        let mut cache_entry = example_cache_entry();
        cache_entry.body = vec![b'a'; 100];
        assert_eq!(228, cache_entry.get_memory_size());
    }

    #[test]
    fn one_header_size() {
        let mut cache_entry = example_cache_entry();
        cache_entry
            .headers
            .insert("a", HeaderValue::from_static("b"));
        assert_eq!(131, cache_entry.get_memory_size());
    }
}
