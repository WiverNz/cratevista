//! Normalizes a generated snapshot into a committable E2E fixture.
//!
//! `cargo cratevista generate` records the rustdoc command verbatim in its
//! `target_failed` diagnostic, and cargo resolves that command to an ABSOLUTE
//! manifest path. Committing it would bake the refreshing developer's filesystem
//! layout into the repository. This tool rewrites the known fixture-workspace
//! prefix to the stable token `<fixture-workspace>` and then re-commits the
//! snapshot through the production writer, so `artifact_hashes` are recomputed
//! over the exact normalized bytes that land on disk.
//!
//! The genuine content is preserved: `partial` stays `true` and the real
//! `target_failed` diagnostic remains. Only the machine-specific path prefix is
//! rewritten, deterministically. Committed fixtures are never hand-edited.
//!
//! Invoked by `web/scripts/refresh-e2e-snapshots.mjs`; not part of the shipped
//! CLI and not a change to the production diagnostics pipeline.
//!
//! Usage:
//!   cargo run -p cratevista-core --example gen_e2e_fixtures -- \
//!       <source-dir> <dest-dir> <workspace-root>

use std::path::Path;
use std::process::ExitCode;

use cratevista_core::artifacts::commit_artifacts;
use cratevista_schema::{DiagnosticsReport, ExplorerDocument, GenerationReport};

/// The stable stand-in for the absolute fixture-workspace path.
const TOKEN: &str = "<fixture-workspace>";

/// Rewrites every occurrence of `root` in `text` to [`TOKEN`].
///
/// The JSON source escapes Windows separators (`D:\ws` is stored as `D:\\ws`),
/// so the escaped spelling is replaced first — it is the most specific — before
/// the plain and forward-slash spellings.
fn normalize(text: &str, root: &str) -> String {
    let escaped = root.replace('\\', "\\\\");
    let forward = root.replace('\\', "/");
    let mut out = text.replace(&escaped, TOKEN);
    out = out.replace(root, TOKEN);
    out.replace(&forward, TOKEN)
}

fn read(dir: &Path, name: &str) -> std::io::Result<String> {
    std::fs::read_to_string(dir.join(name))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [source, dest, root] = args.as_slice() else {
        eprintln!("usage: gen_e2e_fixtures <source-dir> <dest-dir> <workspace-root>");
        return ExitCode::FAILURE;
    };
    let (source, dest) = (Path::new(source), Path::new(dest));

    let mut texts = Vec::new();
    for name in ["document.json", "diagnostics.json", "generation.json"] {
        match read(source, name) {
            Ok(text) => texts.push(normalize(&text, root)),
            Err(error) => {
                eprintln!("cannot read {}: {error}", source.join(name).display());
                return ExitCode::FAILURE;
            }
        }
    }

    // Parse the normalized text, so the fixture is validated before it is
    // committed and the writer hashes exactly what it serializes.
    let document: ExplorerDocument = match serde_json::from_str(&texts[0]) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("document.json is not a valid ExplorerDocument: {error}");
            return ExitCode::FAILURE;
        }
    };
    let diagnostics: DiagnosticsReport = match serde_json::from_str(&texts[1]) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("diagnostics.json is not a valid DiagnosticsReport: {error}");
            return ExitCode::FAILURE;
        }
    };
    let generation: GenerationReport = match serde_json::from_str(&texts[2]) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("generation.json is not a valid GenerationReport: {error}");
            return ExitCode::FAILURE;
        }
    };

    // `commit_artifacts` recomputes artifact_hashes over the exact canonical
    // bytes it writes, so the normalized fixture is integrity-valid by
    // construction rather than by hand-editing digests.
    match commit_artifacts(dest, &document, &diagnostics, generation) {
        Ok(paths) => {
            for path in paths {
                println!("wrote {}", path.display());
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("could not commit the fixture: {error}");
            ExitCode::FAILURE
        }
    }
}
