pub(in crate::detect) fn droid_permission_required(recent: &str, bottom_eight: &str) -> bool {
    let execute_selection = recent.contains("enter to select")
        && recent.contains("esc to cancel")
        && ["↑↓ to navigate", "use ↑↓ to navigate"]
            .iter()
            .any(|signal| recent.contains(signal))
        && ["> yes, allow", "> no, cancel"]
            .iter()
            .any(|signal| recent.contains(signal));
    let selection_menu = bottom_eight.contains("enter select")
        && bottom_eight.contains("esc cancel")
        && ["↑/↓ navigate", "↑↓ navigate"]
            .iter()
            .any(|signal| bottom_eight.contains(signal));
    execute_selection || selection_menu
}
