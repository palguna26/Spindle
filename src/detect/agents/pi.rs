pub(in crate::detect) fn pi_is_working(recent: &str) -> bool {
    recent.contains("working...")
}

#[cfg(test)]
mod tests {
    use super::pi_is_working;

    #[test]
    fn recognizes_herdr_working_literal() {
        assert!(pi_is_working("working..."));
        assert!(!pi_is_working("working"));
    }
}
