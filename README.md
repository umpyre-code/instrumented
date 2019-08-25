[![Current Crates.io Version](https://img.shields.io/crates/v/instrumented.svg)](https://crates.io/crates/instrumented) [![Docs](https://docs.rs/instrumented/badge.svg)](https://docs.rs/instrumented/) [![pipeline status](https://gitlab.com/umpyre-code/instrumented/badges/master/pipeline.svg)](https://gitlab.com/umpyre-code/instrumented/commits/master) [![coverage report](https://gitlab.com/umpyre-code/instrumented/badges/master/coverage.svg)](https://gitlab.com/umpyre-code/instrumented/commits/master)

# Instrumented 🎸

Observe your service.

You can specify the global metrics prefix with the `METRICS_PREFIX` env var,
and provide default labels with the `METRICS_LABELS` env var, which accepts a
command separated list of `label=value` pairs. For example:

```shell
METRICS_PREFIX=myapp
METRICS_LABELS=app=myapp,env=prod,region=us
```

## Example

```rust
extern crate instrumented;
extern crate log;
extern crate reqwest;

use instrumented::instrument;

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

#[instrument(INFO)]
fn my_func_with_err_result() -> Result<String, MyError> {
    use std::{thread, time};
    let ten_millis = time::Duration::from_millis(10);
    thread::sleep(ten_millis);

    Err(MyError)
}

fn main() {
    let addr = "127.0.0.1:5000".to_string();
    instrumented::init(&addr);

    my_func();
    assert_eq!(my_func_with_ok_result().is_ok(), true);
    assert_eq!(my_func_with_err_result().is_err(), true);

    let body = reqwest::get(&format!("http://{}/metrics", addr))
        .unwrap()
        .text()
        .unwrap();

    println!("{}", body);
}
```
