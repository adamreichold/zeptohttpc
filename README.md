# zeptohttpc

This crate aims to be the smallest possible yet practically useful HTTP client built on top of the [`http`](https://docs.rs/http) and [`httparse`](https://docs.rs/httparse) crates.

## Cargo features

* `encoding_rs`: Support for bodies in various character sets using the [`encoding_rs`](https://docs.rs/encoding_rs) crate.
* `flate2`: Support for compressed bodies using the [`flate2`](https://docs.rs/flate2) crate.
* `native-tls`: Support HTTPS connections using the [`native-tls`](https://docs.rs/native-tls) crate.
* `json`: Support for JSON bodies using the [`serde`](https://docs.rs/serde) and [`serde_json`](https://docs.rs/serde_json) crates.
* `tls`: Support for HTTPS connections using the [`rustls`](https://docs.rs/rustls), [`webpki`](https://docs.rs/webpki) and [`webpki-roots`](https://docs.rs/webpki-roots) crates.

## License

Licensed under

 * [Apache License, Version 2.0](LICENSE-APACHE) or
 * [MIT license](LICENSE-MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
