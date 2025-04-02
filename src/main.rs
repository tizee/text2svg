mod font;
mod render;
mod svg;
mod utils;
mod highlight;

use anyhow::Error;
use clap::Parser;
use font::{FontConfig, FontStyle};
use highlight::HighlightSetting;
use render::RenderConfig;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(about,version,long_about=None)]
struct Args {
    /// input text string
    #[arg(conflicts_with = "file")]
    text: Option<String>,

    /// max width per line (characters)
    #[arg(long, conflicts_with = "highlight")]
    width: Option<usize>,

    /// input file
    #[arg(long,short, conflicts_with = "text")]
    file: Option<PathBuf>,

    /// output svg file path
    #[arg(short, long, default_value = "output.svg")]
    output: Option<PathBuf>,

    /// font family name (e.g., "Arial", "Times New Roman")
    #[arg(long)]
    font: Option<String>,

    /// font size in pixels
    #[arg(long, default_value_t = 64)]
    size: u32,

    /// svg fill color (e.g., "#ff0000", "none"). Overridden by highlight.
    #[arg(long, conflicts_with="highlight", default_value = "none")]
    fill: String,

    /// font stroke color (e.g., "#000", "currentColor"). Overridden by highlight.
    #[arg(long, conflicts_with="highlight", default_value = "#000")]
    color: String,

    /// Add draw animation effect (works best with stroke only)
    #[arg(long, conflicts_with="highlight")]
    animate: bool,

    /// font style (regular, bold, italic, etc.). Overridden by highlight.
    #[arg(value_enum, long, conflicts_with="highlight", default_value = "regular")]
    style: Option<FontStyle>,

    /// letter spacing (in em units, e.g., 0.1)
    #[arg(long, default_value_t = 0.0)] // Default to 0 for better compatibility with <use> positioning
    space: f32,

    /// Enable syntax highlighting mode for files
    #[arg(long)]
    highlight: bool,

    /// Syntax highlighting theme name or path to .tmTheme file
    #[arg(long, requires="highlight", default_value="base16-ocean.dark")]
    theme: Option<String>,

    /// List supported file types/syntax for highlighting
    #[arg(long)]
    list_syntax: bool,

    /// List available built-in highlighting themes
    #[arg(long)]
    list_theme: bool,

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
        // Consider adding more context, e.g., e.chain()
        std::process::exit(1);
    }
}

fn run() -> Result<(),Error> {
    let args = Args::parse();

    if args.debug {
        println!("Debug Mode Enabled");
        println!("Args: {:?}", args);
    }

    if args.list_fonts {
        println!("Installed Font Families:");
        let fonts = font::fonts();
        if fonts.is_empty() {
            println!("  (No fonts found or error listing fonts)");
        } else {
            for name in fonts.iter() {
                println!("- {}", name);
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
             // Attempt to load theme from path using the correct method name
             match highlight_setting.add_theme_from_path("custom-theme", &path) {
                 Ok(_) => {
                     highlight_setting.set_theme("custom-theme");
                     if args.debug { println!("Loaded custom theme from: {}", path.display()); }
                 }
                 Err(e) => {
                    // Use eprintln for warnings/errors
                    eprintln!("Warning: Failed to load theme from path '{}': {}. Using default.", path.display(), e);
                    // Optionally reset to default theme name if loading failed?
                    // highlight_setting.set_theme("base16-ocean.dark"); // Example reset
                 }
             }
        } else {
            // Assume it's a built-in theme name
            if highlight_setting.get_theme(theme_path_or_name).is_none() {
                 eprintln!("Warning: Theme '{}' not found. Available themes:", theme_path_or_name);
                 list_themes(&highlight_setting);
                 eprintln!("Using default theme: {}", highlight_setting.theme);
            } else {
                 highlight_setting.set_theme(theme_path_or_name);
                 if args.debug { println!("Using built-in theme: {}", theme_path_or_name); }
            }
        }
    }


    if args.list_syntax {
        println!("Supported Syntaxes (Name, Extensions):");
        for syntax in highlight_setting.syntax_set.syntaxes() {
            println!("- {} (.{})",syntax.name, syntax.file_extensions.join(", ."));
        }
        return Ok(()); // Exit after listing
    }

    if args.list_theme {
       list_themes(&highlight_setting);
       return Ok(()); // Exit after listing
    }

    // --- Font and Render Config ---
    // Require font for actual rendering
    let font_name = match args.font {
        Some(f) => f,
        None => {
            // Don't exit if only listing things, but require for rendering
            if args.text.is_none() && args.file.is_none() {
                 return Ok(()); // Nothing to render, maybe just listed things
            }
            return Err(anyhow::anyhow!("--font option is required for rendering"));
        }
    };

    let output_path = args.output.unwrap_or_else(|| PathBuf::from("output.svg"));

    // Create FontConfig
    let mut font_config = FontConfig::new(
        font_name,
        args.size,
        args.fill.clone(), // Clone needed as args might be used later
        args.color.clone(),
        args.debug
    )?;
    font_config.set_letter_space(args.space);

    if args.debug {
        println!("Font Config: {:?}", font_config);
    }

    // Create RenderConfig (for non-highlight mode)
    let mut render_config = RenderConfig::new(args.animate, args.style.unwrap_or(FontStyle::Regular));
    render_config.set_max_width(args.width);


    // --- Rendering Logic ---
    if let Some(text) = args.text {
        if args.highlight {
             eprintln!("Warning: Highlight mode is ignored when providing text directly via argument.");
        }
        println!("Rendering text to {}...", output_path.display());
        render::render_text_to_svg_file(
            &text,
            &mut font_config,
            &render_config,
            output_path,
        );
    } else if let Some(file) = args.file {
        if !file.exists() {
            return Err(anyhow::anyhow!("Input file not found: {}", file.display()));
        }
        if args.highlight {
            println!("Rendering file {} with highlighting to {}...", file.display(), output_path.display());
            render::render_file_highlight(
                &file,
                &mut font_config,
                &highlight_setting, // Pass the configured settings
                output_path,
            );
        } else {
            println!("Rendering file {} as plain text to {}...", file.display(), output_path.display());
            render::render_text_file_to_svg(
                &file,
                &mut font_config,
                &render_config,
                output_path,
            );
        }
    } else {
        // This case should ideally be caught earlier if font wasn't provided,
        // but added for completeness if only flags like --list-fonts were used.
        if !args.list_fonts && !args.list_syntax && !args.list_theme {
             println!("No input text or file provided. Use --text or --file.");
             // Potentially print help here
        }
    }

    Ok(())
}


fn list_themes(settings: &HighlightSetting) {
     println!("Available Themes:");
        for theme_name in settings.theme_set.themes.keys() {
            println!("- {}", theme_name);
        }
}

