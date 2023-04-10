use std::{
    env, fs, io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use clap::Parser;
use color_eyre::eyre::{self, WrapErr};
use ctrlc;
use rustc_hash::{FxHashMap, FxHashSet};
use swc_atoms::JsWord;
use swc_css::ast::ComponentValue;
use tracing::{error, info, trace, Level};
use tracing_subscriber::FmtSubscriber;
use walkdir::WalkDir;
use xiss::{
    class_map::ClassMapOutput, compiler::compile, config::Config, const_map::extract_const_values,
    css_map::CssMap,
};

const MODULE_EXTENSION: &str = "xiss";

#[derive(Debug, Parser)]
#[command(name = "xiss")]
#[command(author = "Boris Kaul <localvoid@gmail.com")]
#[command(version = "0.1")]
#[command(about = "Compiler for .xiss CSS modules", long_about=None)]
struct Cli {
    /// Path to a config file
    #[arg(short, long, default_value = "xiss.json")]
    config: PathBuf,
    #[arg(short, long)]
    /// Watch mode
    watch: bool,
    #[arg(short, long)]
    /// Purge output files
    purge: bool,
    /// Reset CSS map
    #[arg(short, long)]
    reset: bool,
    /// Force update
    #[arg(short, long)]
    force: bool,
    /// Class map output type
    #[arg(long, default_value_t = ClassMapOutput::Inline)]
    class_map: ClassMapOutput,
    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

fn main() -> eyre::Result<()> {
    color_eyre::install()?;

    let args = Cli::parse();

    let subscriber = FmtSubscriber::builder()
        .with_max_level(if args.verbose {
            Level::TRACE
        } else {
            Level::INFO
        })
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set default tracing subscriber");

    let cwd = env::current_dir()?;
    trace!("CWD {:?}", cwd);

    trace!("Config path {:?}", args.config);
    let config = if args.config.is_file() {
        Config::from_file(&args.config)?
    } else {
        Config::default()
    };

    trace!("Output directory {:?}", config.output);
    let output = if let Ok(output) = config.output.strip_prefix("./") {
        output
    } else {
        &config.output
    };
    if !output.exists() {
        fs::create_dir_all(output)
            .wrap_err_with(|| format!("Failed to create output directory {:?}", output))?;
    } else if !config.output.is_dir() {
        return Err(eyre::eyre!(
            "Output directory {:?} is not a directory",
            output
        ));
    }

    trace!("Include directory {:?}", config.include);
    let include = if let Ok(include) = config.include.strip_prefix("./") {
        include
    } else {
        &config.include
    };
    if !include.is_dir() {
        return Err(eyre::eyre!(
            "Invalid include path {:?}, include path should be a directory",
            include
        ));
    }

    let mut css_map = CssMap::new(
        &config.map.exclude.class,
        &config.map.exclude.var,
        &config.map.exclude.keyframes,
    )?;
    if config.map.lock.is_file() {
        let file = fs::OpenOptions::new()
            .read(true)
            .open(&config.map.lock)
            .wrap_err_with(|| format!("Failed to open css map lock file {:?}", config.map.lock))?;
        let mut reader = io::BufReader::new(file);
        css_map.import(&mut reader).wrap_err_with(|| {
            format!("Failed to import css map lock file {:?}", config.map.lock)
        })?;
    }
    let css_map_file = if config.map.path.is_file() && !args.reset {
        let file = fs::OpenOptions::new()
            .read(true)
            .append(true)
            .open(&config.map.path)
            .wrap_err_with(|| format!("Failed to open css map file {:?}", config.map.path))?;
        let mut reader = io::BufReader::new(file);
        css_map
            .import(&mut reader)
            .wrap_err_with(|| format!("Failed to import css map file {:?}", config.map.path))?;
        reader.into_inner()
    } else {
        if let Some(dir) = config.map.path.parent() {
            fs::create_dir_all(dir).wrap_err_with(|| {
                format!(
                    "Failed to create a directory for a css map file {:?}",
                    config.map
                )
            })?;
        }
        fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&config.map.path)
            .wrap_err_with(|| format!("Failed to open css map file {:?}", config.map.path))?
    };
    let mut css_map_writer = io::BufWriter::new(css_map_file);

    let const_map_path = include.join("const.css");
    let const_map = if const_map_path.is_file() {
        let const_map_content = fs::read_to_string(&const_map_path)
            .wrap_err_with(|| format!("Failed to read const map file {:?}", const_map_path))?;
        match extract_const_values(&const_map_path, const_map_content) {
            Ok(result) => result,
            Err(err) => {
                return Err(eyre::eyre!(
                    "Invalid const map file {:?}\n{}",
                    const_map_path,
                    err
                ));
            }
        }
    } else {
        FxHashMap::default()
    };

    let mut modules = FxHashSet::default();

    build(
        &mut modules,
        &mut css_map,
        &mut css_map_writer,
        &const_map,
        output,
        include,
        args.force || args.reset,
        args.class_map,
    )?;

    if args.purge {
        purge_output_files(&mut modules, output)?;
    }

    if args.watch {
        watch(
            &mut modules,
            &mut css_map,
            &mut css_map_writer,
            &const_map,
            &cwd,
            output,
            include,
            args.class_map,
        )?;
    }

    Ok(())
}

fn update_module<W: io::Write>(
    css_map: &mut CssMap,
    css_map_writer: &mut W,
    const_map: &FxHashMap<JsWord, Vec<ComponentValue>>,
    output: &Path,
    path: &Path,
    module_id: &str,
    force_update: bool,
    class_map_output: ClassMapOutput,
) -> eyre::Result<()> {
    let out_module_path = output.join(module_id);
    let css_path = out_module_path.with_extension("css");
    let js_path = css_path.with_extension("js");
    let ts_path = css_path.with_extension("d.ts");

    if force_update || should_compile(path, &css_path, &js_path, &ts_path) {
        match fs::read_to_string(path) {
            Ok(contents) => {
                trace!("Compiling module \"{}\"", module_id);
                match compile(
                    path,
                    contents,
                    css_map,
                    const_map,
                    module_id,
                    class_map_output,
                ) {
                    Ok(artifact) => {
                        css_map
                            .flush_new_ids(css_map_writer)
                            .wrap_err("Failed to update css map")?;
                        if let Some(dirname) = css_path.parent() {
                            if !dirname.exists() {
                                if let Err(err) = fs::create_dir_all(dirname) {
                                    error!(
                                        "Unable to create output directory {:?}: {}",
                                        dirname, err
                                    );
                                }
                            }
                        }
                        try_update_output_file(&css_path, &artifact.css);
                        try_update_output_file(&js_path, &artifact.js);
                        try_update_output_file(&ts_path, &artifact.ts);
                    }
                    Err(err) => {
                        error!("Failed to compile {:?}\n{}", path, err);
                    }
                }
            }
            Err(err) => {
                error!("Unable to read xiss file {:?}: {}", path, err);
            }
        }
    }
    Ok(())
}

fn build<W: io::Write>(
    modules: &mut FxHashSet<String>,
    css_map: &mut CssMap,
    css_map_writer: &mut W,
    const_map: &FxHashMap<JsWord, Vec<ComponentValue>>,
    output: &Path,
    include: &Path,
    force_update: bool,
    class_map_output: ClassMapOutput,
) -> eyre::Result<()> {
    for entry in WalkDir::new(include) {
        let entry = entry?;
        if !has_module_extension(entry.path()) {
            continue;
        }

        match path_to_module_id(include, entry.path()) {
            Ok(module_id) => {
                modules.insert(module_id.to_string());
                update_module(
                    css_map,
                    css_map_writer,
                    const_map,
                    output,
                    entry.path(),
                    module_id,
                    force_update,
                    class_map_output,
                )?;
            }
            Err(err) => {
                error!("{}", err);
            }
        }
    }
    Ok(())
}

#[derive(Debug)]
enum WatchMessage {
    FileChanged(PathBuf),
    CtrlC,
}

fn watch<W: io::Write>(
    modules: &mut FxHashSet<String>,
    css_map: &mut CssMap,
    css_map_writer: &mut W,
    const_map: &FxHashMap<JsWord, Vec<ComponentValue>>,
    cwd: &Path,
    output: &Path,
    include: &Path,
    class_map_output: ClassMapOutput,
) -> eyre::Result<()> {
    use notify_debouncer_mini::{new_debouncer, notify::*, DebounceEventResult};
    info!("Watching files for changes. Press Ctrl-C to abort...");

    let (tx, rx) = crossbeam::channel::unbounded();
    let tx2 = tx.clone();

    let mut debouncer = new_debouncer(
        Duration::from_millis(500),
        None,
        move |res: DebounceEventResult| match res {
            Ok(events) => events.iter().for_each(|e| {
                if has_module_extension(&e.path) {
                    tx.send(WatchMessage::FileChanged(e.path.to_path_buf()))
                        .expect("Could not send watcher message");
                }
            }),
            Err(errors) => errors
                .iter()
                .for_each(|e| error!("File watcher error: {}", e)),
        },
    )?;

    debouncer
        .watcher()
        .watch(Path::new(include), RecursiveMode::Recursive)?;

    ctrlc::set_handler(move || {
        tx2.send(WatchMessage::CtrlC)
            .expect("Could not send Ctrl-C signal")
    })?;

    loop {
        match rx
            .recv()
            .wrap_err("Could not receive messages from a channel")?
        {
            WatchMessage::FileChanged(path) => {
                if let Ok(path) = path.strip_prefix(cwd) {
                    match path_to_module_id(include, path) {
                        Ok(module_id) => {
                            if path.exists() {
                                if !modules.contains(module_id) {
                                    modules.insert(module_id.to_string());
                                    trace!("File added: {:?}", path);
                                } else {
                                    trace!("File modified: {:?}", path);
                                }
                                update_module(
                                    css_map,
                                    css_map_writer,
                                    const_map,
                                    output,
                                    &path,
                                    module_id,
                                    false,
                                    class_map_output,
                                )?;
                            } else {
                                modules.remove(module_id);

                                let out_module_path = output.join(module_id);
                                let css_path = out_module_path.with_extension("css");
                                try_remove_file(&css_path);
                                try_remove_file(&css_path.with_extension("js"));
                                try_remove_file(&css_path.with_extension("d.ts"));

                                trace!("File removed: {:?}", path);
                            }
                        }
                        Err(err) => {
                            error!("{}", err);
                        }
                    }
                }
            }
            WatchMessage::CtrlC => break,
        }
    }

    Ok(())
}

/// Purges output files that are no longer associated with xiss modules.
fn purge_output_files(modules: &mut FxHashSet<String>, output: &Path) -> eyre::Result<()> {
    trace!("Purging output files");
    let mut purged_files = 0;
    for entry in WalkDir::new(output) {
        let entry = entry?;
        let path = entry.path();
        if let Some(file_name) = path.file_name() {
            // ignore dot files
            if !Path::new(file_name).starts_with(".") {
                if path.is_file() {
                    match path_to_module_id(output, path) {
                        Ok(module_id) => {
                            if !modules.contains(module_id) {
                                trace!("Removing file {:?}", path);
                                if let Err(err) = fs::remove_file(entry.path()) {
                                    error!("Unable to remove file {:?}: {}", entry.path(), err);
                                }
                                modules.remove(module_id);
                                purged_files += 1;
                            }
                        }
                        Err(err) => {
                            error!("{}", err);
                        }
                    }
                }
            }
        }
    }
    if purged_files > 0 {
        info!("Purged {} files", purged_files);
    }
    Ok(())
}

/// Returns true if path has a [MODULE_EXTENSION].
fn has_module_extension(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        if ext == MODULE_EXTENSION {
            return true;
        }
    }
    false
}

