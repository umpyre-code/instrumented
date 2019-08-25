//! # Instrumented
//!
//! `instrumented` provides an attribute macro that enables instrumentation of
//! functions for use with Prometheus.
//!
//! This crate is largely based on the [`log-derive`](https://docs.rs/log-derive/) crate, and
//! inspired by the [`metered`](https://docs.rs/metered/) crate.
//!
//! To get started, add the [`instrumented_codegen::instrument`] proc macro to any function you
//! want to instrument. Have a look in the `example` directory for a full usage.
//!
//! ## Configuring
//!
//! You can specify the global metrics prefix with the `METRICS_PREFIX` env var,
//! and provide default labels with the `METRICS_LABELS` env var, which accepts a
//! command separated list of `label=value` pairs. For example:
//!
//! ```shell
//! METRICS_PREFIX=myapp
//! METRICS_LABELS=app=myapp,env=prod,region=us
//! ```
//!
//! ## Example
//!
//! ```rust
//! extern crate instrumented;
//! extern crate log;
//! extern crate reqwest;
//!
//! use instrumented::instrument;
//!
//! // Logs at warn level with the `special` context.
//! #[instrument(WARN, ctx = "special")]
//! fn my_func() {
//!     use std::{thread, time};
//!     let ten_millis = time::Duration::from_millis(10);
//!     thread::sleep(ten_millis);
//! }
//!
//! #[derive(Debug)]
//! pub struct MyError;
//!
//! // Logs result at info level
//! #[instrument(INFO)]
//! fn my_func_with_ok_result() -> Result<String, MyError> {
//!     use std::{thread, time};
//!     let ten_millis = time::Duration::from_millis(10);
//!     thread::sleep(ten_millis);
//!
//!     Ok(String::from("hello world"))
//! }
//!
//! // Logs result at debug level
//! #[instrument(DEBUG)]
//! fn my_func_with_err_result() -> Result<String, MyError> {
//!     use std::{thread, time};
//!     let ten_millis = time::Duration::from_millis(10);
//!     thread::sleep(ten_millis);
//!
//!     Err(MyError)
//! }
//!
//! fn main() {
//!     let addr = "127.0.0.1:5000".to_string();
//!     instrumented::init(&addr);
//!
//!     my_func();
//!     assert_eq!(my_func_with_ok_result().is_ok(), true);
//!     assert_eq!(my_func_with_err_result().is_err(), true);
//!
//!     let body = reqwest::get(&format!("http://{}/metrics", addr))
//!         .unwrap()
//!         .text()
//!         .unwrap();
//!
//!     println!("{}", body);
//! }
//! ```
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate hyper;
#[allow(unused_imports)]
#[macro_use]
extern crate instrumented_codegen;

/// Codegen crate
pub use instrumented_codegen::instrument;

/// `rust-prometheus` crate
pub mod prometheus {
    extern crate prometheus;
    /// `rust-prometheus` crate
    pub use self::prometheus::*;
}

use hyper::http::StatusCode;
use hyper::rt::Future;
use hyper::service::service_fn_ok;
use hyper::{Body, Request, Response, Server};

#[cfg(all(target_os = "linux"))]
fn register_default_process_collector(
    reg: &crate::prometheus::Registry,
) -> crate::prometheus::Result<()> {
    use crate::prometheus::process_collector::ProcessCollector;

    let pc = ProcessCollector::for_self();
    reg.register(Box::new(pc))
}

