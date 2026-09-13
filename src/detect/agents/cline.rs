pub(in crate::detect) fn cline_permission_required(recent: &str) -> bool {
    recent.contains("let cline use this tool")
        || [
            ("[act mode]", "execute command?", "yes"),
            ("[act mode]", "use this tool?", "yes"),
            ("[plan mode]", "execute command?", "yes"),
            ("[plan mode]", "use this tool?", "yes"),
        ]
        .iter()
        .any(|(mode, action, approval)| {
            recent.contains(mode) && recent.contains(action) && recent.contains(approval)
        })
}