/// Extracts module id from a path.
fn path_to_module_id<'a>(output: &Path, path: &'a Path) -> eyre::Result<&'a str> {
    let relative_path = path.strip_prefix(output).wrap_err_with(|| {
        format!(
            "Invalid file path {:?}, path should be a subpath of an output directory {:?}",
            path, output
        )
    })?;
    let mut module_id = if let Some(path) = relative_path.to_str() {
        path
    } else {
        return Err(eyre::eyre!(
            "Invalid file path {:?}, path should be a valid utf8 string",
            relative_path,
        ));
    };

    while let Some(i) = module_id.rfind('.') {
        module_id = &module_id[..i];
    }
    Ok(module_id)
}

/// Checks if output file should be updated.
fn should_update(path: &Path, content: &str) -> bool {
    if let Ok(s) = fs::read_to_string(path) {
        s != content
    } else {
        true
    }
}

/// Checks if module should be recompiled.
fn should_compile(module_path: &Path, css_path: &Path, js_path: &Path, ts_path: &Path) -> bool {
    if let Ok(output_time) = max_modified_time_3(css_path, js_path, ts_path) {
        if let Ok(meta) = module_path.metadata() {
            if let Ok(input_time) = meta.modified() {
                if input_time <= output_time {
                    return false;
                }
            }
        }
    }
    true
}

/// Returns max modified time.
fn max_modified_time_3(
    css_path: &Path,
    js_path: &Path,
    ts_path: &Path,
) -> Result<SystemTime, std::io::Error> {
    let mut t = css_path.metadata()?.modified()?;
    let t2 = js_path.metadata()?.modified()?;
    if t2 > t {
        t = t2;
    }
    let t3 = ts_path.metadata()?.modified()?;
    if t3 > t {
        t = t3;
    }

    Ok(t)
}

fn try_update_output_file(path: &Path, output: &str) {
    if should_update(path, output) {
        trace!("Updating {:?}", path);
        if let Err(err) = fs::write(path, output) {
            error!("Unable to write file {:?}: {}", path, err);
        }
    }
}

fn try_remove_file(path: &Path) {
    if path.exists() {
        if let Err(err) = fs::remove_file(path) {
            error!("Unable to remove file {:?}: {}", path, err);
        } else {
            trace!("Removing file {:?}", path);
        }
    }
}
