//! # Instrumented
//!
//! `instrumented` provides an attribute macro that enables instrumentation of
//! functions for use with Prometheus.
//!
//! This crate is largely based on the `log-derive` crate, and
//! inspired by the `metered` crate.
//!
//!
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;

extern crate hyper;
extern crate prometheus;
#[allow(unused_imports)]
#[macro_use]
extern crate instrumented_codegen;
#[doc(hidden)]
pub use instrumented_codegen::*;

use hyper::http::StatusCode;
use hyper::rt::Future;
use hyper::service::service_fn_ok;
use hyper::{Body, Request, Response, Server};
use prometheus::{Encoder, Registry, TextEncoder};

#[cfg(all(target_os = "linux"))]
fn register_default_process_collector(reg: &Registry) -> Result<()> {
    use prometheus::process_collector::ProcessCollector;

    let pc = ProcessCollector::for_self();
    reg.register(Box::new(pc))
}

lazy_static! {
    static ref DEFAULT_REGISTRY: Registry = {
        let reg = Registry::default();

        // Register a default process collector.
        #[cfg(all(target_os = "linux"))]
        register_default_process_collector(&reg).unwrap();

        reg
    };
    static ref FUNC_CALLED: prometheus::IntCounterVec = {
        let counter_opts = prometheus::Opts::new(
            "function_called",
            "Number of times a function was called",
        );
        let counter = prometheus::IntCounterVec::new(counter_opts, &["name"]).unwrap();

        DEFAULT_REGISTRY
            .register(Box::new(counter.clone())).unwrap();

        counter
    };
    static ref FUNC_ERRORS: prometheus::IntCounterVec = {
        let counter_opts = prometheus::Opts::new(
            "function_error",
            "Number of times the result of a function was an error",
        );
        let counter = prometheus::IntCounterVec::new(counter_opts, &["name"]).unwrap();

        DEFAULT_REGISTRY
            .register(Box::new(counter.clone())).unwrap();

        counter
    };
    static ref FUNC_TIMER: prometheus::HistogramVec = {
        let histogram_opts = prometheus::HistogramOpts::new(
            "function_timer",
            "Histogram of function call times observed",
        );
        let histogram = prometheus::HistogramVec::new(histogram_opts, &["name"]).unwrap();

        DEFAULT_REGISTRY
            .register(Box::new(histogram.clone())).unwrap();

        histogram
    };
    static ref FUNC_INFLIGHT: prometheus::IntGaugeVec = {
        let gauge_opts = prometheus::Opts::new(
            "function_inflight",
            "Number of function calls currently in flight",
        );
        let gauge = prometheus::IntGaugeVec::new(gauge_opts, &["name"]).unwrap();

        DEFAULT_REGISTRY
            .register(Box::new(gauge.clone())).unwrap();

        gauge
    };
}

pub fn inc_called_counter_for(name: &'static str) {
    FUNC_CALLED.with_label_values(&[name]).inc();
}

pub fn inc_error_counter_for(name: &'static str) {
    FUNC_ERRORS.with_label_values(&[name]).inc();
}

pub fn get_timer_for(name: &'static str) -> prometheus::HistogramTimer {
    FUNC_TIMER.with_label_values(&[name]).start_timer()
}

pub fn inc_inflight_for(name: &'static str) {
    FUNC_INFLIGHT.with_label_values(&[name]).inc();
}

pub fn dec_inflight_for(name: &'static str) {
    FUNC_INFLIGHT.with_label_values(&[name]).dec();
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
