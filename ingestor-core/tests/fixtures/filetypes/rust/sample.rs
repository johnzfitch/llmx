//! Sample Rust module for testing.

use std::collections::HashMap;

/// A simple struct for testing.
#[derive(Debug, Clone)]
pub struct TestStruct {
    pub name: String,
    pub value: i32,
}

impl TestStruct {
    /// Creates a new TestStruct.
    pub fn new(name: &str, value: i32) -> Self {
        TestStruct {
            name: name.to_string(),
            value,
        }
    }

    /// Returns the name.
    pub fn get_name(&self) -> &str {
        &self.name
    }
}

/// A test function.
pub fn process_data(items: Vec<TestStruct>) -> HashMap<String, i32> {
    items.into_iter().map(|item| (item.name, item.value)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let s = TestStruct::new("test", 42);
        assert_eq!(s.name, "test");
        assert_eq!(s.value, 42);
    }
}
