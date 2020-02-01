// Copyright 2020 Adam Reichold
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
use std::env::args;
use std::error::Error;
use std::io::{copy, stdout};

use zeptohttpc::{http::Request, Options, RequestBuilderExt, RequestExt};

fn main() -> Result<(), Box<dyn Error>> {
    let uri = args().nth(1).ok_or("Missing URI argument")?;

    let mut opts = Options::default();
    opts.follow_redirects = None;

    let resp = Request::get(uri).empty()?.send_with_opts(opts)?;

    for (name, value) in resp.headers() {
        eprintln!("{}: {:?}", name, value);
    }

    eprintln!();

    copy(&mut resp.into_body(), &mut stdout())?;

    Ok(())
}
