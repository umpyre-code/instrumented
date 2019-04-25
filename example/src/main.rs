extern crate instrumented;
extern crate log;
extern crate reqwest;

use instrumented::{instrument, prometheus};

#[instrument(INFO)]
fn my_func() {
    use std::{thread, time};
    let ten_millis = time::Duration::from_millis(10);
    thread::sleep(ten_millis);
}

#[derive(Debug)]
pub struct MyError;

#[instrument(INFO)]
fn my_func_with_ok_result() -> Result<String, MyError> {
    use std::{thread, time};
    let ten_millis = time::Duration::from_millis(10);
    thread::sleep(ten_millis);

    Ok(String::from("hello world"))
}

#[instrument(INFO, ctx = "my_context")]
fn my_func_with_err_result() -> Result<String, crate::MyError> {
    use std::{thread, time};
    let ten_millis = time::Duration::from_millis(10);
    thread::sleep(ten_millis);

    Err(crate::MyError)
}

fn main() {
    let addr = "127.0.0.1:5000".to_string();
    instrumented::init(&addr);

    my_func();
    assert_eq!(my_func_with_ok_result().is_ok(), true);
    assert_eq!(my_func_with_err_result().is_err(), true);

    // Add a custom counter
    let counter = prometheus::IntCounter::new("custom_counter", "My custom counter").unwrap();
    instrumented::register(Box::new(counter.clone())).unwrap();
    counter.inc_by(10);

    let body = reqwest::get(&format!("http://{}/metrics", addr))
        .unwrap()
        .text()
        .unwrap();

    println!("{}", body);
}
