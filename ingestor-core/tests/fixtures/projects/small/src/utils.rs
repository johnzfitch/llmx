//! Utility functions

/// Process a vector of data.
pub fn process_data(data: Vec<i32>) -> Vec<i32> {
    data.iter().map(|x| x * 2).collect()
}

/// Calculate the sum of a vector.
pub fn sum(data: &[i32]) -> i32 {
    data.iter().sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_data() {
        assert_eq!(process_data(vec![1, 2, 3]), vec![2, 4, 6]);
    }

    #[test]
    fn test_sum() {
        assert_eq!(sum(&[1, 2, 3]), 6);
    }
}
