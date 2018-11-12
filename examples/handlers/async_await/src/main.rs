//! An example showing the request components implemented using
//! async and await!()
#![feature(async_await, futures_api, await_macro, trait_alias)]

extern crate futures;
extern crate gotham;
#[macro_use]
extern crate gotham_derive;
extern crate hyper;
extern crate mime;
extern crate serde;
#[macro_use]
extern crate serde_derive;
// This is subtle and quite important. We #[macro_use] tokio, with 
// tokio = {version = "0.1", features = ["async-await-preview"] }
// in our Cargo.toml. This gives us a version of await!() that
// happily accepts both std::Future and futures::Future.
#[macro_use]
extern crate tokio;
extern crate tokio_async_await;

use futures::Future;
use std::time::{Duration, Instant};

use hyper::StatusCode;

use gotham::handler::{HandlerError, HandlerFuture, IntoHandlerError};
use gotham::helpers::http::response::create_response;
use gotham::router::builder::DefineSingleRoute;
use gotham::router::builder::{build_simple_router, DrawRoutes};
use gotham::router::Router;
use gotham::state::{FromState, State};

use tokio::timer::Delay;
use tokio_async_await::compat::backward;

/// We define a trait alias for the thing that sleep() returns, below.
/// This avoids a Box::new() compared to simple_async_handlers.
trait SleepFuture = Future<Item = Vec<u8>, Error = HandlerError> + Send;

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct QueryStringExtractor {
    seconds: u64,
}

/// Sneaky hack to make tests take less time. Nothing to see here ;-).
#[cfg(not(test))]
fn get_duration(seconds: &u64) -> Duration {
    Duration::from_secs(seconds.to_owned())
}
#[cfg(test)]
fn get_duration(seconds: &u64) -> Duration {
    Duration::from_millis(seconds.to_owned())
}
/// All this function does is return a future that resolves after a number of
/// seconds, with a Vec<u8> that tells you how long it slept for.
///
/// It is exactly the same function that you see in the `simple_async_handers`
/// example, and returns a futures::Future.
///
/// Most things that you call to access remote services (e.g databases and
/// web apis) can be coerced into returning futures that yield useful data,
/// so the patterns that you learn in this example should be applicable to
/// real world problems.
fn sleep(seconds: u64) -> impl SleepFuture {
    let when = Instant::now() + get_duration(&seconds);
    let delay = Delay::new(when)
        .map_err(|e| panic!("timer failed; err={:?}", e))
        .and_then(move |_| {
            Ok(format!("slept for {} seconds\n", seconds)
                .as_bytes()
                .to_vec())
        });

    delay
}

/// This handler sleeps for the requested number of seconds, using the `sleep()`
/// helper method, above.
/// 
/// Notice that we don't impliment this as an `async fn` directly as our Handler
/// function, because it has the wrong type signature.
/// The return value from `async fn blah() -> Whatever` gets translated into:
///     std::Future<Output = Whatever>
/// and we want:
///     Box<HandlerFuture>
/// where HandlerFuture is:
///     futures::Future<Item = (State, Response<Body>), Error = (State, HandlerError)> + Send
/// 
/// Luckily, tokio_async_await::compat::backwards::Compat::new(f) can help us here.
/// It accepts a std::Future<Output = Result<Item, Error>> and converts it into
/// something that implements futures::Future<Item = Item, Error = Error>.
/// 
/// Let's see how all of these pieces of the puzzle fit together:
fn sleep_handler(mut state: State) -> Box<HandlerFuture> {
    // f is a std::Future<Output = Result<(State, Response<Body>), (State, HandlerError)>
    let f = async move {
        let seconds = QueryStringExtractor::take_from(&mut state).seconds;
        println!("sleep for {} seconds once: starting", seconds);

        // Here, we call our helper function that returns a future, and immediately
        // await its result
        let sleep_result = await!(sleep(seconds.clone()));

        // Here, we convert the Result from `sleep()` into the form that Gotham expects.
        // We have to use .then() rather than .and_then() because we need to coerce both
        // the success and error cases into the right shape.
        // `state` is moved in, so that we can return it, and we convert any errors
        // that we have into the form that Hyper expects, using the helper from
        // IntoHandlerError.
        match sleep_result {
            Ok(data) => {
                let res = create_response(&state, StatusCode::OK, mime::TEXT_PLAIN, data);
                println!("sleep for {} seconds once: finished", seconds);
                Ok((state, res))
            }
            Err(err) => Err((state, err.into_handler_error())),
        }
    };
    Box::new(backward::Compat::new(f))
}

macro_rules! handler_try {
    ($state: expr, $expr:expr) => (match $expr {
        Ok(val) => val,
        Err(err) => {
            return Err(($state, From::from(err)))
        }
    });
    ($expr:expr,) => (try!($expr));
}

/// This example uses a `future::Stream` to implement a `for` loop. It calls sleep(1)
/// as many times as needed to make the requested duration.
///
/// https://github.com/alexcrichton/futures-await has a more readable syntax for
/// async for loops, if you are using nightly Rust.
fn loop_handler(mut state: State) -> Box<HandlerFuture> {
    // Do the sleep(), and append the result to the accumulator so that it can
    // be returned.
    let f = async move {
        let seconds = QueryStringExtractor::take_from(&mut state).seconds;
        println!("sleep for one second {} times: starting", seconds);

        let mut accumulator = Vec::new();
        for _ in 0..seconds {
            // You could do this using the following match expression:
            //     let sleep_result = await!(sleep(1));
            //     match sleep_result {
            //         Ok(body) => {
            //             accumulator.extend(body);
            //         },
            //         Err(err) => {
            //             return Err((state, err));
            //         }
            //     };
            // but this quickly gets boring, so we use a macro instead:
            let text = handler_try!(state, await!(sleep(1)));

            accumulator.extend(text);
        }
        println!("sleep for one second {} times: finished", seconds);
        // Error cases are all handled above, so we only need to translate the response here.
        let res = create_response(&state, StatusCode::OK, mime::TEXT_PLAIN, accumulator);
        Ok((state, res))
    };

    Box::new(backward::Compat::new(f))
}

/// Create a `Router`.
fn router() -> Router {
    build_simple_router(|route| {
        route
            .get("/sleep")
            .with_query_string_extractor::<QueryStringExtractor>()
            .to(sleep_handler);
        ;
        route
            .get("/loop")
            .with_query_string_extractor::<QueryStringExtractor>()
            .to(loop_handler);
        ;
    })
}

/// Start a server and use a `Router` to dispatch requests.
pub fn main() {
    let addr = "127.0.0.1:7878";
    println!("Listening for requests at http://{}", addr);
    gotham::start(addr, router())
}

#[cfg(test)]
mod tests {
    use gotham::test::TestServer;

    use super::*;

    fn assert_returns_ok(url_str: &str, expected_response: &str) {
        let test_server = TestServer::new(router()).unwrap();
        let response = test_server.client().get(url_str).perform().unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            &String::from_utf8(response.read_body().unwrap()).unwrap(),
            expected_response
        );
    }

    #[test]
    fn sleep_says_how_long_it_slept_for() {
        assert_returns_ok("http://localhost/sleep?seconds=2", "slept for 2 seconds\n");
    }

    #[test]
    fn loop_breaks_the_time_into_one_second_sleeps() {
        assert_returns_ok(
            "http://localhost/loop?seconds=2",
            "slept for 1 seconds\nslept for 1 seconds\n",
        );
    }
}
