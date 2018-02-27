//! A basic example showing the request components

extern crate futures;
extern crate gotham;
#[macro_use]
extern crate gotham_derive;
extern crate hyper;
extern crate mime;
extern crate serde_json;

use hyper::{Body, Headers, HttpVersion, Method, Response, StatusCode, Uri};
use futures::{future, stream, Future, Stream};

use gotham::http::response::create_response;
use gotham::state::{FromState, State};
use gotham::router::Router;
use gotham::router::builder::{build_simple_router, DefineSingleRoute, DrawRoutes};
use gotham::handler::{HandlerFuture, IntoHandlerError};
use serde_json::Value;

/// Extract the main elements of the request except for the `Body`
fn print_request_elements(state: &State) {
    let method = Method::borrow_from(state);
    let uri = Uri::borrow_from(state);
    let http_version = HttpVersion::borrow_from(state);
    let headers = Headers::borrow_from(state);
    println!("Method: {:?}", method);
    println!("URI: {:?}", uri);
    println!("HTTP Version: {:?}", http_version);
    println!("Headers: {:?}", headers);
}

#[derive(StateData)]
struct EntryData {
    value: Value,
}

fn entry_handler(state: State) -> Box<HandlerFuture> {
    {
        let entry = EntryData::borrow_from(&state);
        println!("Entry: {:?}", entry.value);
    }
    // HACK: response is created every time the loop iterates
    // and once at the start, even though we only use the last one
    let res = create_response(&state, StatusCode::Ok, None);
    let f = future::ok((state, res));
    Box::new(f)
}

/// Extracts the elements of the POST request and prints them
fn post_handler(mut state: State) -> Box<HandlerFuture> {
    print_request_elements(&state);
    let f = Body::take_from(&mut state)
        .concat2()
        .then(|full_body| -> Box<HandlerFuture> {
            match full_body {
                Ok(valid_body) => {
                    let body_content: Value = serde_json::from_slice(&valid_body).unwrap();
                    println!("Body: {:?}", body_content);
                    if let Value::Array(ref entries) = body_content["entry"] {
                        // HACK: response is created every time the loop iterates
                        // and once at the start, even though we only use the last one
                        let res = create_response(&state, StatusCode::Ok, None);
                        let stream = stream::iter_ok(entries.clone());
                        Box::new(stream.fold((state, res), |(mut state, _), value| {
                            state.put(EntryData { value });
                            entry_handler(state)
                        }))
                    } else {
                        let res = create_response(&state, StatusCode::Ok, None);
                        Box::new(future::ok((state, res)))
                    }
                }
                Err(e) => Box::new(future::err((state, e.into_handler_error()))),
            }
        });

    Box::new(f)
}

/// Show the GET request components by printing them.
fn get_handler(state: State) -> (State, Response) {
    print_request_elements(&state);
    let res = create_response(&state, StatusCode::Ok, None);

    (state, res)
}

/// Create a `Router`
fn router() -> Router {
    build_simple_router(|route| {
        route.associate("/", |assoc| {
            assoc.get().to(get_handler);
            assoc.post().to(post_handler);
        });
    })
}

/// Start a server and use a `Router` to dispatch requests
pub fn main() {
    let addr = "127.0.0.1:7878";
    println!("Listening for requests at http://{}", addr);
    gotham::start(addr, router())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gotham::test::TestServer;

    #[test]
    fn get_request() {
        let test_server = TestServer::new(router()).unwrap();
        let response = test_server
            .client()
            .get("http://localhost")
            .perform()
            .unwrap();

        assert_eq!(response.status(), StatusCode::Ok);
    }

    #[test]
    fn post_request() {
        let test_server = TestServer::new(router()).unwrap();
        let response = test_server
            .client()
            .post(
                "http://localhost",
                r#"{"entry": [1, 2, 3]}"#,
                mime::TEXT_PLAIN,
            )
            .perform()
            .unwrap();

        assert_eq!(response.status(), StatusCode::Ok);
    }
}
