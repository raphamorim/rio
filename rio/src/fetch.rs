use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

use failure::{Fallible};

//http://localhost:3000/package/github.com.expressjs.express-4.17.1.tar.gz

pub fn fetch_packages() -> Fallible<()> {
    Ok(())
}
