use fx_core::{Analysis, AnalysisDiagnostic};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompileTarget {
    JavaScript,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompileOptions {
    pub target: CompileTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompileSession {
    pub target: CompileTarget,
    pub analysis: Analysis,
}

pub fn prepare_compile(
    analysis: Analysis,
    options: CompileOptions,
) -> Result<CompileSession, Vec<AnalysisDiagnostic>> {
    let diagnostics = analysis
        .diagnostics_by_path()
        .into_values()
        .flatten()
        .collect::<Vec<_>>();

    if diagnostics.is_empty() {
        Ok(CompileSession {
            target: options.target,
            analysis,
        })
    } else {
        Err(diagnostics)
    }
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            target: CompileTarget::JavaScript,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fx_core::{SourceFile, analyze_workspace};
    use std::path::PathBuf;

    #[test]
    fn prepares_phase_2_5_examples_for_javascript() {
        let analysis = analyze_workspace(&PathBuf::from("examples/phase-2.5"), phase_2_5_files());
        let session =
            prepare_compile(analysis, CompileOptions::default()).expect("compile session");

        assert_eq!(session.target, CompileTarget::JavaScript);
        assert!(
            session
                .analysis
                .diagnostics_by_path()
                .values()
                .all(Vec::is_empty)
        );
    }

    #[test]
    fn returns_existing_diagnostics_before_creating_session() {
        let analysis = analyze_workspace(
            &PathBuf::from("examples/phase-2.5"),
            vec![SourceFile {
                path: PathBuf::from("examples/phase-2.5/bad.fx"),
                source: "bad() : Int\n  missing\n".to_string(),
            }],
        );

        let diagnostics =
            prepare_compile(analysis, CompileOptions::default()).expect_err("diagnostics");

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.message.contains("unknown name `missing`") })
        );
    }

    #[test]
    fn preserves_original_frontend_diagnostic_messages() {
        let analysis = analyze_workspace(
            &PathBuf::from("examples/phase-2.5"),
            vec![SourceFile {
                path: PathBuf::from("examples/phase-2.5/bad.fx"),
                source: "bad() : Int\n  if\n".to_string(),
            }],
        );

        let diagnostics =
            prepare_compile(analysis, CompileOptions::default()).expect_err("diagnostics");

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.message.contains("expected expression") })
        );
    }

    fn phase_2_5_files() -> Vec<SourceFile> {
        vec![
            SourceFile {
                path: PathBuf::from("examples/phase-2.5/domain.fx"),
                source: include_str!("../../../examples/phase-2.5/domain.fx").to_string(),
            },
            SourceFile {
                path: PathBuf::from("examples/phase-2.5/functions.fx"),
                source: include_str!("../../../examples/phase-2.5/functions.fx").to_string(),
            },
            SourceFile {
                path: PathBuf::from("examples/phase-2.5/main.fx"),
                source: include_str!("../../../examples/phase-2.5/main.fx").to_string(),
            },
        ]
    }
}
