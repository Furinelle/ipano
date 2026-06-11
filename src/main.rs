mod aggregate;
mod egress;
mod model;
mod fetch;
mod render;
mod sources;

fn main() {
    println!("ipano {}", env!("CARGO_PKG_VERSION"));
}
