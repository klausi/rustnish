use crate::cache::LruCache;
use crate::cache::MemorySizable;
use crate::errors::ResultExt;
use crate::errors::*;
use error_chain::bail;
use futures::{Future, Stream};
use http::Method;
use hyper::client::HttpConnector;
use hyper::header::HeaderName;
use hyper::header::{HeaderValue, CACHE_CONTROL, COOKIE, SERVER, VIA};
use hyper::server::conn::Http;
use hyper::service::Service;
use hyper::Client;
use hyper::StatusCode;
use hyper::Version;
use hyper::{Body, HeaderMap, Request, Response};
use regex::Regex;
use std::mem::size_of_val;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::runtime::Runtime;

mod cache;

mod errors {
    use error_chain::*;

    // Create the Error, ErrorKind, ResultExt, and Result types
    error_chain! {}
}

struct Proxy {
    port: u16,
    upstream_port: u16,
    client: Client<HttpConnector>,
    // The socket address the original request is coming from.
    source_address: SocketAddr,
    cache: Cache,
}

impl Service for Proxy {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = hyper::Error;
    type Future = Box<Future<Item = Response<Body>, Error = hyper::Error> + Send>;

    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        let cache_key = self.cache.cache_key(&request);

        if let Some(response) = self.cache.lookup(&cache_key) {
            return Box::new(futures::future::ok(response));
        }

        let upstream_uri = {
            // 127.0.0.1 is hard coded here for now because we assume that upstream
            // is on the same host. Should be made configurable later.
            let mut upstream_uri = format!(
                "http://127.0.0.1:{}{}",
                self.upstream_port,
                request.uri().path()
            );
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
                    return Box::new(futures::future::ok(
                        Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .body("Invalid upstream URI".into())
                            .unwrap(),
                    ));
                }
            }
        };

        *request.uri_mut() = upstream_uri;

        {
            let headers = request.headers_mut();
            headers.append(
                HeaderName::from_static("x-forwarded-for"),
                self.source_address.ip().to_string().parse().unwrap(),
            );
            headers.append(
                HeaderName::from_static("x-forwarded-port"),
                self.port.to_string().parse().unwrap(),
            );
        }

        let mut cloned_cache = self.cache.clone();

        Box::new(self.client.request(request).then(move |result| {
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

                        headers.append(VIA, format!("{} rustnish-0.0.1", version).parse().unwrap());

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
            futures::future::ok(our_response)
        }))
    }
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
                        let body_bytes = body.concat2().wait().unwrap().to_vec();

                        let mut inner_cache = self.lru_cache.lock().unwrap();
                        let entry = CachedResponse {
                            status: header_part.status,
                            version: header_part.version,
                            headers: header_part.headers.clone(),
                            body: body_bytes.clone(),
                        };
                        // Store an expiry date for this repsponse. After
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

pub fn start_server_blocking(port: u16, upstream_port: u16) -> Result<()> {
    let runtime = start_server_background(port, upstream_port)
        .chain_err(|| "Spawning server thread failed")?;

    runtime.shutdown_on_idle().wait().unwrap();

    bail!("The server thread finished unexpectedly");
}

pub fn start_server_background(port: u16, upstream_port: u16) -> Result<Runtime> {
    // 256 MB memory cahce as a default.
    start_server_background_memory(port, upstream_port, 256 * 1024 * 1024)
}

pub fn start_server_background_memory(
    port: u16,
    upstream_port: u16,
    memory_size: usize,
) -> Result<Runtime> {
    let address: SocketAddr = ([127, 0, 0, 1], port).into();
    let mut runtime = Runtime::new().unwrap();

    // We can't use Http::new().bind() because we need to pass down the
    // remote source IP address to our proxy service. So we need to
    // create a TCP listener ourselves and handle each connection to
    // have access to the source IP address.
    // @todo Simplify this once Hyper has a better API to handle IP
    // addresses.
    let listener = TcpListener::bind(&address)
        .chain_err(|| format!("Failed to bind server to address {}", address))?;
    let client = Client::new();
    let http = Http::new();

    let inner_cache = LruCache::<String, CachedResponse>::with_memory_size(memory_size);
    let cache = Cache {
        lru_cache: Arc::new(Mutex::new(inner_cache)),
    };

    let server = listener
        .incoming()
        .for_each(move |socket| {
            let source_address = socket.peer_addr().unwrap();
            tokio::spawn(
                http.serve_connection(
                    socket,
                    Proxy {
                        port,
                        upstream_port,
                        client: client.clone(),
                        source_address,
                        cache: cache.clone(),
                    },
                )
                .map(|_| ())
                .map_err(|_| ()),
            );
            Ok(())
        })
        .map_err(|e| panic!("accept error: {}", e));

    println!("Listening on http://{}", address);
    runtime.spawn(server);

    Ok(runtime)
}

#[cfg(test)]
mod tests {

    use crate::cache::MemorySizable;
    use hyper::header::HeaderValue;
    use hyper::{HeaderMap, StatusCode, Version};
    use crate::CachedResponse;

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
