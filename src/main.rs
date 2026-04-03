use anyhow::Error;
use clap::Parser;
use std::io::{self, IsTerminal, Read as IoRead, Write as IoWrite};
use std::path::PathBuf;
use text2svg::font::{FontConfig, FontStyle};
use text2svg::highlight::HighlightSetting;
use text2svg::render::{self, RenderConfig, TextAlign};

// Exit codes for distinct error categories
const EXIT_USER_ERROR: i32 = 1;
const EXIT_FONT_ERROR: i32 = 2;
const EXIT_IO_ERROR: i32 = 3;

#[derive(Debug)]
enum CliError {
    User(Error),
    Font(Error),
    Io(Error),
}

impl CliError {
    fn exit_code(&self) -> i32 {
        match self {
            CliError::User(_) => EXIT_USER_ERROR,
            CliError::Font(_) => EXIT_FONT_ERROR,
            CliError::Io(_) => EXIT_IO_ERROR,
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::User(e) | CliError::Font(e) | CliError::Io(e) => write!(f, "{}", e),
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    about,
    version,
    long_about = None,
    after_help = "\
Examples:
  # Render text to SVG file
  text2svg \"Hello World\" --font Arial --output hello.svg

  # Render text to stdout (for piping)
  text2svg \"Hello\" --font Arial --output -

  # Render a file with syntax highlighting
  text2svg --file main.rs --font \"Fira Code\" --highlight --theme base16-ocean.dark -o code.svg

  # Pipe text from stdin
  echo \"Hello\" | text2svg --font Arial --output greeting.svg

  # Wrap text at pixel width with center alignment
  text2svg \"Long text here\" --font Arial --pixel-width 400 --align center -o wrapped.svg

  # List available resources
  text2svg --list-fonts
  text2svg --list-theme
  text2svg --list-syntax"
)]
struct Args {
    /// input text string
    #[arg(conflicts_with = "file")]
    text: Option<String>,

    /// max width per line (characters)
    #[arg(long, conflicts_with_all = ["highlight", "pixel_width"])]
    width: Option<usize>,

    /// max width per line (pixels)
    #[arg(long, conflicts_with_all = ["highlight", "width"])]
    pixel_width: Option<f32>,

    /// input file (use "-" for stdin)
    #[arg(long, short, conflicts_with = "text")]
    file: Option<String>,

    /// output svg file path (use "-" for stdout)
    #[arg(short, long, default_value = "output.svg")]
    output: String,

    /// font family name (e.g., "Arial", "Times New Roman")
    #[arg(long)]
    font: Option<String>,

    /// font size in pixels
    #[arg(long, default_value_t = 64)]
    size: u32,

    /// svg fill color (e.g., "#ff0000", "none"). Overridden by highlight.
    #[arg(long, conflicts_with = "highlight", default_value = "none")]
    fill: String,

    /// font stroke color (e.g., "#000", "currentColor"). Overridden by highlight.
    #[arg(long, conflicts_with = "highlight", default_value = "#000")]
    color: String,

    /// Add draw animation effect (works best with stroke only)
    #[arg(long, conflicts_with = "highlight")]
    animate: bool,

    /// font style (regular, bold, italic, etc.). Overridden by highlight.
    #[arg(
        value_enum,
        long,
        conflicts_with = "highlight",
        default_value = "regular"
    )]
    style: Option<FontStyle>,

    /// letter spacing (in em units, e.g., 0.1)
    #[arg(long, default_value_t = 0.0)]
    // Default to 0 for better compatibility with <use> positioning
    space: f32,

    /// font features (e.g., "cv01=1,calt=0,liga=1")
    #[arg(long, conflicts_with = "highlight")]
    features: Option<String>,

    /// Enable syntax highlighting mode for files
    #[arg(long)]
    highlight: bool,

    /// Syntax highlighting theme name or path to .tmTheme file
    #[arg(long, requires = "highlight", default_value = "base16-ocean.dark")]
    theme: Option<String>,

    /// List supported file types/syntax for highlighting
    #[arg(long)]
    list_syntax: bool,

    /// List available built-in highlighting themes
    #[arg(long)]
    list_theme: bool,

    /// Text alignment for multi-line output (left, center, right)
    #[arg(value_enum, long, default_value = "left")]
    align: Option<TextAlign>,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// List installed font families
    #[arg(long)]
    list_fonts: bool,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        std::process::exit(e.exit_code());
    }
}

