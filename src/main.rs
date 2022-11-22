extern crate core;

use crate::layout::DataChunk;

mod api;
mod fs;
mod interval;
mod layout;
mod state;
mod util;

#[tokio::main]
async fn main() {
    println!("{}", DataChunk::new(0, 0, 1000000));
}