lazy_static! {
    static ref DEFAULT_REGISTRY: ::prometheus::Registry = {
        use std::collections::HashMap;

        let prefix =  match std::env::var("METRICS_PREFIX")  {
            Ok(value) => Some(value.to_string()),
            Err(_) => None,
        };
        let labels =  match std::env::var("METRICS_LABELS")  {
            Ok(value) => {
                let mut labels = HashMap::new();
                value.split(',').for_each(|s| {let v: Vec<&str> = s.splitn(2, '=').collect();
                if v.len() ==2 {
                    labels.insert(v[0].to_owned(), v[1].to_owned());
                }});
                Some(labels)
            },
            Err(_) => None,
        };

        #[allow(clippy::let_and_return)]
        let reg = ::prometheus::Registry::new_custom(prefix, labels).unwrap();

        // Register a default process collector.
        #[cfg(all(target_os = "linux"))]
        register_default_process_collector(&reg).unwrap();

        reg
    };
    static ref FUNC_CALLED: prometheus::IntCounterVec = {
        let counter_opts = prometheus::Opts::new(
            "function_called_total",
            "Number of times a function was called",
        );
        let counter = prometheus::IntCounterVec::new(counter_opts, &["type","name","ctx"]).unwrap();

        DEFAULT_REGISTRY
            .register(Box::new(counter.clone())).unwrap();

        counter
    };
    static ref FUNC_ERRORS: prometheus::IntCounterVec = {
        let counter_opts = prometheus::Opts::new(
            "function_error_total",
            "Number of times the result of a function was an error",
        );
        let counter = prometheus::IntCounterVec::new(counter_opts, &["type","name","ctx","err"]).unwrap();

        DEFAULT_REGISTRY
            .register(Box::new(counter.clone())).unwrap();

        counter
    };
    static ref FUNC_TIMER: prometheus::HistogramVec = {
        let histogram_opts = prometheus::HistogramOpts::new(
            "function_time_seconds",
            "Histogram of function call times observed",
        );
        let histogram = prometheus::HistogramVec::new(histogram_opts, &["type","name","ctx"]).unwrap();

        DEFAULT_REGISTRY
            .register(Box::new(histogram.clone())).unwrap();

        histogram
    };
    static ref FUNC_INFLIGHT: prometheus::IntGaugeVec = {
        let gauge_opts = prometheus::Opts::new(
            "function_calls_inflight_total",
            "Number of function calls currently in flight",
        );
        let gauge = prometheus::IntGaugeVec::new(gauge_opts, &["type","name","ctx"]).unwrap();

        DEFAULT_REGISTRY
            .register(Box::new(gauge.clone())).unwrap();

        gauge
    };
}

#[doc(hidden)]
pub fn inc_called_counter_for(name: &'static str, ctx: &'static str) {
    FUNC_CALLED
        .with_label_values(&["func_call", name, ctx])
        .inc();
}

#[doc(hidden)]
pub fn inc_error_counter_for(name: &'static str, ctx: &'static str, err: String) {
    FUNC_ERRORS
        .with_label_values(&["func_call", name, ctx, &err])
        .inc();
}

#[doc(hidden)]
pub fn get_timer_for(name: &'static str, ctx: &'static str) -> prometheus::HistogramTimer {
    FUNC_TIMER
        .with_label_values(&["func_call", name, ctx])
        .start_timer()
}

#[doc(hidden)]
pub fn inc_inflight_for(name: &'static str, ctx: &'static str) {
    FUNC_INFLIGHT
        .with_label_values(&["func_call", name, ctx])
        .inc();
}

#[doc(hidden)]
pub fn dec_inflight_for(name: &'static str, ctx: &'static str) {
    FUNC_INFLIGHT
        .with_label_values(&["func_call", name, ctx])
        .dec();
}

/// Initializes the metrics context, and starts an HTTP server
/// to serve metrics.
pub fn init(addr: &str) {
    let parsed_addr = addr.parse().unwrap();
    let server = Server::bind(&parsed_addr)
        .serve(|| {
            // This is the `Service` that will handle the connection.
            // `service_fn_ok` is a helper to convert a function that
            // returns a Response into a `Service`.
            service_fn_ok(move |req: Request<Body>| {
                use crate::prometheus::*;
                if req.uri().path() == "/metrics" {
                    let metric_families = DEFAULT_REGISTRY.gather();
                    let mut buffer = vec![];
                    let encoder = TextEncoder::new();
                    encoder.encode(&metric_families, &mut buffer).unwrap();

                    Response::builder()
                        .status(StatusCode::OK)
                        .header("Content-Type", encoder.format_type())
                        .body(Body::from(buffer))
                        .expect("Error constructing response")
                } else {
                    Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Body::from("Not found."))
                        .expect("Error constructing response")
                }
            })
        })
        .map_err(|e| error!("server error: {}", e));

    info!("Exporting metrics at http://{}/metrics", addr);

    let mut rt = tokio::runtime::Builder::new()
        .core_threads(1) // one thread is sufficient
        .build()
        .expect("Unable to build metrics exporter tokio runtime");

    std::thread::spawn(move || {
        rt.spawn(server);
        rt.shutdown_on_idle().wait().unwrap();
    });
}

/// Register a collector with the global registry.
pub fn register(c: Box<dyn::prometheus::core::Collector>) -> ::prometheus::Result<()> {
    DEFAULT_REGISTRY.register(c)
}
