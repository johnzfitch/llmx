//! Configuration handling

/// Application configuration.
#[derive(Debug)]
pub struct Config {
    pub name: String,
    pub debug: bool,
    pub port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            name: "test-app".to_string(),
            debug: false,
            port: 8080,
        }
    }
}

/// Load configuration from environment.
pub fn load_config() -> Config {
    Config::default()
}
