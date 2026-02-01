//! Small test project - main entry point

mod utils;
mod config;

fn main() {
    let config = config::load_config();
    println!("Starting app: {}", config.name);

    let result = utils::process_data(vec![1, 2, 3, 4, 5]);
    println!("Result: {:?}", result);
}
