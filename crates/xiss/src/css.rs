use std::{fmt, mem::take, path::Path, sync::Arc};

use parking_lot::Mutex;
use swc_common::{
    errors::{Handler, HANDLER},
    FileName, SourceMap,
};
use swc_css::{
    ast::Stylesheet,
    parser::{parse_file, parser::ParserConfig},
};
use swc_error_reporters::{GraphicalReportHandler, PrettyEmitter, PrettyEmitterConfig};

pub fn process_css<P: AsRef<Path>, R, F: FnOnce(&Handler, &mut Stylesheet) -> Option<R>>(
    path: P,
    contents: String,
    func: F,
) -> Result<R, String> {
    let mut errors = vec![];
    let file_name = FileName::from(path.as_ref().to_path_buf());
    let cm: Arc<SourceMap> = Default::default();
    let wr = Box::new(LockedWriter::default());
    let emitter = PrettyEmitter::new(
        cm.clone(),
        wr.clone(),
        GraphicalReportHandler::new().with_context_lines(3),
        PrettyEmitterConfig {
            skip_filename: false,
        },
    );
    let handler = Handler::with_emitter(true, false, Box::new(emitter));
    let fm = cm.new_source_file(FileName::Custom(file_name.to_string()), contents);

    match parse_file::<Stylesheet>(
        &fm,
        ParserConfig {
            ..Default::default()
        },
        &mut errors,
    ) {
        Ok(ref mut stylesheet) => {
            if errors.is_empty() {
                if let Some(result) = HANDLER.set(&handler, || func(&handler, stylesheet)) {
                    return Ok(result);
                }
            } else {
                for err in errors {
                    err.to_diagnostics(&handler).emit();
                }
            }
        }
        Err(err) => {
            err.to_diagnostics(&handler).emit();
        }
    }
    let error_str = take(&mut *wr.0.lock());
    Err(error_str)
}

#[derive(Clone, Default)]
struct LockedWriter(Arc<Mutex<String>>);

impl fmt::Write for LockedWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0.lock().push_str(s);
        Ok(())
    }
}