fn run() -> Result<(), CliError> {
    let args = Args::parse();

    if args.debug {
        eprintln!("Debug Mode Enabled");
        eprintln!("Args: {:?}", args);
    }

    if args.list_fonts {
        let fonts = text2svg::font::fonts();
        if fonts.is_empty() {
            eprintln!("No fonts found or error listing fonts");
        } else {
            for name in fonts.iter() {
                println!("{}", name);
            }
        }
        return Ok(());
    }

    // Initialize highlight settings even if not used immediately, for listing themes/syntaxes
    let mut highlight_setting = HighlightSetting::default();

    // Handle custom theme path or name
    if let Some(theme_path_or_name) = &args.theme {
        let path = PathBuf::from(theme_path_or_name);
        if path.exists() && path.is_file() {
            match highlight_setting.add_theme_from_path("custom-theme", &path) {
                Ok(_) => {
                    highlight_setting.set_theme("custom-theme");
                    if args.debug {
                        eprintln!("Loaded custom theme from: {}", path.display());
                    }
                }
                Err(e) => {
                    return Err(CliError::User(anyhow::anyhow!(
                        "Failed to load theme from path '{}': {}\n  Hint: use --list-theme to see available built-in themes",
                        path.display(),
                        e
                    )));
                }
            }
        } else {
            // Assume it's a built-in theme name
            if highlight_setting.get_theme(theme_path_or_name).is_none() {
                return Err(CliError::User(anyhow::anyhow!(
                    "Theme '{}' not found\n  Hint: use --list-theme to see available themes",
                    theme_path_or_name
                )));
            } else {
                highlight_setting.set_theme(theme_path_or_name);
                if args.debug {
                    eprintln!("Using built-in theme: {}", theme_path_or_name);
                }
            }
        }
    }

    if args.list_syntax {
        for syntax in highlight_setting.syntax_set.syntaxes() {
            println!("{}\t.{}", syntax.name, syntax.file_extensions.join("\t."));
        }
        return Ok(());
    }

    if args.list_theme {
        list_themes(&highlight_setting);
        return Ok(());
    }

    // --- Determine input text ---
    // Try: positional text arg > --file > piped stdin
    let (input_text, input_file) = resolve_input(&args)?;

    // --- Font and Render Config ---
    let font_name = match args.font {
        Some(f) => f,
        None => {
            if input_text.is_none() && input_file.is_none() {
                return Ok(());
            }
            return Err(CliError::User(anyhow::anyhow!(
                "--font is required for rendering\n  Hint: use --list-fonts to see available font families"
            )));
        }
    };

    let output_to_stdout = args.output == "-";
    let output_path = if output_to_stdout {
        // Use a temporary path; we'll intercept before save
        PathBuf::from("/dev/null")
    } else {
        PathBuf::from(&args.output)
    };

    // Create FontConfig
    let mut font_config = FontConfig::new(
        font_name,
        args.size,
        args.fill.clone(),
        args.color.clone(),
        args.debug,
    )
    .map_err(|e| {
        CliError::Font(anyhow::anyhow!(
            "{}\n  Hint: use --list-fonts to see available font families",
            e
        ))
    })?;
    font_config.set_letter_space(args.space);

    // Apply font features if specified
    if let Some(features_str) = &args.features {
        if let Err(err) = font_config.set_features_from_string(features_str) {
            return Err(CliError::User(anyhow::anyhow!(
                "Failed to parse font features '{}': {}",
                features_str,
                err
            )));
        }
        if args.debug {
            eprintln!(
                "Applied font features: {}",
                font_config.get_features_summary()
            );
        }
    }

    if args.debug {
        eprintln!("Font Config: {:?}", font_config);
        eprintln!(
            "Active font features: {}",
            font_config.get_features_summary()
        );
    }

    // Create RenderConfig (for non-highlight mode)
    let mut render_config =
        RenderConfig::new(args.animate, args.style.unwrap_or(FontStyle::Regular));
    render_config.set_max_width(args.width);
    render_config.set_max_pixel_width(args.pixel_width);
    render_config.set_align(args.align.unwrap_or(TextAlign::Left));

    // --- Rendering Logic ---
    if let Some(text) = input_text {
        if args.highlight {
            eprintln!("Warning: Highlight mode is ignored when providing text directly.");
        }
        let doc = render::render_text_to_svg(&text, &mut font_config, &render_config);
        write_svg_output(doc, output_to_stdout, &output_path)?;
    } else if let Some(file) = input_file {
        if args.highlight {
            let doc =
                render::render_file_highlight_to_doc(&file, &mut font_config, &highlight_setting);
            write_svg_output(doc, output_to_stdout, &output_path)?;
        } else {
            let doc = render::render_text_file_to_svg_doc(&file, &mut font_config, &render_config);
            write_svg_output(doc, output_to_stdout, &output_path)?;
        }
    } else if !args.list_fonts && !args.list_syntax && !args.list_theme {
        return Err(CliError::User(anyhow::anyhow!(
            "No input provided. Pass text as argument, use --file <path>, or pipe via stdin"
        )));
    }

    Ok(())
}

