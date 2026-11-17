use std::{env, error::Error};

fn main() -> Result<(), Box<dyn Error>> {
  println!("cargo:rustc-env=VERSION={}", env!("CARGO_PKG_VERSION"));
  println!("cargo:rustc-env=TARGET={}", env::var("TARGET")?);
  Ok(())
}
