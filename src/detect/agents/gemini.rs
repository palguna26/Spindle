pub(in crate::detect) fn gemini_permission_required(recent: &str) -> bool {
    recent.contains("\u{2502} apply this change")
        || recent.contains("\u{2502} allow execution")
        || (recent.contains("yes")
            && (recent.contains("waiting for user confirmation")
                || recent.contains("\u{2502} do you want to proceed")
                || recent.contains("do you want to proceed?")))
        || recent.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with('\u{276f}') && (line.contains("yes") || line.contains("allow"))
        })
}