/// Resolve input source: positional text, --file, or piped stdin.
fn resolve_input(args: &Args) -> Result<(Option<String>, Option<PathBuf>), CliError> {
    if let Some(ref text) = args.text {
        return Ok((Some(text.clone()), None));
    }

    if let Some(ref file_arg) = args.file {
        if file_arg == "-" {
            // Read from stdin
            let text = read_stdin()?;
            return Ok((Some(text), None));
        }
        let path = PathBuf::from(file_arg);
        if !path.exists() {
            return Err(CliError::Io(anyhow::anyhow!(
                "Input file not found: {}",
                path.display()
            )));
        }
        return Ok((None, Some(path)));
    }

    // Auto-detect piped stdin when no text and no file
    if !io::stdin().is_terminal() {
        let text = read_stdin()?;
        if !text.is_empty() {
            return Ok((Some(text), None));
        }
    }

    Ok((None, None))
}

fn read_stdin() -> Result<String, CliError> {
    let mut buf = String::new();
    io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| CliError::Io(anyhow::anyhow!("Failed to read from stdin: {}", e)))?;
    // Trim trailing newline that shells typically add
    if buf.ends_with('\n') {
        buf.pop();
        if buf.ends_with('\r') {
            buf.pop();
        }
    }
    Ok(buf)
}

/// Write SVG document to stdout or file.
fn write_svg_output(
    doc: Option<svg::Document>,
    to_stdout: bool,
    output_path: &PathBuf,
) -> Result<(), CliError> {
    let doc = match doc {
        Some(d) => d,
        None => {
            return Err(CliError::Io(anyhow::anyhow!("Failed to render SVG")));
        }
    };

    if to_stdout {
        let mut stdout = io::stdout().lock();
        svg::write(&mut stdout, &doc)
            .map_err(|e| CliError::Io(anyhow::anyhow!("Failed to write SVG to stdout: {}", e)))?;
        stdout
            .write_all(b"\n")
            .map_err(|e| CliError::Io(anyhow::anyhow!("Failed to write to stdout: {}", e)))?;
    } else {
        svg::save(output_path, &doc).map_err(|e| {
            CliError::Io(anyhow::anyhow!(
                "Failed to save SVG to {}: {}",
                output_path.display(),
                e
            ))
        })?;
    }
    Ok(())
}

fn list_themes(settings: &HighlightSetting) {
    for theme_name in settings.theme_set.themes.keys() {
        println!("{}", theme_name);
    }
}
