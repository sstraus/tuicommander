//! Argv consent policy. Templates are user-created and never model-created.
#[derive(Clone)]
pub(crate) struct Template(Vec<String>);

impl Template {
    pub(crate) fn new(argv: Vec<String>) -> Result<Self, String> {
        validate_argv(&argv)?;
        Ok(Self(argv))
    }

    pub(crate) fn matches(&self, argv: &[String]) -> bool {
        self.0 == argv
    }
}

pub(crate) fn validate_argv(argv: &[String]) -> Result<(), String> {
    let Some(program) = argv.first().filter(|p| !p.is_empty()) else {
        return Err("argv must include a program".into());
    };
    if argv.len() > 128 || argv.iter().any(|a| a.contains('\0') || a.len() > 8192) {
        return Err("Invalid argv".into());
    }
    let name = program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    // Exact-argv consent decides which command may receive values (Boss,
    // 2026-11-01, R04): no lexical ban on option letters or program names.
    // Batch files stay refused: Windows runs them through an implicit cmd.exe.
    if name.ends_with(".bat") || name.ends_with(".cmd") {
        return Err("Implicit shell batch execution is forbidden".into());
    }
    Ok(())
}
