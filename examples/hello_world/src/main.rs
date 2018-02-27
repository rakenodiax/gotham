//! A Hello World example application for working with Gotham.

extern crate futures;
extern crate gotham;
extern crate hyper;
extern crate mime;
extern crate tokio_core;

use hyper::StatusCode;
use hyper::client::Client;

use gotham::http::response::create_response;
use gotham::state::State;
use gotham::handler::HandlerFuture;
use tokio_core::reactor::Handle;
use futures::{future, Future};
/// Create a `Handler` which is invoked when responding to a `Request`.
///
/// How does a function become a `Handler`?.
/// We've simply implemented the `Handler` trait, for functions that match the signature used here,
/// within Gotham itself.
fn say_hello(state: State) -> Box<HandlerFuture> {
    let work = {
        let handle = state.borrow::<Handle>();
        let client = Client::new(&handle);

        client
            .get("http://httpbin.org/get".parse().unwrap())
            .and_then(|res| {
                println!("Response: {}", res.status());
                future::ok(b"ui".to_vec())
            })
    };

    let work = work.then(|_| {
        let resp = create_response(&state, StatusCode::Ok, Some((vec![], mime::TEXT_PLAIN)));
        future::ok((state, resp))
    });

    Box::new(work)
}

/// Start a server and call the `Handler` we've defined above for each `Request` we receive.
pub fn main() {
    let addr = "127.0.0.1:7878";
    println!("Listening for requests at http://{}", addr);
    gotham::start(addr, || Ok(say_hello))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gotham::test::TestServer;

    #[test]
    fn receive_hello_world_response() {
        let test_server = TestServer::new(|| Ok(say_hello)).unwrap();
        let response = test_server
            .client()
            .get("http://localhost")
            .perform()
            .unwrap();

        assert_eq!(response.status(), StatusCode::Ok);

        let body = response.read_body().unwrap();
        assert_eq!(&body[..], b"Hello World!");
    }
}
