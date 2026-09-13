pub(in crate::detect) fn kilo_permission_required(recent: &str) -> bool {
    recent.contains("△ permission required")
        || (recent.contains("esc dismiss")
            && ["enter confirm", "enter submit", "enter toggle"]
                .iter()
                .any(|signal| recent.contains(signal))
            && ["↑↓ select", "⇆ tab"]
                .iter()
                .any(|signal| recent.contains(signal)))
}
