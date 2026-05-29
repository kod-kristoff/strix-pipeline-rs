use clap::Parser;

use crate::options::Args;

#[path = "strix_pipeline/options.rs"]
mod options;

fn main() {
    println!("Hello, world!");
    let args = Args::parse();
    dbg!(&args);
    match args.cmd {
        options::Command::Add(cmd) => {}
        options::Command::Delete(cmd) => {}
        options::Command::GenerateVectorData(cmd) => {}
    }
}
