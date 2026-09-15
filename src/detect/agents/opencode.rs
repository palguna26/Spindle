pub(in crate::detect) fn opencode_permission_required(recent: &str) -> bool {
    if recent.contains("\u{25b3} permission required") {
        return true;
    }

    recent.contains("esc dismiss")
        && (recent.contains("enter confirm")
            || recent.contains("enter submit")
            || recent.contains("enter toggle"))
        && (recent.contains("\u{2191}\u{2193} select") || recent.contains("\u{21c6} tab"))
}
